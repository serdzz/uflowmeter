#![allow(dead_code)]
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;

pub mod history_lib;

#[cfg(test)]
mod history_lib_tests;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
