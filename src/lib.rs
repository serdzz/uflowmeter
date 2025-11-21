#![allow(dead_code)]
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: emballoc::Allocator<4096> = emballoc::Allocator::new();

#[cfg(not(test))]
extern crate stm32l1xx_hal as hal;

pub mod history_lib;

#[cfg(not(test))]
pub mod hardware {
    pub mod display;
    pub mod gpio_power;
    pub mod hd44780;
    pub mod pins;

    pub use display::*;
    pub use gpio_power::*;
    pub use hd44780::*;
    pub use pins::*;
}

#[cfg(test)]
mod history_lib_tests;

#[cfg(test)]
mod ui_logic_tests;
