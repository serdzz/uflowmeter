#![allow(unsafe_code)]
use crate::app::*;
use crate::*;
use bit_field::*;
use core::marker::PhantomData;
use core::str::FromStr;
//use core::str::FromStr;
use heapless::String;

#[derive(Debug, Clone)]
pub struct Edit<A, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String<LEN>,
    editable: bool,
    blink_state: bool,
    invalidate: bool,
    blink: u64,
    blink_mask: u32,
    phantom: PhantomData<A>,
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Edit<A, LEN, X, Y> {
    const BLINK_TIME: u64 = 200_u64;

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
        self.blink_state = false;
    }

    pub fn blink_mask(&mut self, mask: u32) {
        self.blink_mask = mask;
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
        if self.editable && monotonics::now().ticks() > self.blink {
            self.blink = monotonics::now().ticks() + Self::BLINK_TIME;
            self.invalidate = true;
            if self.blink_state {
                self.blink_state = false;
                display.finish_line(LEN, X as usize);
            } else {
                self.blink_state = true;
                write!(display, "{}", self.state).unwrap();
                display.finish_line(LEN, self.state.len() + X as usize);
            }
        }
        if self.invalidate {
            display.reset_custom_chars();
            display.set_position(X, Y);
            self.invalidate = false;
            let mut state = self.state.clone();
            if self.blink_state {
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
