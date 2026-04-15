#![allow(dead_code)]

use crate::gui::{CharacterDisplay, UiEvent, Widget};
use core::fmt::Write;
use heapless::String;

/// Live meter widget — shows a title on line 0 and a value on line 1.
/// Supports long-press Enter for calibration entry.
/// Ported from C++ UI::LiveMeter.
#[derive(Debug)]
pub struct LiveMeter<A, const LEN: usize> {
    title: String<LEN>,
    value: String<LEN>,
    precision: u8,
    invalidate: bool,
    on_long_press: Option<fn() -> A>,
    already_pressed: bool,
}

impl<A, const LEN: usize> LiveMeter<A, LEN> {
    pub fn new(title: &str, precision: u8) -> Self {
        let mut t = String::new();
        t.push_str(title).ok();
        Self {
            title: t,
            value: String::new(),
            precision,
            invalidate: true,
            on_long_press: None,
            already_pressed: false,
        }
    }

    pub fn set_value(&mut self, val: f32) {
        self.value.clear();
        write!(self.value, "{:.1$}", val, self.precision as usize).ok();
        self.invalidate = true;
    }

    pub fn set_text(&mut self, text: &str) {
        self.value.clear();
        self.value.push_str(text).ok();
        self.invalidate = true;
    }

    pub fn set_on_long_press(&mut self, f: fn() -> A) {
        self.on_long_press = Some(f);
    }
}

impl<A, const LEN: usize> Widget<A, A> for LiveMeter<A, LEN> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: A) {
        self.invalidate = true;
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.invalidate {
            display.set_position(0, 0);
            write!(display, "{}", self.title).ok();
            display.finish_line(16, self.title.len());

            display.set_position(0, 1);
            write!(display, "{}", self.value).ok();
            display.finish_line(16, self.value.len());
            self.invalidate = false;
        }
    }

    fn event(&mut self, e: UiEvent) -> Option<A> {
        match e {
            UiEvent::Enter => {
                if let Some(f) = self.on_long_press {
                    // Long press detection would need a timer
                    // For now, Enter triggers immediately
                    self.already_pressed = true;
                    Some(f())
                } else {
                    None
                }
            }
            _ => {
                self.already_pressed = false;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_meter_new() {
        let m: LiveMeter<(), 16> = LiveMeter::new("Flow", 2);
        assert_eq!(m.title.as_str(), "Flow");
    }

    #[test]
    fn test_live_meter_set_value() {
        let mut m: LiveMeter<(), 16> = LiveMeter::new("Flow", 2);
        m.set_value(123.456);
        assert!(m.value.contains("123.4"));
    }

    #[test]
    fn test_live_meter_set_text() {
        let mut m: LiveMeter<(), 16> = LiveMeter::new("Vol", 0);
        m.set_text("---");
        assert_eq!(m.value.as_str(), "---");
    }

    fn long_press_cb() -> u8 {
        42
    }

    #[test]
    fn test_live_meter_long_press() {
        let mut m: LiveMeter<u8, 16> = LiveMeter::new("Flow", 1);
        m.set_on_long_press(long_press_cb);
        let result = m.event(UiEvent::Enter);
        assert_eq!(result, Some(42));
    }
}