#![allow(dead_code)]

use crate::gui::{UiEvent, Widget};

/// Button widget — triggers a callback on Enter.
/// Ported from C++ UI::Button.
pub struct Button<A> {
    label: &'static str,
    on_press: Option<fn() -> A>,
    invalidate: bool,
}

impl<A> Button<A> {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            on_press: None,
            invalidate: true,
        }
    }

    pub fn set_on_press(&mut self, f: fn() -> A) {
        self.on_press = Some(f);
    }
}

impl<A: Clone> Widget<A, A> for Button<A> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: A) {}

    fn render(&mut self, display: &mut impl core::fmt::Write) {
        if self.invalidate {
            write!(display, "{}", self.label).ok();
            self.invalidate = false;
        }
    }

    fn event(&mut self, e: UiEvent) -> Option<A> {
        match e {
            UiEvent::Enter => self.on_press.and_then(|f| Some(f())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press_cb() -> u8 {
        99
    }

    #[test]
    fn test_button_new() {
        let b: Button<u8> = Button::new("OK");
        assert_eq!(b.label, "OK");
    }

    #[test]
    fn test_button_press() {
        let mut b: Button<u8> = Button::new("Cal");
        b.set_on_press(press_cb);
        let result = b.event(UiEvent::Enter);
        assert_eq!(result, Some(99));
    }

    #[test]
    fn test_button_no_press() {
        let mut b: Button<u8> = Button::new("X");
        let result = b.event(UiEvent::Up);
        assert_eq!(result, None);
    }
}