#![no_std]
#![no_main]

//! Embassy-based firmware skeleton.
//!
//! Uses `embassy_stm32::executor::Executor` so transparent STOP-mode
//! integration via the `low-power` feature kicks in automatically when
//! all tasks are blocked. STM32L1 support for that path lives on the
//! `stm32l1_low_power` branch of `serdzz/embassy` (pinned via
//! `[patch.crates-io]` in Cargo.toml).

mod drivers;

use core::cell::RefCell;

use defmt::*;
use embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::exti::{self, ExtiInput};
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::mode::Blocking;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::rtc::{
    AnyRtc, DateTime as RtcDateTime, DayOfWeek, Rtc, RtcContainer, RtcTimeProvider,
};
use embassy_stm32::usart::{self, Config as UartConfig, Uart};
use embassy_stm32::{Config, bind_interrupts, interrupt, peripherals};
use embassy_sync::blocking_mutex::NoopMutex;
use embassy_time::Duration;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use drivers::deferred_display::DeferredDisplay;
use drivers::eeprom::Eeprom25Lc1024;
use drivers::tdc1000::Tdc1000;
use drivers::tdc7200::Tdc7200;
use drivers::hd44780::Hd44780;
use drivers::keypad::{keypad_task, ButtonFlags, KeyEvent, KEYS};
use drivers::uart::{MODBUS_FRAMES, SHELL_ACTIONS, UART_TX};
use embassy_futures::select::{Either6, select6};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::{Receiver, Watch};
use embassy_time::{Instant, Timer};
use core::future::pending;
use uflowmeter::calibration::{CalibData, CalibTable, Calculator, MeterConfig};
use uflowmeter::history::RingStorage;
use uflowmeter::modbus_handler::ModbusHandler;
use uflowmeter::ui::MenuController;
use uflowmeter::{App, AppRequest, Options, UiEvent};

// Three retention rings backed by EEPROM, layout chained at compile
// time via SIZE_ON_FLASH so each slot starts right after the previous.
// Sizes match the legacy RTIC build:
//   Hour:  2160 entries × 3600s        = 90 days at 1/h
//   Day:   31×12×3 = 1116 × 86 400s    = 3 years at 1/d
//   Month: 10×12 = 120 × 31×86 400s    = 10 years at 1/m
pub type HourHistory = RingStorage<0, 2160, 3600>;
pub type DayHistory =
    RingStorage<{ HourHistory::SIZE_ON_FLASH }, { 31 * 12 * 3 }, { 3600 * 24 }>;
pub type MonthHistory = RingStorage<
    { HourHistory::SIZE_ON_FLASH + DayHistory::SIZE_ON_FLASH },
    { 10 * 12 },
    { 3600 * 24 * 31 },
>;

/// Once-a-minute tick used to advance accumulators and possibly emit
/// a history-ring write. Single-slot — if a tick is missed because
/// the dispatcher is busy, the next one will catch up via the
/// previous-minute comparison.
static HISTORY_TICK: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

/// Latest computed flow rate (m³/h). measurement_task signals every
/// time a fresh up+down ToF pair lands and the calibration math
/// resolves to a finite value. Signal coalesces — main loop reads
/// the freshest value and updates `app.flow`.
static FLOW_RESULT: Signal<CriticalSectionRawMutex, f32> = Signal::new();

/// Latest Options snapshot. Main loop publishes after every event
/// that can mutate Options (UI Set* dispatches, Modbus writes);
/// measurement_task subscribes and rebuilds its calibration
/// table + MeterConfig whenever `try_changed` fires. Two-slot capacity
/// leaves room for a second consumer (e.g. a future thermal task).
static OPTIONS_WATCH: Watch<CriticalSectionRawMutex, Options, 2> = Watch::new();

#[embassy_executor::task]
async fn history_tick_task() {
    loop {
        embassy_time::Timer::after_secs(60).await;
        let _ = HISTORY_TICK.try_send(());
    }
}

bind_interrupts!(
    pub struct Irqs {
        EXTI0 => exti::InterruptHandler<interrupt::typelevel::EXTI0>;
        EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
        EXTI15_10 => exti::InterruptHandler<interrupt::typelevel::EXTI15_10>;
        USART1 => usart::InterruptHandler<peripherals::USART1>;
        DMA1_CHANNEL4 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH4>;
        DMA1_CHANNEL5 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH5>;
    }
);

#[embassy_executor::main(executor = "embassy_stm32::executor::Executor", entry = "cortex_m_rt::entry")]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.enable_debug_during_sleep = true;
    config.min_stop_pause = Duration::from_millis(10);
    // MSI 2 MHz instead of the default 4 MHz — halves active-phase
    // current draw. UART at 115200 needs ~1.84 MHz min for 16x
    // oversampling and SPI at 1 MHz needs core ≥2 MHz, so this is
    // the lowest safe range for our workload.
    config.rcc.msi = Some(embassy_stm32::rcc::MSIRange::RANGE2M);
    let p = embassy_stm32::init(config);
    info!("uflowmeter (embassy): boot");

    // RTC: low-power Rtc::new takes only the peripheral and returns
    // (container, time_provider). The container holds the Rtc behind
    // a CriticalSection Mutex so other tasks (like embassy's low-power
    // executor) can also reach in. `rtc_now` lets us read DateTime
    // without holding the lock.
    let (rtc_container, rtc_now) = Rtc::new(p.RTC);

    // LCD power (PC0, active-LOW). ON at boot so init runs against a
    // live panel; switched off together with the backlight on idle
    // timeout to drop the HD44780 controller's ~0.5–1 mA idle draw.
    // A re-init runs on the next key press (~50 ms wake budget).
    let mut lcd_power = Output::new(p.PC0, Level::Low, Speed::Low);
    // Backlight OFF at boot — first key press wakes it; idle timeout
    // turns it back off so the device spends most of its life in STOP.
    // Active-LOW: Level::High = off, set_low() = on.
    let mut backlight = Output::new(p.PC5, Level::High, Speed::Low);

    let rs = Output::new(p.PC1, Level::Low, Speed::Low);
    let rw = Output::new(p.PC2, Level::Low, Speed::Low);
    let e = Output::new(p.PC3, Level::Low, Speed::Low);
    let d4 = Output::new(p.PA4, Level::Low, Speed::Low);
    let d5 = Output::new(p.PA5, Level::Low, Speed::Low);
    let d6 = Output::new(p.PA6, Level::Low, Speed::Low);
    let d7 = Output::new(p.PA7, Level::Low, Speed::Low);
    let mut lcd = Hd44780::new(rs, rw, e, d4, d5, d6, d7);
    lcd.init().await;
    info!("lcd init done");

    // SPI2 shared bus: 25LC1024 EEPROM + TDC1000 + TDC7200.
    // SCK=PB13, MISO=PB14, MOSI=PB15. CS pins per src/hardware/pins.rs.
    let mut spi2_cfg = SpiConfig::default();
    spi2_cfg.frequency = Hertz(1_000_000);
    let spi2 = Spi::new_blocking(p.SPI2, p.PB13, p.PB15, p.PB14, spi2_cfg);
    // Forward declaration: keep HOLD/WP high so EEPROM accepts writes.
    let _eeprom_hold = Output::new(p.PC11, Level::High, Speed::Low);
    let _eeprom_wp = Output::new(p.PC12, Level::High, Speed::Low);

    // Static bus so SpiDevice borrows can outlive this scope.
    static SPI2_BUS: StaticCell<
        NoopMutex<RefCell<Spi<'static, Blocking, embassy_stm32::spi::mode::Master>>>,
    > = StaticCell::new();
    let spi2_bus: &_ = SPI2_BUS.init(NoopMutex::new(RefCell::new(spi2)));

    let mut eeprom = Eeprom25Lc1024::new(SpiDevice::new(
        spi2_bus,
        Output::new(p.PC10, Level::High, Speed::VeryHigh),
    ));
    // Load Options from EEPROM at offset 0 (with secondary copy
    // at offset 1024 — handled by Options::load_with_buf).
    let mut opt_buf = [0u8; uflowmeter::options::Options::SIZE];
    let mut options: Options = match Options::load_with_buf(&mut eeprom, &mut opt_buf) {
        Ok(opt) => {
            info!("options: loaded from EEPROM");
            opt
        }
        Err(_) => {
            warn!("options: load failed, using defaults");
            Options::default()
        }
    };
    info!(
        "options serial={=u32} sensor_type={=u8}",
        options.serial_number(),
        options.sensor_type()
    );

    // Publish the boot snapshot before spawning any consumer; subsequent
    // mutations re-publish via `opt_sender.send(options)`.
    let opt_sender = OPTIONS_WATCH.sender();
    opt_sender.send(options);
    let opt_receiver = unwrap!(OPTIONS_WATCH.receiver());

    // Bring up the three history rings — recover existing service
    // metadata from EEPROM where present, fall back to empty state
    // otherwise (also covers the first-power-on case where the EEPROM
    // bytes are uninitialised).
    let mut hour_history = HourHistory::new(&mut eeprom).unwrap_or_else(|_| {
        warn!("hour_history: init failed, starting empty");
        HourHistory {
            data: uflowmeter::history::ServiceData::default(),
        }
    });
    let mut day_history = DayHistory::new(&mut eeprom).unwrap_or_else(|_| {
        warn!("day_history: init failed, starting empty");
        DayHistory {
            data: uflowmeter::history::ServiceData::default(),
        }
    });
    let mut month_history = MonthHistory::new(&mut eeprom).unwrap_or_else(|_| {
        warn!("month_history: init failed, starting empty");
        MonthHistory {
            data: uflowmeter::history::ServiceData::default(),
        }
    });
    info!(
        "history: hour_size={=u32} day_size={=u32} month_size={=u32}",
        hour_history.data.size(),
        day_history.data.size(),
        month_history.data.size()
    );

    // TDC1000 — analog frontend, CS=PB11, EN=PB10, RES=PC6.
    // EN starts LOW so the chip is powered down at boot; measurement_task
    // turns it on for the duration of each ToF cycle and off again
    // when done. RES is active-low so we hold it HIGH (out of reset).
    let tdc1000_en = Output::new(p.PB10, Level::Low, Speed::Low);
    let tdc1000_res = Output::new(p.PC6, Level::High, Speed::Low);
    let tdc1000 = Tdc1000::new(
        SpiDevice::new(spi2_bus, Output::new(p.PB11, Level::High, Speed::VeryHigh)),
        tdc1000_en,
        tdc1000_res,
    );

    // TDC7200 — time-to-digital, CS=PB12, EN=PB1. EN starts LOW
    // (powered down); measurement_task brings it up per-cycle.
    let tdc7200_en = Output::new(p.PB1, Level::Low, Speed::Low);
    let tdc7200 = Tdc7200::new(
        SpiDevice::new(spi2_bus, Output::new(p.PB12, Level::High, Speed::VeryHigh)),
        tdc7200_en,
    );
    // Config is loaded inside measurement_task on every cycle (after
    // power_on), so we don't try to write registers here while EN=low.

    // TDC7200 INT line (active-LOW) on PB0 → wakes us at end of
    // measurement via EXTI0.
    let tdc_int = ExtiInput::new(p.PB0, p.EXTI0, Pull::Up, Irqs);

    // Spawn the measurement loop — runs forever, triggers a TDC1000
    // TX pulse + TDC7200 measurement once per second, logs the raw
    // 24-bit ToF.
    spawner.spawn(unwrap!(measurement_task(
        tdc1000,
        tdc7200,
        tdc_int,
        opt_receiver
    )));

    // USART1: TX=PA9, RX=PA10. 115200 baud (legacy default).
    // PC9 (RsPowerEn, active-LOW) powers the RS485 transceiver.
    let _rs_power = Output::new(p.PC9, Level::Low, Speed::Low);
    // USART1 stays uninitialised at boot. `uart_session_task` brings
    // it up on demand (EXTI on PA10 wakes us from STOP for the first
    // incoming start bit; for unsolicited TX we wake from UART_TX
    // channel push), then tears it down after SESSION_IDLE_TIMEOUT
    // of quiet so REFCOUNT_STOP1 drops and the executor sleeps.
    spawner.spawn(unwrap!(drivers::uart::uart_session_task(
        p.USART1,
        p.PA10,
        p.PA9,
        p.DMA1_CH4,
        p.DMA1_CH5,
        p.EXTI10,
    )));
    spawner.spawn(unwrap!(drivers::uart::shell_task()));
    spawner.spawn(unwrap!(history_tick_task()));

    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);
    spawner.spawn(unwrap!(keypad_task(btn_config, btn_enter, btn_down, btn_up)));

    let mut app = App::new();
    sync_app_datetime(&mut app, &rtc_now);
    load_uptime_from_backup(&mut app, &rtc_container);
    let mut ui = MenuController::new();
    // Sync-fill / async-flush adapter: ui.render writes ops into the
    // buffer in microseconds, then flush().await streams them out to
    // the HD44780 between executor yields.
    let mut frame = DeferredDisplay::new();

    // Modbus dispatcher — slave address from Options at boot. The
    // handler is stateless aside from the address; we hold it across
    // iterations so updating the address via UI takes effect on the
    // next request.
    let mut modbus = ModbusHandler::new(options.slave_address());

    // Idle timeout: backlight + LCD power both go off `IDLE_TIMEOUT`
    // after the last key press. `idle_deadline = None` means "no UI
    // session active" → no timer armed → executor is free to enter STOP.
    const IDLE_TIMEOUT: embassy_time::Duration = embassy_time::Duration::from_secs(15);
    let mut idle_deadline: Option<Instant> = None;
    // Whether the HD44780 currently holds valid state. Cleared when
    // we cut LCD power; the next key press re-runs lcd.init() before
    // the first render.
    let mut lcd_initialized = true;

    loop {
        // Build the idle-timeout future. When no session is active we
        // park on `pending()` (never resolves) so the select arm is
        // inert and no sub-IDLE_TIMEOUT alarm sits in the time driver.
        let idle_fut = async {
            match idle_deadline {
                Some(deadline) => Timer::at(deadline).await,
                None => pending::<()>().await,
            }
        };

        match select6(
            KEYS.receive(),
            MODBUS_FRAMES.receive(),
            HISTORY_TICK.receive(),
            FLOW_RESULT.wait(),
            SHELL_ACTIONS.receive(),
            idle_fut,
        )
        .await
        {
            Either6::First(KeyEvent::Pressed(flag)) => {
                // Any key wakes the LCD (if not already lit) and resets
                // the idle countdown. If the panel was powered down on
                // the previous idle, bring it back up before the first
                // render this iteration.
                if !lcd_initialized {
                    lcd_power.set_low();
                    embassy_time::Timer::after_millis(50).await;
                    lcd.init().await;
                    lcd_initialized = true;
                }
                backlight.set_low();
                idle_deadline = Some(Instant::now() + IDLE_TIMEOUT);

                let ui_event = if flag.contains(ButtonFlags::ENTER) {
                    Some(UiEvent::Enter)
                } else if flag.contains(ButtonFlags::CONFIG) {
                    Some(UiEvent::Back)
                } else if flag.contains(ButtonFlags::DOWN) {
                    // Match legacy keyboard.rs: hardware Down → Left,
                    // Up → Right (inverted vs. visual intuition — the
                    // UI convention is pre-existing).
                    Some(UiEvent::Left)
                } else if flag.contains(ButtonFlags::UP) {
                    Some(UiEvent::Right)
                } else {
                    None
                };

                if let Some(e) = ui_event {
                    if let Some(req) = ui.event(e, &app) {
                        handle_app_request(
                            req,
                            &mut backlight,
                            &mut options,
                            &mut eeprom,
                            &mut opt_buf,
                            &rtc_container,
                        );
                        // Slave address may have changed.
                        modbus.modbus_mut().set_slave_address(options.slave_address());
                        // Republish so measurement_task picks up the
                        // new calibration on its next cycle.
                        opt_sender.send(options);
                    }
                }
            }
            Either6::Second(frame_bytes) => {
                handle_modbus_frame(
                    &modbus,
                    &frame_bytes,
                    &mut options,
                    &mut eeprom,
                    &mut hour_history,
                    &mut day_history,
                    &mut month_history,
                    &app,
                );
                // Modbus writes mutate Options in-place; republish so
                // the measurement task sees the change.
                opt_sender.send(options);
            }
            Either6::Third(()) => {
                handle_history_tick(
                    &mut app,
                    &rtc_now,
                    &mut eeprom,
                    &mut hour_history,
                    &mut day_history,
                    &mut month_history,
                    &rtc_container,
                );
            }
            Either6::Fourth(volume) => {
                app.flow = volume;
            }
            Either6::Fifth(action) => {
                handle_shell_action(
                    action,
                    &mut options,
                    &mut eeprom,
                    &mut opt_buf,
                    &rtc_container,
                );
                // Shell may have mutated Options (set_serial); republish
                // so measurement_task picks it up.
                opt_sender.send(options);
            }
            Either6::Sixth(()) => {
                defmt::info!("idle: backlight + LCD power off");
                backlight.set_high();
                lcd_power.set_high();
                lcd_initialized = false;
                idle_deadline = None;
                continue;
            }
        }

        sync_app_datetime(&mut app, &rtc_now);

        // Skip the LCD re-render when the panel is dark / powered
        // down — nothing visible, and skipping saves a flush().await
        // that would otherwise drive the (now power-gated) HD44780.
        if idle_deadline.is_some() && lcd_initialized {
            ui.update(&app);
            ui.render(&app, &mut frame);
            frame.flush(&mut lcd).await;
        }
    }
}

/// Dispatch a freshly-framed Modbus RTU request: parse, build reply
/// (or exception), push the reply onto UART_TX so uart_task writes it
/// out on its next iteration. Silently drops requests not addressed
/// to us — that's the InvalidSlaveAddress path.
#[allow(clippy::too_many_arguments)]
fn handle_modbus_frame(
    modbus: &ModbusHandler,
    frame: &[u8],
    options: &mut Options,
    eeprom: &mut Eeprom,
    hour_history: &mut HourHistory,
    day_history: &mut DayHistory,
    month_history: &mut MonthHistory,
    app: &App,
) {
    let result = modbus.handle_request(
        frame,
        options,
        eeprom,
        app.flow,
        app.hour_flow,
        app.day_flow,
        app.month_flow,
        hour_history,
        day_history,
        month_history,
    );
    match result {
        Ok(reply) => {
            if UART_TX.try_send(reply).is_err() {
                defmt::warn!("modbus: UART_TX queue full, reply dropped");
            }
        }
        Err(uflowmeter::modbus::ModbusError::InvalidSlaveAddress)
        | Err(uflowmeter::modbus::ModbusError::InvalidLength)
        | Err(uflowmeter::modbus::ModbusError::InvalidCrc) => {
            // Not for us / shell text / line noise — silent.
        }
        Err(e) => defmt::warn!("modbus: handler error {:?}", defmt::Debug2Format(&e)),
    }
}

type Eeprom = drivers::eeprom::Eeprom25Lc1024<
    embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice<
        'static,
        embassy_sync::blocking_mutex::raw::NoopRawMutex,
        embassy_stm32::spi::Spi<'static, Blocking, embassy_stm32::spi::mode::Master>,
        Output<'static>,
    >,
>;

/// AppRequest dispatcher. Options-mutating variants update the
/// in-memory `options` then persist the whole blob to EEPROM via
/// `Options::save_with_buf` (dual-page CRC layout).
fn handle_app_request(
    req: AppRequest,
    backlight: &mut Output<'static>,
    options: &mut Options,
    eeprom: &mut Eeprom,
    buf: &mut [u8; uflowmeter::options::Options::SIZE],
    rtc: &RtcContainer,
) {
    match req {
        AppRequest::SystemReset => {
            defmt::info!("AppRequest::SystemReset");
            // Save options before reset — matches legacy behavior.
            let _ = options.save_with_buf(eeprom, buf);
            cortex_m::peripheral::SCB::sys_reset();
        }
        AppRequest::LcdLed(on) => {
            // Active-LOW backlight: on → set_low, off → set_high.
            if on {
                backlight.set_low();
            } else {
                backlight.set_high();
            }
        }
        AppRequest::DeepSleep => {
            // No-op — embassy's executor enters STOP automatically
            // when all tasks idle (see embassy-stm32 low_power feature).
        }
        AppRequest::Process => {
            // Trigger TDC measurement once the full TDC1000/7200
            // orchestration is ported. For now just log.
            defmt::trace!("AppRequest::Process (unimplemented)");
        }
        AppRequest::SetDateTime(dt) => {
            set_rtc_datetime(rtc, dt);
        }
        AppRequest::SetHistory(_, _) => {
            defmt::trace!("AppRequest::SetHistory (unimplemented)");
        }
        AppRequest::SetCommType(v) => {
            options.set_comm_type(v);
            persist_options(options, eeprom, buf);
        }
        AppRequest::SetAddress(v) => {
            options.set_slave_address(v);
            persist_options(options, eeprom, buf);
        }
        AppRequest::SetNegative(on) => {
            options.set_enable_negative(on as u8);
            persist_options(options, eeprom, buf);
        }
        AppRequest::SetMuster(_) => {
            // Muster is UI-only (EditBoxState) in the legacy code —
            // doesn't have a dedicated Options field. No-op.
            defmt::trace!("AppRequest::SetMuster — UI-only, not persisted");
        }
        AppRequest::ExitShell | AppRequest::EnterCalibration => {
            defmt::trace!("AppRequest::Exit/Enter (no-op)");
        }
    }
}

fn persist_options(
    options: &mut Options,
    eeprom: &mut Eeprom,
    buf: &mut [u8; uflowmeter::options::Options::SIZE],
) {
    match options.save_with_buf(eeprom, buf) {
        Ok(()) => defmt::info!("options: saved to EEPROM"),
        Err(_) => defmt::error!("options: save failed"),
    }
}

/// Apply a shell-originated side-effect. Mirrors handle_app_request
/// for the subset of commands that have observable effects on the
/// device state (RTC, Options). Verbose toggle is a stub for now —
/// defmt verbosity is set at compile time via DEFMT_LOG.
fn handle_shell_action(
    action: uflowmeter::shell::ShellAction,
    options: &mut Options,
    eeprom: &mut Eeprom,
    buf: &mut [u8; uflowmeter::options::Options::SIZE],
    rtc: &RtcContainer,
) {
    use uflowmeter::shell::ShellAction;
    match action {
        ShellAction::SetDateUnix(ts) => {
            let odt = time::OffsetDateTime::from_unix_timestamp(ts as i64).ok();
            match odt {
                Some(d) => {
                    let pdt = time::PrimitiveDateTime::new(d.date(), d.time());
                    defmt::info!("shell: set RTC from unix={=u32}", ts);
                    set_rtc_datetime(rtc, pdt);
                }
                None => defmt::warn!("shell: bad unix ts {=u32}", ts),
            }
        }
        ShellAction::SetSerial(n) => {
            options.set_serial_number(n);
            persist_options(options, eeprom, buf);
            defmt::info!("shell: serial set to {=u32}", n);
        }
        ShellAction::SetVerbose(on) => {
            defmt::info!(
                "shell: verbose toggled to {} (compile-time DEFMT_LOG actually controls this)",
                on
            );
        }
    }
}

/// Push a `time::PrimitiveDateTime` (what the UI hands us) into the
/// embassy RTC. Goes through `critical_section::with` because under
/// the low-power feature the Rtc lives inside a CS Mutex shared with
/// embassy's executor.
fn set_rtc_datetime(rtc: &RtcContainer, dt: time::PrimitiveDateTime) {
    let dow = match dt.weekday() {
        time::Weekday::Monday => DayOfWeek::Monday,
        time::Weekday::Tuesday => DayOfWeek::Tuesday,
        time::Weekday::Wednesday => DayOfWeek::Wednesday,
        time::Weekday::Thursday => DayOfWeek::Thursday,
        time::Weekday::Friday => DayOfWeek::Friday,
        time::Weekday::Saturday => DayOfWeek::Saturday,
        time::Weekday::Sunday => DayOfWeek::Sunday,
    };
    let rtc_dt = match RtcDateTime::from(
        dt.year() as u16,
        dt.month() as u8,
        dt.day(),
        dow,
        dt.hour(),
        dt.minute(),
        dt.second(),
        0,
    ) {
        Ok(v) => v,
        Err(_) => {
            defmt::error!("SetDateTime: invalid components");
            return;
        }
    };
    critical_section::with(|cs| {
        let mut borrow = rtc.borrow_mut(cs);
        match borrow.set_datetime(rtc_dt) {
            Ok(()) => defmt::info!("RTC datetime updated"),
            Err(_) => defmt::error!("RTC set_datetime failed"),
        }
    });
}

/// Background measurement loop: alternate TDC1000 channels (downstream
/// / upstream), trigger a single ToF measurement, wait for TDC7200
/// INT, read TIME1, log. First-pass — no flow-velocity calculation,
/// no app.flow update yet.
type Tdc1000Dev = drivers::tdc1000::Tdc1000<
    'static,
    embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice<
        'static,
        embassy_sync::blocking_mutex::raw::NoopRawMutex,
        embassy_stm32::spi::Spi<'static, Blocking, embassy_stm32::spi::mode::Master>,
        Output<'static>,
    >,
>;
type Tdc7200Dev = drivers::tdc7200::Tdc7200<
    'static,
    embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice<
        'static,
        embassy_sync::blocking_mutex::raw::NoopRawMutex,
        embassy_stm32::spi::Spi<'static, Blocking, embassy_stm32::spi::mode::Master>,
        Output<'static>,
    >,
>;

/// Run a single TDC measurement on the currently selected TDC1000
/// channel and return the raw 24-bit TIME1 value. Returns `None` on
/// SPI error or INT timeout. The 50 ms timeout matches the legacy
/// budget — at 1480 m/s and 100 mm transducer spacing a real reading
/// is < 100 µs, so anything over 50 ms is a stuck transducer.
async fn single_measurement(
    tdc7200: &mut Tdc7200Dev,
    tdc_int: &mut ExtiInput<'static, embassy_stm32::mode::Async>,
) -> Option<u32> {
    if tdc7200.start_measurement().is_err() {
        defmt::error!("tdc7200 start failed");
        return None;
    }
    match embassy_time::with_timeout(
        embassy_time::Duration::from_millis(50),
        tdc_int.wait_for_falling_edge(),
    )
    .await
    {
        Ok(()) => tdc7200.read_time1().ok(),
        Err(_) => None,
    }
}

/// Background measurement loop: for each cycle, takes a downstream
/// and an upstream ToF reading, runs the result through the
/// `calibration::Calculator`, and signals the resulting flow (m³/h)
/// to the main loop. Calibration (table + MeterConfig) is rebuilt
/// whenever the main loop publishes new Options via OPTIONS_WATCH,
/// so menu / Modbus tweaks take effect on the next cycle without a
/// reboot.
#[embassy_executor::task]
async fn measurement_task(
    mut tdc1000: Tdc1000Dev,
    mut tdc7200: Tdc7200Dev,
    mut tdc_int: ExtiInput<'static, embassy_stm32::mode::Async>,
    mut opt_rx: Receiver<'static, CriticalSectionRawMutex, Options, 2>,
) {
    // Pull the initial snapshot (the main loop publishes before
    // spawning us, so this should always be Some).
    let mut options = opt_rx.try_get().unwrap_or_default();
    let mut table = options_to_calib_table(&options);
    let mut calc = Calculator::new(options_to_meter_config(&options));

    loop {
        embassy_time::Timer::after_secs(5).await;

        // Pick up any live calibration changes before kicking off the
        // next measurement pair.
        if let Some(new_opts) = opt_rx.try_changed() {
            options = new_opts;
            table = options_to_calib_table(&options);
            calc = Calculator::new(options_to_meter_config(&options));
            defmt::info!("measurement: calibration refreshed from OPTIONS_WATCH");
        }

        // Bring both TDC chips up for this cycle. ~1 ms regulator
        // settle is generous for these parts. Re-load config because
        // the chips lose all register state when EN was low.
        tdc1000.power_on();
        tdc7200.power_on();
        embassy_time::Timer::after_millis(1).await;
        let tdc1000_regs = options.tdc1000_regs().to_le_bytes();
        if tdc1000.load_config(&tdc1000_regs[..10]).is_err() {
            defmt::warn!("tdc1000: load_config failed");
        }
        let tdc7200_regs = options.tdc7200_regs().to_le_bytes();
        if tdc7200.load_config(&tdc7200_regs[..10]).is_err() {
            defmt::warn!("tdc7200: load_config failed");
        }
        let _ = tdc1000.clear_error_flags();

        // Downstream first (channel 0).
        let _ = tdc1000.set_channel(false);
        let tof_down = single_measurement(&mut tdc7200, &mut tdc_int).await;

        // Upstream (channel 1).
        let _ = tdc1000.set_channel(true);
        let tof_up = single_measurement(&mut tdc7200, &mut tdc_int).await;

        // Cut chip power before processing the result — calc/log are
        // pure CPU work and don't need the analog frontend.
        tdc1000.power_off();
        tdc7200.power_off();

        match (tof_down, tof_up) {
            (Some(d), Some(u)) => {
                let volume = calc.get_volume(&table, u as f32, d as f32);
                if volume.is_finite() {
                    defmt::info!(
                        "tof down={=u32} up={=u32} flow={=f32} m³/h",
                        d,
                        u,
                        volume
                    );
                    FLOW_RESULT.signal(volume);
                }
            }
            _ => defmt::warn!("tof: down={} up={}", tof_down.is_some(), tof_up.is_some()),
        }
    }
}

/// Build the geometry/limit config used by the flow Calculator from
/// Options. Only `const_val` is persisted; everything else takes
/// MeterConfig::default() values (tof_min/max bounds, vmin/vmax).
/// When `const_val` is zero (uncalibrated / pre-existing EEPROM
/// without the field), the calculator returns 0 — matches legacy
/// behavior so a stale flash doesn't produce phantom flow.
fn options_to_meter_config(options: &Options) -> MeterConfig {
    MeterConfig {
        const_val: f32::from_bits(options.const_val()),
        ..MeterConfig::default()
    }
}

/// Pull the calibration table for channel 1 out of Options. Float
/// fields are stored as raw u32 bits via the bitfield's B32 slots —
/// `f32::from_bits` recovers them. Matches the C++ layout.
fn options_to_calib_table(options: &Options) -> CalibTable {
    CalibTable {
        dtof0: f32::from_bits(options.zero1()),
        data: [
            CalibData {
                v: f32::from_bits(options.v11()),
                k: f32::from_bits(options.k11()),
            },
            CalibData {
                v: f32::from_bits(options.v12()),
                k: f32::from_bits(options.k12()),
            },
            CalibData {
                v: f32::from_bits(options.v13()),
                k: f32::from_bits(options.k13()),
            },
        ],
    }
}

/// RTC backup register slots (BKP0..BKP15 on STM32L1).
/// BKP0 — cumulative uptime in seconds across resets (as long as
/// VBAT is present).
/// BKP1 — last unix-timestamp seen by the uptime tracker, used to
/// compute real elapsed time between ticks.
const BKP_UPTIME_SECONDS: usize = 0;
const BKP_LAST_UPTIME_RTC: usize = 1;

/// Load the persisted uptime from backup registers into `app` at
/// boot. Called once before the main loop starts.
fn load_uptime_from_backup(app: &mut App, rtc: &RtcContainer) {
    app.uptime_seconds = rtc.read_backup_register(BKP_UPTIME_SECONDS).unwrap_or(0);
    app.last_uptime_rtc = rtc.read_backup_register(BKP_LAST_UPTIME_RTC).unwrap_or(0);
    defmt::info!(
        "uptime: loaded from BKP — total={=u32} s, last_ts={=u32}",
        app.uptime_seconds,
        app.last_uptime_rtc
    );
}

/// Tick every ~60 s: accumulate the current flow reading into the
/// hour/day/month float counters, advance the persisted uptime in
/// RTC backup registers, and when the local datetime crosses a
/// boundary (minute=0 for hour, hour=0 for day, day=1 for month)
/// flush the accumulator into the corresponding ring and reset it.
/// Mirrors the legacy `AppRequest::Process` callback minus the TDC
/// trigger (measurement is its own task now).
fn handle_history_tick(
    app: &mut App,
    rtc: &RtcTimeProvider,
    eeprom: &mut Eeprom,
    hour_history: &mut HourHistory,
    day_history: &mut DayHistory,
    month_history: &mut MonthHistory,
    rtc_container: &RtcContainer,
) {
    let dt = match rtc.now() {
        Ok(v) => v,
        Err(_) => {
            defmt::warn!("history tick: RTC not ready, skipping");
            return;
        }
    };

    // Accumulate (currently flow is 0.0 until measurement_task starts
    // writing it — keeps the ring layout exercised regardless).
    app.hour_flow += app.flow;
    app.day_flow += app.flow;
    app.month_flow += app.flow;

    // Build a unix timestamp from the RTC reading. The conversion
    // mirrors sync_app_datetime — bail out silently if either part
    // doesn't compute (post-VBAT-loss state).
    let (date, time_of_day) = match (
        time::Date::from_calendar_date(
            dt.year() as i32,
            time::Month::try_from(dt.month()).unwrap_or(time::Month::January),
            dt.day(),
        ),
        time::Time::from_hms(dt.hour(), dt.minute(), dt.second()),
    ) {
        (Ok(d), Ok(t)) => (d, t),
        _ => return,
    };
    let pdt = time::PrimitiveDateTime::new(date, time_of_day);
    let ts = pdt.assume_utc().unix_timestamp() as u32;

    // Uptime tracking: how many seconds have we *actually* been awake
    // since the last tick? Use the RTC delta (not a fixed 60 s) so a
    // missed/late tick doesn't under- or over-count. Anchor on first
    // tick after boot when last_uptime_rtc is still zero from cold
    // VBAT or never-initialised backup state.
    if app.last_uptime_rtc != 0 && ts > app.last_uptime_rtc {
        let delta = ts - app.last_uptime_rtc;
        // Clamp absurd deltas (clock jumps from `date set`, etc.) so a
        // single bad sample can't wreck the counter.
        if delta < 86_400 {
            app.uptime_seconds = app.uptime_seconds.saturating_add(delta);
        }
    }
    app.last_uptime_rtc = ts;
    rtc_container.write_backup_register(BKP_UPTIME_SECONDS, app.uptime_seconds);
    rtc_container.write_backup_register(BKP_LAST_UPTIME_RTC, app.last_uptime_rtc);

    if dt.minute() == 0 {
        match hour_history.add(eeprom, app.hour_flow as i32, ts) {
            Ok(()) => {
                defmt::info!("hour_history.add({=f32}, {=u32})", app.hour_flow, ts);
                app.hour_flow = 0.0;
            }
            Err(_) => defmt::error!("hour_history.add failed"),
        }

        if dt.hour() == 0 {
            match day_history.add(eeprom, app.day_flow as i32, ts) {
                Ok(()) => {
                    defmt::info!("day_history.add({=f32}, {=u32})", app.day_flow, ts);
                    app.day_flow = 0.0;
                }
                Err(_) => defmt::error!("day_history.add failed"),
            }

            if dt.day() == 1 {
                match month_history.add(eeprom, app.month_flow as i32, ts) {
                    Ok(()) => {
                        defmt::info!("month_history.add({=f32}, {=u32})", app.month_flow, ts);
                        app.month_flow = 0.0;
                    }
                    Err(_) => defmt::error!("month_history.add failed"),
                }
            }
        }
    }
}

/// Pull the current datetime from the STM32 RTC and convert it into
/// the `time::PrimitiveDateTime` that App / ui.rs expects. Silently
/// keeps the prior value if the RTC read or conversion fails — most
/// commonly that means VBAT was lost and the RTC is in its post-reset
/// default state, which has no valid weekday.
fn sync_app_datetime(app: &mut App, rtc: &RtcTimeProvider) {
    if let Ok(dt) = rtc.now() {
        if let (Ok(month), Ok(date), Ok(time)) = (
            time::Month::try_from(dt.month()),
            time::Date::from_calendar_date(dt.year() as i32, time::Month::try_from(dt.month()).unwrap_or(time::Month::January), dt.day()),
            time::Time::from_hms(dt.hour(), dt.minute(), dt.second()),
        ) {
            let _ = month;
            app.datetime = time::PrimitiveDateTime::new(date, time);
        }
    }
}
