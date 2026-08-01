//! Embassy-based hardware drivers (post-RTIC migration).
//!
//! These coexist with the legacy `src/hardware/` directory during the
//! port — the old modules use stm32l1xx-hal types and the embedded-hal
//! 0.2 traits; new code lives here against embassy-stm32 + embedded-hal
//! 1.0 + async APIs.

pub mod cyrillic;
pub mod deferred_display;
pub mod eeprom;
pub mod hd44780;
pub mod hd44780_blocking;
pub mod keypad;
pub mod sensor_mux;
pub mod tdc1000;
pub mod tdc7200;
pub mod uart;
