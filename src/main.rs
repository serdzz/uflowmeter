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
use embassy_stm32::rtc::{Rtc, RtcTimeProvider};
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
    // (container, time_provider). `_rtc_container` lives until end of
    // main so the low-power executor sees the RTC; `rtc_now` lets us
    // read DateTime without holding a lock.
    let (_rtc_container, rtc_now) = Rtc::new(p.RTC);

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
    let options: Options = match Options::load_with_buf(&mut eeprom, &mut opt_buf) {
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

    // Initial render so the user sees something even before the first
    // key press.
    ui.update(&app);
    ui.render(&app, &mut frame);
    frame.flush(&mut lcd).await;

    loop {
        let event = KEYS.receive().await;
        let KeyEvent::Pressed(flag) = event;
        let ui_event = if flag.contains(ButtonFlags::ENTER) {
            Some(UiEvent::Enter)
        } else if flag.contains(ButtonFlags::CONFIG) {
            Some(UiEvent::Back)
        } else if flag.contains(ButtonFlags::DOWN) {
            // Match legacy keyboard.rs: hardware Down → UiEvent::Left,
            // Up → UiEvent::Right. (Yes the names are inverted vs.
            // visual intuition; that's the existing UI convention.)
            Some(UiEvent::Left)
        } else if flag.contains(ButtonFlags::UP) {
            Some(UiEvent::Right)
        } else {
            None
        };

        if let Some(e) = ui_event {
            if let Some(req) = ui.event(e, &app) {
                handle_app_request(req, &mut backlight);
            }
        }
        sync_app_datetime(&mut app, &rtc_now);
        ui.update(&app);
        ui.render(&app, &mut frame);
        frame.flush(&mut lcd).await;
    }
}

/// First-pass AppRequest dispatcher. Wire up handlers as the
/// dependencies (EEPROM-backed options, RTC set_datetime, TDC
/// measurement orchestration) get ported.
fn handle_app_request(req: AppRequest, backlight: &mut Output<'static>) {
    match req {
        AppRequest::SystemReset => {
            defmt::info!("AppRequest::SystemReset");
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
        AppRequest::SetDateTime(_dt) => {
            defmt::warn!("AppRequest::SetDateTime — RTC set_datetime not wired yet");
        }
        AppRequest::SetHistory(_, _) => {
            defmt::trace!("AppRequest::SetHistory (unimplemented)");
        }
        AppRequest::SetCommType(_)
        | AppRequest::SetAddress(_)
        | AppRequest::SetMuster(_)
        | AppRequest::SetNegative(_) => {
            defmt::warn!("AppRequest::Set* — options persistence not wired yet");
        }
        AppRequest::ExitShell | AppRequest::EnterCalibration => {
            defmt::trace!("AppRequest::Exit/Enter (no-op)");
        }
    }
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
