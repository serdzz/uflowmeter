#![allow(dead_code)]

use crate::gui::{UiEvent, Widget};
use heapless::String;

/// Clickable label — triggers callback when a key pattern is matched.
/// Simplified from C++ UI::ClickableLabel (no pattern matching — just Enter).
pub struct ClickableLabel<A, const LEN: usize> {
    text: String<LEN>,
    align_right: bool,
    on_press: Option<fn() -> A>,
    invalidate: bool,
}

impl<A, const LEN: usize> ClickableLabel<A, LEN> {
    pub fn new(text: &str, align_right: bool) -> Self {
        let mut t = String::new();
        t.push_str(text).ok();
        Self {
            text: t,
            align_right,
            on_press: None,
            invalidate: true,
        }
    }

    pub fn set_on_press(&mut self, f: fn() -> A) {
        self.on_press = Some(f);
    }

    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text).ok();
        self.invalidate = true;
    }
}

impl<A: Clone, const LEN: usize> Widget<A, A> for ClickableLabel<A, LEN> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: A) {}

    fn render(&mut self, display: &mut impl core::fmt::Write) {
        if self.invalidate {
            if self.align_right {
                let pad = 16usize.saturating_sub(self.text.len());
                for _ in 0..pad {
                    write!(display, " ").ok();
                }
            }
            write!(display, "{}", self.text).ok();
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
        7
    }

    #[test]
    fn test_clickable_label_new() {
        let l: ClickableLabel<u8, 16> = ClickableLabel::new("Menu", false);
        assert_eq!(l.text.as_str(), "Menu");
    }

    #[test]
    fn test_clickable_label_press() {
        let mut l: ClickableLabel<u8, 16> = ClickableLabel::new("Cfg", false);
        l.set_on_press(press_cb);
        let result = l.event(UiEvent::Enter);
        assert_eq!(result, Some(7));
    }

    #[test]
    fn test_clickable_label_set_text() {
        let mut l: ClickableLabel<u8, 16> = ClickableLabel::new("Old", false);
        l.set_text("New");
        assert_eq!(l.text.as_str(), "New");
    }
}