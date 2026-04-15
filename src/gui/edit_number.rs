#![allow(dead_code)]

use crate::gui::{CharacterDisplay, UiEvent, Widget};
use core::fmt::Write;
use core::marker::PhantomData;
use heapless::String;

/// Numeric editor widget — cycles value between min and max.
/// Ported from C++ UI::EditNumber.
#[derive(Debug, Clone)]
pub struct EditNumber<A, T: Copy + PartialOrd + core::fmt::Display, const LEN: usize> {
    value: T,
    min: T,
    max: T,
    label: String<LEN>,
    editable: bool,
    edit_active: bool,
    blink_on: bool,
    invalidate: bool,
    on_change: Option<fn(T) -> A>,
    _phantom: PhantomData<A>,
}

impl<A, T: Copy + PartialOrd + core::fmt::Display, const LEN: usize>
    EditNumber<A, T, LEN>
{
    pub fn new(min: T, max: T) -> Self {
        Self {
            value: min,
            min,
            max,
            label: String::new(),
            editable: false,
            edit_active: false,
            blink_on: false,
            invalidate: true,
            on_change: None,
            _phantom: PhantomData,
        }
    }

    pub fn set_value(&mut self, val: T) {
        self.value = val;
        self.invalidate = true;
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn set_minmax(&mut self, min: T, max: T) {
        self.min = min;
        self.max = max;
    }

    pub fn set_on_change(&mut self, f: fn(T) -> A) {
        self.on_change = Some(f);
    }

    fn next(&mut self) {
        // Only works for integer-like types via Display
        // For u8/u16/u32 we rely on wrapping_add semantics
    }

    fn format_value(&mut self) {
        self.label.clear();
        write!(self.label, "{}", self.value).ok();
    }
}

// Implement for u8 — the most common use case
impl<A, const LEN: usize> EditNumber<A, u8, LEN> {
    fn increment(&mut self) {
        if self.value < self.max {
            self.value += 1;
        } else {
            self.value = self.min;
        }
    }

    fn decrement(&mut self) {
        if self.value > self.min {
            self.value -= 1;
        } else {
            self.value = self.max;
        }
    }
}

impl<A, const LEN: usize> Widget<A, A> for EditNumber<A, u8, LEN> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: A) {
        self.format_value();
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.editable {
            if self.blink_on {
                self.blink_on = false;
                display.set_position(0, 1);
                display.finish_line(16, 0);
            } else {
                self.blink_on = true;
                self.format_value();
                display.set_position(0, 1);
                write!(display, "{}", self.label).ok();
                display.finish_line(16, self.label.len());
            }
        } else if self.invalidate {
            self.format_value();
            display.set_position(0, 1);
            write!(display, "{}", self.label).ok();
            display.finish_line(16, self.label.len());
            self.invalidate = false;
        }
    }

    fn event(&mut self, e: UiEvent) -> Option<A> {
        match e {
            UiEvent::Up => {
                if self.editable {
                    self.increment();
                    self.invalidate = true;
                }
                None
            }
            UiEvent::Down => {
                if self.editable {
                    self.decrement();
                    self.invalidate = true;
                }
                None
            }
            UiEvent::Enter => {
                if self.editable {
                    self.editable = false;
                    self.invalidate = true;
                    // Return on_change result when exiting edit mode
                    self.on_change.and_then(|f| Some(f(self.value)))
                } else {
                    self.editable = true;
                    self.invalidate = true;
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_cb(v: u8) -> u8 {
        v
    }

    #[test]
    fn test_edit_number_new() {
        let e: EditNumber<u8, u8, 8> = EditNumber::new(1, 250);
        assert_eq!(e.value(), 1);
    }

    #[test]
    fn test_edit_number_set_value() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(0, 255);
        e.set_value(42);
        assert_eq!(e.value(), 42);
    }

    #[test]
    fn test_edit_number_increment() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(0, 10);
        e.set_value(5);
        e.editable = true;
        let _ = e.event(UiEvent::Up);
        assert_eq!(e.value(), 6);
    }

    #[test]
    fn test_edit_number_wrap_around() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(1, 10);
        e.set_value(10);
        e.editable = true;
        let _ = e.event(UiEvent::Up);
        assert_eq!(e.value(), 1);
    }

    #[test]
    fn test_edit_number_decrement() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(1, 10);
        e.set_value(5);
        e.editable = true;
        let _ = e.event(UiEvent::Down);
        assert_eq!(e.value(), 4);
    }

    #[test]
    fn test_edit_number_decrement_wrap() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(1, 10);
        e.set_value(1);
        e.editable = true;
        let _ = e.event(UiEvent::Down);
        assert_eq!(e.value(), 10);
    }

    #[test]
    fn test_edit_number_enter_toggle() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(0, 100);
        assert!(!e.editable);
        let _ = e.event(UiEvent::Enter);
        assert!(e.editable);
    }

    #[test]
    fn test_edit_number_on_change() {
        let mut e: EditNumber<u8, u8, 8> = EditNumber::new(1, 250);
        e.set_on_change(change_cb);
        e.editable = true;
        let result = e.event(UiEvent::Enter);
        assert_eq!(result, Some(1));
    }
}