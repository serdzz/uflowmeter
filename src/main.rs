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
use embassy_stm32::rtc::{DateTime as RtcDateTime, DayOfWeek, Rtc, RtcContainer, RtcTimeProvider};
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
use drivers::uart::{MODBUS_FRAMES, UART_TX};
use embassy_futures::select::{Either, select};
use uflowmeter::modbus_handler::{HistoryAccess, ModbusHandler};
use uflowmeter::ui::MenuController;
use uflowmeter::{App, AppRequest, Options, UiEvent};

bind_interrupts!(
    pub struct Irqs {
        EXTI0 => exti::InterruptHandler<interrupt::typelevel::EXTI0>;
        EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
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
    let p = embassy_stm32::init(config);
    info!("uflowmeter (embassy): boot");

    // RTC: low-power Rtc::new takes only the peripheral and returns
    // (container, time_provider). The container holds the Rtc behind
    // a CriticalSection Mutex so other tasks (like embassy's low-power
    // executor) can also reach in. `rtc_now` lets us read DateTime
    // without holding the lock.
    let (rtc_container, rtc_now) = Rtc::new(p.RTC);

    let _lcd_on = Output::new(p.PC0, Level::Low, Speed::Low);
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

    // TDC1000 — analog frontend, CS=PB11, EN=PB10, RES=PC6.
    let tdc1000_en = Output::new(p.PB10, Level::High, Speed::Low);
    let tdc1000_res = Output::new(p.PC6, Level::High, Speed::Low);
    let mut tdc1000 = Tdc1000::new(
        SpiDevice::new(spi2_bus, Output::new(p.PB11, Level::High, Speed::VeryHigh)),
        tdc1000_en,
        tdc1000_res,
    );
    // Load TDC1000 config from options (legacy stored a 10-byte
    // register dump in options.tdc1000_regs).
    let tdc1000_regs = options.tdc1000_regs().to_le_bytes();
    if tdc1000.load_config(&tdc1000_regs[..10]).is_err() {
        warn!("tdc1000: load_config failed");
    }
    let _ = tdc1000.clear_error_flags();

    // TDC7200 — time-to-digital, CS=PB12, EN=PB1.
    let tdc7200_en = Output::new(p.PB1, Level::High, Speed::Low);
    let mut tdc7200 = Tdc7200::new(
        SpiDevice::new(spi2_bus, Output::new(p.PB12, Level::High, Speed::VeryHigh)),
        tdc7200_en,
    );
    let tdc7200_regs = options.tdc7200_regs().to_le_bytes();
    if tdc7200.load_config(&tdc7200_regs[..10]).is_err() {
        warn!("tdc7200: load_config failed");
    }

    // TDC7200 INT line (active-LOW) on PB0 → wakes us at end of
    // measurement via EXTI0.
    let tdc_int = ExtiInput::new(p.PB0, p.EXTI0, Pull::Up, Irqs);

    // Spawn the measurement loop — runs forever, triggers a TDC1000
    // TX pulse + TDC7200 measurement once per second, logs the raw
    // 24-bit ToF.
    spawner.spawn(unwrap!(measurement_task(tdc1000, tdc7200, tdc_int)));

    // USART1: TX=PA9, RX=PA10. 115200 baud (legacy default).
    // PC9 (RsPowerEn, active-LOW) powers the RS485 transceiver.
    let _rs_power = Output::new(p.PC9, Level::Low, Speed::Low);
    let mut uart_cfg = UartConfig::default();
    uart_cfg.baudrate = 115200;
    // Argument order: (peri, rx, tx, tx_dma, rx_dma, irq, config).
    let uart = unwrap!(Uart::new(
        p.USART1,
        p.PA10,
        p.PA9,
        p.DMA1_CH4,
        p.DMA1_CH5,
        Irqs,
        uart_cfg,
    ));
    spawner.spawn(unwrap!(drivers::uart::uart_task(uart)));
    spawner.spawn(unwrap!(drivers::uart::shell_task()));

    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);
    spawner.spawn(unwrap!(keypad_task(btn_config, btn_enter, btn_down, btn_up)));

    let mut app = App::new();
    sync_app_datetime(&mut app, &rtc_now);
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

    // Initial render so the user sees something even before the first
    // key press.
    ui.update(&app);
    ui.render(&app, &mut frame);
    frame.flush(&mut lcd).await;

    loop {
        match select(KEYS.receive(), MODBUS_FRAMES.receive()).await {
            Either::First(KeyEvent::Pressed(flag)) => {
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
                    }
                }
            }
            Either::Second(frame_bytes) => {
                handle_modbus_frame(&modbus, &frame_bytes, &mut options, &mut eeprom, &app);
            }
        }
        sync_app_datetime(&mut app, &rtc_now);
        ui.update(&app);
        ui.render(&app, &mut frame);
        frame.flush(&mut lcd).await;
    }
}

/// Stub history binding for the Modbus handler. The handler still
/// declares hour/day/month parameters from the legacy register map,
/// but doesn't actually read them yet — the per-register dispatch in
/// modbus_handler.rs only knows Options + flow data. When history
/// rings get re-enabled this can be swapped for real `RingStorage`
/// references.
struct NoHistory;
impl<S, E> HistoryAccess<S, E> for NoHistory {
    fn find(
        &mut self,
        _storage: &mut S,
        _time: u32,
    ) -> Result<Option<i32>, uflowmeter::history::Error> {
        Ok(None)
    }
    fn first_timestamp(&mut self) -> u32 {
        0
    }
    fn last_timestamp(&mut self) -> u32 {
        0
    }
}

/// Dispatch a freshly-framed Modbus RTU request: parse, build reply
/// (or exception), push the reply onto UART_TX so uart_task writes it
/// out on its next iteration. Silently drops requests not addressed
/// to us — that's the InvalidSlaveAddress path.
fn handle_modbus_frame(
    modbus: &ModbusHandler,
    frame: &[u8],
    options: &mut Options,
    eeprom: &mut Eeprom,
    app: &App,
) {
    let mut hour_h = NoHistory;
    let mut day_h = NoHistory;
    let mut month_h = NoHistory;
    let result = modbus.handle_request(
        frame,
        options,
        eeprom,
        app.flow,
        app.hour_flow,
        app.day_flow,
        app.month_flow,
        &mut hour_h,
        &mut day_h,
        &mut month_h,
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
            if on {
                backlight.set_high();
            } else {
                backlight.set_low();
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

#[embassy_executor::task]
async fn measurement_task(
    mut tdc1000: Tdc1000Dev,
    mut tdc7200: Tdc7200Dev,
    mut tdc_int: ExtiInput<'static, embassy_stm32::mode::Async>,
) {
    let mut downstream = true;
    loop {
        embassy_time::Timer::after_secs(1).await;
        let _ = tdc1000.set_channel(!downstream);
        let _ = tdc1000.clear_error_flags();
        if tdc7200.start_measurement().is_err() {
            defmt::error!("tdc7200 start failed");
            continue;
        }
        // INT goes LOW on done. Wait at most 50 ms.
        match embassy_time::with_timeout(
            embassy_time::Duration::from_millis(50),
            tdc_int.wait_for_falling_edge(),
        )
        .await
        {
            Ok(()) => match tdc7200.read_time1() {
                Ok(t) => defmt::info!("tof ch={=u8} time1={=u32}", downstream as u8, t),
                Err(_) => defmt::error!("tdc7200 read_time1 failed"),
            },
            Err(_) => defmt::warn!("tdc7200 INT timeout (ch={=u8})", downstream as u8),
        }
        downstream = !downstream;
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
