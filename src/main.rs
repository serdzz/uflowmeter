#![no_std]
#![no_main]

//! Embassy-based firmware skeleton.
//!
//! Status: WIP migration from RTIC. Backup of the prior implementation
//! lives in `src/main.rs.rtic-backup`.

mod drivers;

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

use drivers::hd44780::Hd44780;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("uflowmeter (embassy): boot");

    // Power the LCD and its backlight. PC0 (LcdOn) is active-LOW —
    // legacy code drives it low to enable the LCD rail. PC5 (LcdLed)
    // is straight high = backlight on.
    let _lcd_on = Output::new(p.PC0, Level::Low, Speed::Low);
    let _backlight = Output::new(p.PC5, Level::High, Speed::Low);

    // HD44780 4-bit parallel mode. Pin map per src/hardware/pins.rs:
    //   RS=PC1, RW=PC2, E=PC3, D4=PA4, D5=PA5, D6=PA6, D7=PA7.
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

    lcd.set_position(0, 0).await;
    lcd.write_str("embassy").await;
    lcd.set_position(0, 1).await;
    lcd.write_str("alive").await;

    let mut n: u32 = 0;
    loop {
        Timer::after_millis(1000).await;
        info!("tick {}", n);
        n = n.wrapping_add(1);
    }
}
