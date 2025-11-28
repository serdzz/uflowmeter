#![allow(unsafe_code)]
use crate::gui::{CharacterDisplay, UiEvent, Widget};
use bit_field::*;
#[allow(unused_imports)]
use core::fmt::Write;
use core::marker::PhantomData;
use core::str::FromStr;
use heapless::String;
#[derive(Debug, Clone)]
pub struct Edit<A, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String<LEN>,
    pub editable: bool,
    pub blink_state: bool,
    pub invalidate: bool,
    pub blink: u64,
    pub blink_mask: u32,
    phantom: PhantomData<A>,
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Edit<A, LEN, X, Y> {
    pub const BLINK_TIME: u64 = 200_u64;

    pub fn new(val: &str) -> Self {
        Self {
            state: String::from_str(val).expect("REASON"),
            editable: false,
            blink_state: false,
            invalidate: true,
            blink: 0,
            blink_mask: 0xffffffff,
            phantom: PhantomData,
        }
    }

    pub fn editable(&self) -> bool {
        self.editable
    }

    pub fn set_editable(&mut self, val: bool) {
        self.editable = val;
        self.blink_state = val; // Start with true when editable
    }

    pub fn blink_mask(&mut self, mask: u32) {
        self.blink_mask = mask;
        self.invalidate = true;
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> core::fmt::Write for Edit<A, LEN, X, Y> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.state.push_str(s).is_err();
        self.invalidate = true;
        Ok(())
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Widget<&str, A> for Edit<A, LEN, X, Y> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, state: &str) {
        if self.state != state {
            self.state = String::from_str(state).expect("RESON");
            self.invalidate = true;
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<A> {
        if let UiEvent::Enter = event {
            if self.editable {
                self.editable = false;
                self.blink_state = false;
                self.invalidate = true;
            } else {
                self.editable = true;
                self.blink_state = true;
                self.blink = 0;
            }
        }
        None
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        // If editable, toggle blink state to create blinking effect
        // Toggle every BLINK_TIME renders (~200ms at 10Hz = 2 renders)
        if self.editable {
            self.blink += 1;
            #[cfg(not(test))]
            defmt::trace!(
                "Edit::render editable - blink={}, threshold={}, blink_state={}, blink_mask={:08b}",
                self.blink,
                Self::BLINK_TIME / 100,
                self.blink_state,
                self.blink_mask
            );
            if self.blink >= Self::BLINK_TIME / 100 {
                // Assuming ~100ms per render
                self.blink = 0;
                self.blink_state = !self.blink_state;
                #[cfg(not(test))]
                defmt::info!(
                    "Edit::render TOGGLED blink_state to {}, blink_mask={:08b}",
                    self.blink_state,
                    self.blink_mask
                );
            }
            self.invalidate = true;
        }

        if self.invalidate {
            display.reset_custom_chars();
            display.set_position(X, Y);
            self.invalidate = false;
            let mut state = self.state.clone();
            if self.editable && !self.blink_state {
                unsafe {
                    let bytes = state.as_bytes_mut();
                    for (i, item) in bytes.iter_mut().enumerate().take(LEN) {
                        if self.blink_mask.get_bit(LEN - i - 1) {
                            *item = b' ';
                        }
                    }
                }
            }
            write!(display, "{}", state).unwrap();
            display.finish_line(LEN, self.state.len() + X as usize);
        }
    }
}
