#![no_std]
#![no_main]

//! Embassy-based firmware skeleton.
//!
//! Status: WIP migration from RTIC. This file is a deliberately minimal
//! "is embassy even alive on this chip?" check. Once the LED blinks and
//! defmt timestamps advance through STOP, the rest of the device — UI,
//! options, history, modbus, TDCs — gets ported on top.
//!
//! See `src/main.rs.rtic-backup` for the prior RTIC implementation.

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("uflowmeter (embassy): boot");

    // PA12 is the LCD backlight on this board (per src/hardware/pins.rs).
    // Toggling it gives a visible "I'm alive" signal without needing the
    // HD44780 driver yet.
    let mut led = Output::new(p.PA12, Level::Low, Speed::Low);

    loop {
        led.set_high();
        Timer::after_millis(500).await;
        led.set_low();
        Timer::after_millis(500).await;
        info!("tick");
    }
}
