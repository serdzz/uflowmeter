//! Embassy-based hardware drivers (post-RTIC migration).
//!
//! These coexist with the legacy `src/hardware/` directory during the
//! port — the old modules use stm32l1xx-hal types and the embedded-hal
//! 0.2 traits; new code lives here against embassy-stm32 + embedded-hal
//! 1.0 + async APIs.

pub mod hd44780;
pub mod keypad;
