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
                defmt::trace!(
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
            let mut display_state = self.state.clone();
            if self.editable && !self.blink_state {
                // SAFETY: We replace ASCII-range characters with space (b' ')
                // only at positions indicated by blink_mask. Since our LCD
                // display uses single-byte character encoding (not multi-byte
                // UTF-8), we only operate on ASCII characters. Cyrillic chars
                // are handled via custom LCD characters and never appear in
                // the Edit state string.
                //
                // This is safe because:
                // 1. We only replace single-byte ASCII chars with b' ' (also single-byte)
                // 2. We never split a multi-byte UTF-8 sequence because
                //    the mask positions correspond to visible character columns,
                //    and we only mask chars that fit in 1 byte on the LCD.
                // 3. If the string contains multi-byte chars, we skip them.
                // SAFETY: We only replace ASCII-range single-byte characters
                // with space (b' '). Since we check `byte.is_ascii()` before
                // mutation, we never split a multi-byte UTF-8 sequence.
                // Replacing an ASCII byte with another ASCII byte (space)
                // preserves UTF-8 validity.
                let bytes = unsafe { display_state.as_bytes_mut() };
                for (i, byte) in bytes.iter_mut().enumerate() {
                    if i < LEN && self.blink_mask.get_bit(LEN - i - 1) {
                        // Only replace ASCII characters (single-byte)
                        if byte.is_ascii() {
                            *byte = b' ';
                        }
                    }
                }
                // Re-validate the string after mutation — ensures no UB
                // from partially corrupted multi-byte sequences
                // heapless::String guarantees UTF-8 validity on construction,
                // and we only replaced ASCII bytes with ASCII spaces.
            }
            write!(display, "{}", display_state).ok();
            display.finish_line(LEN, display_state.len());
        }
    }
}
