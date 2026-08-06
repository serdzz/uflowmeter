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
use embassy_stm32::rtc::{
    AnyRtc, DateTime as RtcDateTime, DayOfWeek, Rtc, RtcContainer, RtcTimeProvider,
};
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart;
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_stm32::{bind_interrupts, interrupt, peripherals, Config};
use embassy_sync::blocking_mutex::NoopMutex;
use embassy_time::Duration;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use core::future::pending;
use drivers::deferred_display::DeferredDisplay;
use drivers::eeprom::Eeprom25Lc1024;
use drivers::hd44780::Hd44780;
use drivers::keypad::{keypad_task, ButtonFlags, KeyEvent, KEYS};
use drivers::sensor_mux::{self as mux, SensorMux};
use drivers::tdc1000::Tdc1000;
use drivers::tdc7200::Tdc7200;
use drivers::uart::{MODBUS_FRAMES, SHELL_ACTIONS, UART_TX};
use embassy_futures::select::{select6, Either6};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::{Receiver, Watch};
use embassy_time::{Instant, Timer};
use uflowmeter::average_lib::AverageBuffer;
use uflowmeter::calibration::{Calculator, CalibData, CalibTable, MeterConfig};
use uflowmeter::history::RingStorage;
use uflowmeter::modbus_handler::ModbusHandler;
use uflowmeter::tdc_lib::{average_stops, decode_tof, MAX_STOPS};
use uflowmeter::ui::MenuController;
use uflowmeter::{App, AppRequest, CommType, Options, UiEvent};

// Three retention rings backed by EEPROM, layout chained at compile
// time via SIZE_ON_FLASH so each slot starts right after the previous.
// Sizes match the legacy RTIC build:
//   Hour:  2160 entries × 3600s        = 90 days at 1/h
//   Day:   31×12×3 = 1116 × 86 400s    = 3 years at 1/d
//   Month: 10×12 = 120 × 31×86 400s    = 10 years at 1/m
pub type HourHistory = RingStorage<0, 2160, 3600>;
pub type DayHistory = RingStorage<{ HourHistory::SIZE_ON_FLASH }, { 31 * 12 * 3 }, { 3600 * 24 }>;
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

#[embassy_executor::main(
    executor = "embassy_stm32::executor::Executor",
    entry = "cortex_m_rt::entry"
)]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.enable_debug_during_sleep = true;
    // Minimum lead time to the next timer event for the low-power
    // executor to bother entering STOP. Must stay > TDC_INT_TIMEOUT:
    // while `single_measurement` is parked on that timeout the only
    // pending alarm is 50 ms out, so a smaller threshold lets the
    // executor STOP mid-measurement. STOP gates MSI/HSI/*and HSE*, so
    // the 8 MHz reference we drive out on MCO/PA8 dies right when the
    // TDC7200 is counting — showing up as sporadic `tof: down=false
    // up=false` rather than as an outright fault. The 5 s measurement
    // period and the 15 s UI idle timeout are both far beyond this, so
    // raising it costs no STOP opportunities.
    config.min_stop_pause = Duration::from_millis(100);
    // Default MSI 4 MHz — tried RANGE2M but average current rose
    // from 4.5 to 7.1 mA on the bench unit. Slower core stretches
    // every wake task → more time awake → fewer STOP entries because
    // `time_until_next_alarm < min_stop_pause` fires more often.

    // Enable the 8 MHz HSE crystal so we can drive it out on the
    // MCO pin (PA8) as the TDC1000 / TDC7200 reference clock.
    // SYSCLK stays on MSI; HSE is only used as the MCO source.
    // (Legacy RTIC code declared the crystal as 24 MHz, but the
    // L1 MCO prescaler only does ÷1/2/4/8/16 — there's no ÷3, so
    // the 8 MHz the TDC chips actually expect can only come from
    // an 8 MHz crystal at /1.)
    config.rcc.hse = Some(embassy_stm32::rcc::Hse {
        freq: embassy_stm32::time::Hertz(8_000_000),
        mode: embassy_stm32::rcc::HseMode::Oscillator,
    });
    let p = embassy_stm32::init(config);
    info!("uflowmeter (embassy): boot");

    // MCO out on PA8 → TDC1000 / TDC7200 CLOCK pins. HSE source, /1
    // prescaler = 8 MHz on the pin. The `Mco` instance just needs
    // to stay alive (its drop would tristate PA8); shove into `_mco`
    // so the AF stays driven for the lifetime of `main`.
    let _mco = embassy_stm32::rcc::Mco::new(
        p.MCO,
        p.PA8,
        embassy_stm32::rcc::McoSource::HSE,
        embassy_stm32::rcc::McoConfig::default(),
    );

    // OSC_EN (PA11) — gates the reference oscillator that feeds the
    // TDC1000 / TDC7200 CLOCK pins. The legacy firmware configured it
    // as a push-pull output with ODR at its reset value, i.e. driven
    // LOW (`src/hardware/pins.rs:164,219`), and PA11 sits in the same
    // pull-up group as every other active-low enable on this board
    // (LCD power PC0, backlight PC5, RS485 power PC9, both CS lines) —
    // so LOW means "oscillator on". The embassy port never touched
    // this pin, leaving it a floating input after reset, which is the
    // most likely reason both TDCs read back as all-zero registers.
    let _osc_en = Output::new(p.PA11, Level::Low, Speed::Low);

    // Transducer-pair multiplexer (PB3 enable, PB4/PB5 address).
    // Starts parked in `Off`, matching the C++ `SensorMux::init()`.
    let sensor_mux = SensorMux::new(
        Output::new(p.PB3, Level::Low, Speed::Low),
        Output::new(p.PB4, Level::Low, Speed::Low),
        Output::new(p.PB5, Level::Low, Speed::Low),
    );

    // Independent watchdog. Created here so the peripheral is claimed
    // up front, but only started inside measurement_task.
    let watchdog = IndependentWatchdog::new(p.IWDG, WATCHDOG_TIMEOUT_US);

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
    // EN starts LOW; measurement_task raises it once at start-up.
    // RES starts LOW — that is the chip's running level, not an
    // "in reset" state. `Tdc1000::reset()` pulses it high and drops it
    // back before the first register access, mirroring the C++.
    let tdc1000_en = Output::new(p.PB10, Level::Low, Speed::Low);
    let tdc1000_res = Output::new(p.PC6, Level::Low, Speed::Low);
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
    // TX pulse + TDC7200 measurement every 5 s across both transducer
    // pairs, and pets the watchdog on each cycle.
    spawner.spawn(unwrap!(measurement_task(
        tdc1000,
        tdc7200,
        tdc_int,
        opt_receiver,
        sensor_mux,
        watchdog
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
        p.USART1, p.PA10, p.PA9, p.DMA1_CH4, p.DMA1_CH5, p.EXTI10, p.FLASH,
    )));
    spawner.spawn(unwrap!(drivers::uart::shell_task()));
    spawner.spawn(unwrap!(history_tick_task()));

    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);
    spawner.spawn(unwrap!(keypad_task(
        btn_config, btn_enter, btn_down, btn_up
    )));

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
    // Counts 60 s history ticks toward the next M-Bus broadcast.
    let mut mbus_ticks: u8 = 0;

    loop {
        // Build the idle-timeout future. When no session is active we
        // park on `pending()` (never resolves) so the select arm is
        // inert and no sub-IDLE_TIMEOUT alarm sits in the time driver.
        // A visible screen has to be redrawn on a timer, not just on key
        // presses. Two reasons: the clock on the DateTime screen only
        // advances if something re-reads the RTC and redraws, and the
        // blink animation advances one frame per render, so without a
        // tick the edited field freezes in whichever half of its cycle
        // the last key press left it.
        //
        // Editing wants the faster rate — the blink period in ui.rs is
        // six frames, so BLINK_FRAME gives ~300 ms on / ~300 ms off.
        // Otherwise LIVE_REFRESH is enough to keep seconds ticking
        // without spending a fifth of the session redrawing.
        let refresh_deadline = idle_deadline.map(|_| {
            Instant::now()
                + if ui.is_editing() {
                    BLINK_FRAME
                } else {
                    LIVE_REFRESH
                }
        });
        // One timer arm serves both deadlines — select6 has no room for a
        // seventh — so wake at whichever comes first and work out which
        // it was afterwards.
        let wake_at = match (idle_deadline, refresh_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, None) => a,
            (None, b) => b,
        };
        let idle_fut = async {
            match wake_at {
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
                        modbus
                            .modbus_mut()
                            .set_slave_address(options.slave_address());
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
                // M-Bus is a broadcast-only slave: no request/response,
                // just a datagram every MBUS_PERIOD. Counted off the
                // 60 s history tick rather than a timer of its own so
                // the idle path keeps exactly one alarm armed.
                mbus_ticks += 1;
                if mbus_ticks >= MBUS_PERIOD_TICKS {
                    mbus_ticks = 0;
                    if CommType::from_u8(options.comm_type()) == CommType::MBus {
                        send_mbus_datagram(&options, &app);
                    }
                }
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
                // A refresh tick rather than the idle timeout: fall through
                // to the bottom of the loop, which re-reads the RTC and
                // then redraws. Handling it here instead would skip that
                // sync and leave the clock frozen while the frame updated.
                if idle_deadline.is_some_and(|d| Instant::now() < d) {
                    // nothing to do — the shared tail does the work
                } else {
                    defmt::info!("idle: backlight + LCD power off");
                    backlight.set_high();
                    // Park the data/control lines before cutting the supply
                    // — see Hd44780::park.
                    lcd.park();
                    lcd_power.set_high();
                    lcd_initialized = false;
                    idle_deadline = None;
                    continue;
                }
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

/// Queue one 4-20 mA loop frame: `AA 55` then the DAC count, little
/// endian. Consumption is passed to `to_analog` in litres per hour,
/// matching how the C++ stores its immediate value.
fn send_analog_frame(calc: &Calculator, flow_m3ph: f32) {
    let count = calc.to_analog((flow_m3ph * 1000.0) as i32) as u16;
    let mut frame = drivers::uart::ModbusFrame::new();
    if frame.extend_from_slice(&[0xAA, 0x55]).is_err()
        || frame.extend_from_slice(&count.to_le_bytes()).is_err()
    {
        defmt::warn!("analog: frame buffer too small");
        return;
    }
    if UART_TX.try_send(frame).is_err() {
        defmt::warn!("analog: UART_TX queue full, frame dropped");
    }
}

/// Build and queue one M-Bus RSP_UD datagram. Broadcast-only: nothing
/// is expected back, so a full TX queue just means the line is busy
/// and we drop this round rather than stalling the main loop.
fn send_mbus_datagram(options: &Options, app: &App) {
    let frame = uflowmeter::mbus::build_datagram(
        options.slave_address(),
        options.serial_number(),
        app.month_flow,
        app.flow,
        app.uptime_seconds / 60,
    );
    let mut out = drivers::uart::ModbusFrame::new();
    if out.extend_from_slice(&frame).is_err() {
        defmt::warn!("mbus: datagram larger than the TX frame buffer, dropped");
        return;
    }
    if UART_TX.try_send(out).is_err() {
        defmt::warn!("mbus: UART_TX queue full, datagram dropped");
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
        // Handled entirely inside the UART session, which owns the line
        // and the flash peripheral for the duration, and resets the
        // device on success. Reaching here would mean the session
        // forwarded it by mistake.
        ShellAction::FirmwareUpdate => {
            defmt::warn!("shell: FirmwareUpdate reached the main loop, ignoring")
        }
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
        ShellAction::Upload(kind) => {
            // The UART session performs the transfer itself; nothing
            // reaches here until it has the bytes.
            defmt::info!(
                "shell: awaiting {=usize} B upload",
                uflowmeter::upload_lib::UploadKind::block_len(&kind)
            );
        }
        ShellAction::ApplyUpload(kind, data) => {
            use uflowmeter::upload_lib::{apply_calibration_block, apply_tdc_block, UploadKind};
            match kind {
                UploadKind::Calibration => {
                    apply_calibration_block(options, &data);
                    defmt::info!("shell: calibration table updated");
                }
                UploadKind::TdcRegs => {
                    let mut block = [0u8; uflowmeter::upload_lib::TDC_BLOCK];
                    block.copy_from_slice(&data[..uflowmeter::upload_lib::TDC_BLOCK]);
                    apply_tdc_block(options, &block);
                    defmt::info!("shell: TDC register block updated");
                }
            }
            persist_options(options, eeprom, buf);
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

/// How long to wait for the TDC7200 INT edge before giving up on a
/// cycle. Matches the legacy budget — at 1480 m/s and 100 mm
/// transducer spacing a real reading is < 100 µs, so anything over
/// 50 ms is a stuck transducer.
///
/// Load-bearing: `Config::min_stop_pause` must stay strictly greater
/// than this (see the note in `main`), otherwise the executor is free
/// to enter STOP while we're parked on this timeout.
const TDC_INT_TIMEOUT: embassy_time::Duration = embassy_time::Duration::from_millis(50);

// Default TDC register blocks, taken verbatim from the C++ firmware
// that runs this board: `UFlowMeter_c++/UFlowMeter/hardware/umeter.cpp`
// declares one 20-byte `def_regs` array — 10 bytes of TDC1000 config
// (0x00..0x09) followed by 10 bytes of TDC7200 config. That split is
// exactly the `Options::tdc1000_regs` / `tdc7200_regs` layout, so the
// EEPROM blob design was right all along; this board's EEPROM just
// never had valid values written to it.
const TDC1000_DEFAULT_REGS: [u8; drivers::tdc1000::CONFIG_REG_COUNT] =
    [0x48, 0x45, 0x01, 0x01, 0x07, 0xA0, 0x1E, 0x00, 0x6A, 0x03];
const TDC7200_DEFAULT_REGS: [u8; drivers::tdc7200::CONFIG_REG_COUNT] =
    [0x02, 0x44, 0x06, 0x07, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];

/// Redraw interval while a field is being edited, so the blink
/// animation in `ui.rs` advances. Its period is six frames, so this
/// gives a ~600 ms cycle.
const BLINK_FRAME: embassy_time::Duration = embassy_time::Duration::from_millis(100);

/// Redraw interval for a visible screen that is not being edited. Keeps
/// the DateTime seconds and the live flow readings moving; 500 ms is
/// below the one-second resolution of everything on screen, so nothing
/// visibly lags.
const LIVE_REFRESH: embassy_time::Duration = embassy_time::Duration::from_millis(500);

/// M-Bus broadcast period, counted in 60 s history ticks. The C++
/// reschedules its communication process with
/// `Parameters::MBUS_PROCESSING_TIMEOUT = 5*60` seconds after every
/// datagram, so 5 ticks matches it exactly.
const MBUS_PERIOD_TICKS: u8 = 5;

/// How often to repeat an unchanged measurement failure. At the 5 s
/// cycle this is once a minute.
const QUIET_CYCLES: u32 = 12;

/// Transducer pairs on this meter. The C++ calls this
/// `Parameters::SENSOR_COUNT` and keeps one calibration table per pair.
const SENSOR_COUNT: usize = 2;

/// IWDG period. The C++ configures prescaler 128 with reload 4095,
/// which on the L1's ~37 kHz LSI is about 14 s; the measurement loop
/// pets every 5 s, leaving nearly 3x margin.
const WATCHDOG_TIMEOUT_US: u32 = 14_000_000;

/// Settle time between selecting a channel / clearing flags and
/// triggering the measurement. The C++ waits 2 ms here (`umeter.cpp:55`,
/// with a comment that the value is approximate).
const TDC_SETTLE: embassy_time::Duration = embassy_time::Duration::from_millis(2);

/// Decide whether an EEPROM-sourced register block is usable. All-zero
/// means "never programmed"; the ASCII run `0x31..0x39,0x30` is the
/// "1234567890" stub the legacy RTIC firmware used to write into
/// `tdc7200_regs` on every boot. Both are junk — fall back to the C++
/// defaults rather than pushing them into the chip.
fn regs_or_default<const N: usize>(from_eeprom: &[u8], default: &[u8; N]) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(&from_eeprom[..N]);
    let all_zero = out.iter().all(|b| *b == 0);
    let ascii_stub = out.iter().all(|b| b.is_ascii_digit());
    if all_zero || ascii_stub {
        *default
    } else {
        out
    }
}

/// Select a TDC1000 channel, clear both chips' latched flags, settle,
/// trigger one measurement, then decode the result block into a
/// calibrated time of flight averaged over the configured stops.
/// Mirrors the per-channel body of the C++ `UMeter::Impl::measure()`
/// followed by its `get_tof()`.
///
/// The INT wait is level-triggered, not edge-triggered: the C++ polls
/// `while (INT::IsSet())`, so a line that is already low when we get
/// here counts as done. `wait_for_falling_edge()` would instead sit
/// there until the timeout expired and report a spurious failure.
async fn measure_channel(
    tdc1000: &mut Tdc1000Dev,
    tdc7200: &mut Tdc7200Dev,
    tdc_int: &mut ExtiInput<'static, embassy_stm32::mode::Async>,
    ch2: bool,
    verbose: bool,
) -> Option<i32> {
    if tdc1000.set_channel(ch2).is_err() {
        defmt::error!("tdc1000: set_channel failed");
        return None;
    }
    let _ = tdc1000.clear_error_flags();
    let _ = tdc7200.clear_int_flags();
    embassy_time::Timer::after(TDC_SETTLE).await;

    if tdc7200.start_measurement().is_err() {
        defmt::error!("tdc7200 start failed");
        return None;
    }
    if embassy_time::with_timeout(TDC_INT_TIMEOUT, tdc_int.wait_for_low())
        .await
        .is_err()
    {
        return None;
    }

    let block = tdc7200.read_results().ok()?;
    let n_stops = tdc7200.stop_numbers().min(MAX_STOPS);
    let tof = decode_tof(&block, n_stops).or_else(|| {
        // decode_tof only refuses on a bad stop count or an
        // uncalibrated block (CALIBRATION1 == CALIBRATION2).
        // Dump the fields the decode depends on. All-zero CALIBRATION
        // means the chip never completed a calibration cycle (expected
        // when no echo comes back); an all-zero *block* instead points
        // at the auto-increment bulk read itself.
        if !verbose {
            return None;
        }
        defmt::warn!(
            "tdc7200: result block failed to decode (n_stops={=usize}) time1={=[u8]:#x} clk1={=[u8]:#x} cal1={=[u8]:#x} cal2={=[u8]:#x}",
            n_stops,
            &block[0..3],
            &block[3..6],
            &block[33..36],
            &block[36..39]
        );
        None
    })?;
    average_stops(&tof, n_stops)
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
    mut mux: SensorMux<'static>,
    mut wdg: IndependentWatchdog<'static, peripherals::IWDG>,
) {
    // Pull the initial snapshot (the main loop publishes before
    // spawning us, so this should always be Some).
    let mut options = opt_rx.try_get().unwrap_or_default();
    // One table per sensor pair, as in the C++ `calibration_table_[]`.
    let mut tables: [CalibTable; SENSOR_COUNT] = [
        options_to_calib_table(&options, 0),
        options_to_calib_table(&options, 1),
    ];
    let mut calc = Calculator::new(options_to_meter_config(&options));

    // Window mean of recent cycles — what the meter reports as the
    // immediate consumption, mirroring the C++ `average_m3ph_`.
    let mut average = AverageBuffer::new();
    // Consecutive cycles with no usable reading, used to throttle the
    // repeated failure logging.
    let mut failed_streak: u32 = 0;

    // Start the watchdog only once we are about to enter the periodic
    // loop, so a hang in the bring-up above still reaches the debugger
    // instead of rebooting under it.
    wdg.unleash();

    // Temporary bring-up diagnostic: for the first few cycles, read
    // back the config registers we just wrote and log written-vs-read.
    // Reads returning a constant 0x00/0xFF on both chips means the bus
    // (power, CS, EN/RES, MCO reference) is the problem; a readback
    // that matches what we wrote means both encodings are correct and
    // the fault is downstream (transducers / INT wiring). Drop this
    // block once ToF numbers come out.
    let mut diag_cycles: u8 = 3;

    // Bring-up order copied from the C++ `TDC1000::init()`: pulse
    // RESET first, then raise EN. EN then stays high — that firmware
    // only drops it on a full meter shutdown, never between
    // measurements. Config is reloaded per cycle anyway, so adding a
    // power-down path later stays cheap.
    tdc1000.reset();
    tdc1000.power_on();
    tdc7200.power_on();
    embassy_time::Timer::after_millis(2).await;

    loop {
        embassy_time::Timer::after_secs(5).await;

        // Petting here rather than from a timer of its own: this task
        // waking on schedule and getting through a full SPI cycle is
        // the liveness signal worth guarding. A dedicated pet task
        // would keep the dog quiet even with everything else wedged.
        // The C++ pets from its measure process for the same reason
        // (`Src/measure.cpp:78`).
        wdg.pet();

        // Pick up any live calibration changes before kicking off the
        // next measurement pair.
        if let Some(new_opts) = opt_rx.try_changed() {
            options = new_opts;
            tables = [
                options_to_calib_table(&options, 0),
                options_to_calib_table(&options, 1),
            ];
            calc = Calculator::new(options_to_meter_config(&options));
            defmt::info!("measurement: calibration refreshed from OPTIONS_WATCH");
        }

        // Push the whole config block into both chips, preferring the
        // EEPROM copy and falling back to the C++ defaults when it is
        // blank or holds the legacy ASCII stub.
        let tdc1000_regs =
            regs_or_default(&options.tdc1000_regs().to_le_bytes(), &TDC1000_DEFAULT_REGS);
        if tdc1000.load_config(&tdc1000_regs).is_err() {
            defmt::warn!("tdc1000: load_config failed");
        }
        let tdc7200_regs =
            regs_or_default(&options.tdc7200_regs().to_le_bytes(), &TDC7200_DEFAULT_REGS);
        if tdc7200.load_config(&tdc7200_regs).is_err() {
            defmt::warn!("tdc7200: load_config failed");
        }

        if diag_cycles > 0 {
            diag_cycles -= 1;
            for a in 0..4u8 {
                match tdc1000.read_register(a) {
                    Ok(v) => defmt::info!("diag tdc1000 reg={=u8:#x} read={=u8:#x}", a, v),
                    Err(_) => defmt::warn!("diag tdc1000 reg={=u8:#x}: SPI error", a),
                }
            }
            for a in 0..4u8 {
                match tdc7200.read_register(a) {
                    Ok(v) => defmt::info!("diag tdc7200 reg={=u8:#x} read={=u8:#x}", a, v),
                    Err(_) => defmt::warn!("diag tdc7200 reg={=u8:#x}: SPI error", a),
                }
            }
            defmt::info!(
                "diag expect: tdc1000 reg0={=u8:#x} reg1={=u8:#x}, tdc7200 reg0={=u8:#x} reg1={=u8:#x}",
                tdc1000_regs[0],
                tdc1000_regs[1],
                tdc7200_regs[0],
                tdc7200_regs[1]
            );
        }

        // Two transducer pairs, each measured up- and downstream and
        // scored against its own calibration table — the C++ does the
        // same in `MeasureProcess::process()` and averages whichever
        // pairs reported a usable signal.
        let mut sum = 0.0f32;
        let mut good = 0u8;

        // With no transducers wired the frontend fails identically on
        // every cycle — eight warnings per 5 s, enough to bury anything
        // real. Report the first failure in full, then once per
        // QUIET_CYCLES, with a count so the gap is not mistaken for
        // recovery.
        let verbose = failed_streak == 0 || failed_streak.is_multiple_of(QUIET_CYCLES);

        for (sensor, table) in tables.iter().enumerate() {
            mux.set_channel(mux::Channel::for_sensor(sensor));
            // Let the analog path settle after switching pairs before
            // the frontend fires.
            embassy_time::Timer::after(TDC_SETTLE).await;

            let tof_down =
                measure_channel(&mut tdc1000, &mut tdc7200, &mut tdc_int, false, verbose).await;
            let tof_up =
                measure_channel(&mut tdc1000, &mut tdc7200, &mut tdc_int, true, verbose).await;

            if let Ok(flags) = tdc1000.error_flags() {
                if flags != 0 && verbose {
                    defmt::warn!(
                        "tdc1000: sensor {=usize} error flags {=u8:#x} (bad signal)",
                        sensor,
                        flags
                    );
                }
            }

            match (tof_down, tof_up) {
                (Some(d), Some(u)) => {
                    // decode_tof yields picoseconds; the Calculator is
                    // fed nanoseconds — the C++ divides by 1000.0f at
                    // exactly this boundary (`Src/measure.cpp:188`).
                    let volume = calc.get_volume(table, u as f32 / 1000.0, d as f32 / 1000.0);
                    defmt::info!(
                        "sensor {=usize}: tof down={=i32} up={=i32} ps, flow={=f32} m³/h",
                        sensor,
                        d,
                        u,
                        volume
                    );
                    if volume.is_finite() {
                        sum += volume;
                        good += 1;
                    }
                }
                _ if verbose => defmt::warn!(
                    "sensor {=usize}: tof down={} up={}",
                    sensor,
                    tof_down.is_some(),
                    tof_up.is_some()
                ),
                _ => {}
            }
        }

        // Park the mux between cycles so neither pair stays driven.
        mux.set_channel(mux::Channel::Off);

        if good == 0 {
            if verbose && failed_streak > 0 {
                defmt::warn!(
                    "measurement: no usable signal for {=u32} cycles",
                    failed_streak
                );
            }
            failed_streak = failed_streak.saturating_add(1);
        } else if failed_streak > 0 {
            defmt::info!(
                "measurement: signal recovered after {=u32} cycles",
                failed_streak
            );
            failed_streak = 0;
        }

        if good > 0 {
            // Both pairs collapse into one sample before entering the
            // window, so a cycle where only one pair reported does not
            // carry less weight than a cycle where both did.
            average.push(sum / good as f32);
            if let Some(mean) = average.average() {
                let mean = calc.near_zero_filter(mean);
                // 4-20 mA loop: the meter does not drive the loop
                // itself, it ships a DAC count to an external module
                // over the same serial line. Frame is AA 55 followed
                // by the little-endian count (C++ `Analog::process`,
                // comm/analog.cpp:12), emitted once per measurement
                // cycle — the C++ reschedules on RTC_WAKEUP_PERIOD,
                // which is the same 5 s.
                if CommType::from_u8(options.comm_type()) == CommType::Analog {
                    send_analog_frame(&calc, mean);
                }
                defmt::info!(
                    "flow: {=f32} m³/h (window of {=usize})",
                    mean,
                    average.len()
                );
                FLOW_RESULT.signal(mean);
            }
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

/// Pull a sensor's calibration table out of Options. Float fields are
/// stored as raw u32 bits via the bitfield's B32 slots —
/// `f32::from_bits` recovers them. Matches the C++ layout, which keeps
/// one table per sensor pair (`calibration_table_[0]`, `[1]`).
fn options_to_calib_table(options: &Options, sensor: usize) -> CalibTable {
    if sensor == 0 {
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
    } else {
        CalibTable {
            dtof0: f32::from_bits(options.zero2()),
            data: [
                CalibData {
                    v: f32::from_bits(options.v21()),
                    k: f32::from_bits(options.k21()),
                },
                CalibData {
                    v: f32::from_bits(options.v22()),
                    k: f32::from_bits(options.k22()),
                },
                CalibData {
                    v: f32::from_bits(options.v23()),
                    k: f32::from_bits(options.k23()),
                },
            ],
        }
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
            time::Date::from_calendar_date(
                dt.year() as i32,
                time::Month::try_from(dt.month()).unwrap_or(time::Month::January),
                dt.day(),
            ),
            time::Time::from_hms(dt.hour(), dt.minute(), dt.second()),
        ) {
            let _ = month;
            app.datetime = time::PrimitiveDateTime::new(date, time);
        }
    }
}
