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

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::{self, ExtiInput};
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, bind_interrupts, interrupt};
use embassy_time::Duration;
use {defmt_rtt as _, panic_probe as _};

use drivers::deferred_display::DeferredDisplay;
use drivers::eeprom::Eeprom25Lc1024;
use drivers::hd44780::Hd44780;
use drivers::keypad::{keypad_task, ButtonFlags, KeyEvent, KEYS};
use uflowmeter::ui::MenuController;
use uflowmeter::{App, UiEvent};

bind_interrupts!(
    pub struct Irqs {
        EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
    }
);

#[embassy_executor::main(executor = "embassy_stm32::executor::Executor", entry = "cortex_m_rt::entry")]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.enable_debug_during_sleep = true;
    config.min_stop_pause = Duration::from_millis(10);
    let p = embassy_stm32::init(config);
    info!("uflowmeter (embassy): boot");

    let _lcd_on = Output::new(p.PC0, Level::Low, Speed::Low);
    let _backlight = Output::new(p.PC5, Level::High, Speed::Low);

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

    // SPI2 + 25LC1024 EEPROM smoke test. Pin map per
    // src/hardware/pins.rs: SCK=PB13, MISO=PB14, MOSI=PB15.
    // CS=PC10 (MemoryEn, active-low), HOLD=PC11, WP=PC12.
    let mut spi2_cfg = SpiConfig::default();
    spi2_cfg.frequency = Hertz(1_000_000);
    let spi2 = Spi::new_blocking(p.SPI2, p.PB13, p.PB15, p.PB14, spi2_cfg);
    let cs_eeprom = Output::new(p.PC10, Level::High, Speed::VeryHigh);
    let hold = Output::new(p.PC11, Level::High, Speed::Low);
    let wp = Output::new(p.PC12, Level::High, Speed::Low);
    let mut eeprom = Eeprom25Lc1024::new(spi2, cs_eeprom, hold, wp);
    let mut buf = [0u8; 16];
    match eeprom.read(0, &mut buf) {
        Ok(()) => info!("eeprom@0: {=[u8]:x}", buf),
        Err(_) => error!("eeprom read failed"),
    }

    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);
    spawner.spawn(unwrap!(keypad_task(btn_config, btn_enter, btn_down, btn_up)));

    let app = App::new();
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
            // Returned AppRequest currently ignored — driver tasks
            // (Process, DeepSleep, etc.) aren't wired up yet on the
            // embassy port.
            let _ = ui.event(e, &app);
        }
        ui.update(&app);
        ui.render(&app, &mut frame);
        frame.flush(&mut lcd).await;
    }
}
