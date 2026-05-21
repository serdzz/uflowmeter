#![cfg_attr(not(test), no_std)]

// During the embassy migration most of the embedded glue (hardware/*,
// history/mbus/modbus/etc.) is gated off because it still imports the
// old stm32l1xx-hal types. Host tests keep working because they only
// reach into the pure-Rust modules below.

#[cfg(test)]
extern crate alloc;

pub mod apps;
pub mod calibration;
pub mod gui;
pub mod history_lib;
// measurement/ultrasonic_flow.rs still uses embedded-hal 0.2 API
// (blocking::spi, digital::v2). Gated off for the embassy port until
// the TDC drivers are rewritten on embedded-hal 1.0.
#[cfg(test)]
pub mod measurement;
pub mod ui;

pub use apps::{Actions, App, AppRequest};
pub use gui::{CharacterDisplay, Edit, Label, UiEvent, Widget};

#[cfg(test)]
mod history_lib_tests;

#[cfg(test)]
mod history_tests;

#[cfg(test)]
mod ui_logic_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ui_history_tests;
