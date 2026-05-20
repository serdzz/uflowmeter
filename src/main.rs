#![no_std]
#![no_main]

//! Embassy-based firmware skeleton.
//!
//! Status: WIP migration from RTIC. Backup of the prior implementation
//! lives in `src/main.rs.rtic-backup`.

mod drivers;

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use {defmt_rtt as _, panic_probe as _};

use drivers::hd44780::Hd44780;
use drivers::keypad::{keypad_task, ButtonFlags, KeyEvent, KEYS};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
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

    // Buttons: PB6=Config, PB7=Enter, PB8=Down, PB9=Up (all pull-up).
    let btn_config = Input::new(p.PB6, Pull::Up);
    let btn_enter = Input::new(p.PB7, Pull::Up);
    let btn_down = Input::new(p.PB8, Pull::Up);
    let btn_up = Input::new(p.PB9, Pull::Up);
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
