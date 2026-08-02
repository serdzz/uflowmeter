//! Minimal keypad + LCD bring-up example.
//!
//! Exists to cut the UI problem in half. The full firmware couples the
//! display to a lot of machinery — the low-power executor entering
//! STOP, the 15 s idle timeout cutting panel power, the measurement
//! task holding SPI, `DeferredDisplay` batching ops, the `MenuController`
//! deciding what to draw. This example has none of it: press a button,
//! see its name on the LCD.
//!
//! So the outcome is unambiguous:
//!
//!   * text updates on every press → the driver and the wiring are
//!     fine, and the fault is in how the firmware drives them;
//!   * text never updates → the fault is in `Hd44780` itself, and the
//!     rest of the stack is off the hook.
//!
//! It pulls in the real driver by path rather than a copy, so what runs
//! here is exactly what the firmware runs.
//!
//! Flash with (the 500 kHz speed matters — the default rate fails to
//! connect on this board):
//!
//! ```bash
//! cargo build --release --example buttons
//! probe-rs run --chip STM32L151RC --speed 500 \
//!     target/thumbv7m-none-eabi/release/examples/buttons
//! ```

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::exti::{self, ExtiInput};
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::{bind_interrupts, interrupt};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

// The example exercises a subset of the driver; the rest is still
// compiled so this stays a genuine test of the real file.
#[allow(dead_code)]
#[path = "../src/drivers/hd44780.rs"]
mod hd44780;
use hd44780::Hd44780;

bind_interrupts!(
    struct Irqs {
        EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
    }
);

#[embassy_executor::main(
    executor = "embassy_stm32::executor::Executor",
    entry = "cortex_m_rt::entry"
)]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("buttons example: boot");

    // LCD supply (PC0) and backlight (PC5) are both active-LOW. Unlike
    // the firmware, neither is ever switched off here — the panel stays
    // powered for the whole run, which removes the power-cycle from the
    // set of things that could be wrong.
    let _lcd_power = Output::new(p.PC0, Level::Low, Speed::Low);
    let _backlight = Output::new(p.PC5, Level::Low, Speed::Low);

    let mut lcd = Hd44780::new(
        Output::new(p.PC1, Level::Low, Speed::Low), // RS
        Output::new(p.PC2, Level::Low, Speed::Low), // RW
        Output::new(p.PC3, Level::Low, Speed::Low), // E
        Output::new(p.PA4, Level::Low, Speed::Low), // D4
        Output::new(p.PA5, Level::Low, Speed::Low), // D5
        Output::new(p.PA6, Level::Low, Speed::Low), // D6
        Output::new(p.PA7, Level::Low, Speed::Low), // D7
    );
    lcd.init().await;
    info!("lcd init done");

    lcd.set_position(0, 0).await;
    lcd.write_str("BUTTON TEST").await;
    lcd.set_position(0, 1).await;
    lcd.write_str("press any key   ").await;

    let btn_config = ExtiInput::new(p.PB6, p.EXTI6, Pull::Up, Irqs);
    let btn_enter = ExtiInput::new(p.PB7, p.EXTI7, Pull::Up, Irqs);
    let btn_down = ExtiInput::new(p.PB8, p.EXTI8, Pull::Up, Irqs);
    let btn_up = ExtiInput::new(p.PB9, p.EXTI9, Pull::Up, Irqs);

    // Deliberately NOT awaiting EXTI edges. On hardware the edge-driven
    // version caught roughly one press in twenty, so this samples the
    // four lines directly and reports every transition it sees. The two
    // outcomes separate cleanly:
    //
    //   * every press shows up here → the lines are fine and EXTI
    //     arming is what drops them;
    //   * presses are missing here too → the line never returns high
    //     between presses, i.e. an electrical problem, not firmware.
    let mut prev = [false; 4];
    let mut count: u32 = 0;

    loop {
        Timer::after_millis(20).await;

        let now = [
            btn_config.is_low(),
            btn_enter.is_low(),
            btn_down.is_low(),
            btn_up.is_low(),
        ];
        if now == prev {
            continue;
        }

        // Log the whole vector on any change, so a line that stays stuck
        // low is as visible as one that toggles.
        info!(
            "levels cfg={=bool} ent={=bool} dn={=bool} up={=bool}",
            now[0], now[1], now[2], now[3]
        );

        let names = ["CONFIG", "ENTER ", "DOWN  ", "UP    "];
        for i in 0..4 {
            if now[i] && !prev[i] {
                count = count.wrapping_add(1);
                info!("press #{=u32}: {=str}", count, names[i]);

                let mut digits = [b' '; 5];
                let mut n = count;
                for slot in digits.iter_mut().rev() {
                    *slot = b'0' + (n % 10) as u8;
                    n /= 10;
                    if n == 0 {
                        break;
                    }
                }

                lcd.set_position(0, 1).await;
                lcd.write_str(names[i]).await;
                lcd.write_str(" #").await;
                for d in digits {
                    let byte = [d];
                    if let Ok(text) = core::str::from_utf8(&byte) {
                        lcd.write_str(text).await;
                    }
                }
            }
        }
        prev = now;
    }
}
