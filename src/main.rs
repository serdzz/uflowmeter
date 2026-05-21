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
use embassy_stm32::{Config, bind_interrupts, interrupt};
use embassy_time::Duration;
use {defmt_rtt as _, panic_probe as _};

use drivers::hd44780::Hd44780;
use drivers::keypad::{keypad_task, ButtonFlags, KeyEvent, KEYS};

bind_interrupts!(
    pub struct Irqs {
        EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
    }
);

#[embassy_executor::main(executor = "embassy_stm32::executor::Executor", entry = "cortex_m_rt::entry")]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    // Keep the debugger alive across STOP so probe-rs RTT stays
    // connected during bring-up. Costs power — flip off for shipped
    // builds.
    config.enable_debug_during_sleep = true;
    // Minimum idle window that justifies STOP entry. Has to be SMALLER
    // than the keypad polling interval (50 ms) or every poll-period
    // gap stays "too short" and we never enter STOP.
    config.min_stop_pause = Duration::from_millis(10);
    let p = embassy_stm32::init(config);
    info!("uflowmeter (embassy): boot");

    // LCD power + backlight. PC0 (LcdOn) is active-LOW.
    let _lcd_on = Output::new(p.PC0, Level::Low, Speed::Low);
    let _backlight = Output::new(p.PC5, Level::High, Speed::Low);

    // HD44780 4-bit parallel: RS=PC1, RW=PC2, E=PC3, D4..D7 = PA4..PA7.
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

    // Buttons: PB6=Config, PB7=Enter, PB8=Down, PB9=Up (pull-up).
    // ExtiInput so the EXTI lines stay armed — a falling edge wakes the
    // MCU from STOP mode (embassy's executor handles entry/exit).
    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);
    spawner.spawn(unwrap!(keypad_task(btn_config, btn_enter, btn_down, btn_up)));

    lcd.set_position(0, 0).await;
    lcd.write_str("press a key").await;
    lcd.set_position(0, 1).await;
    lcd.write_str("                ").await;

    let mut last_count: u32 = 0;
    loop {
        let event = KEYS.receive().await;
        let KeyEvent::Pressed(flag) = event;
        last_count = last_count.wrapping_add(1);
        info!("key: {} (#{})", defmt::Debug2Format(&flag), last_count);
        lcd.set_position(0, 1).await;
        let label = if flag.contains(ButtonFlags::CONFIG) {
            "Config "
        } else if flag.contains(ButtonFlags::ENTER) {
            "Enter  "
        } else if flag.contains(ButtonFlags::DOWN) {
            "Down   "
        } else if flag.contains(ButtonFlags::UP) {
            "Up     "
        } else {
            "?      "
        };
        lcd.write_str(label).await;
    }
}
