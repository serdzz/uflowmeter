#![allow(dead_code)]
pub mod button;
pub mod clickable_label;
pub mod date_time_widget;
mod edit;
mod edit_number;
mod editbox;
mod empty;
pub mod history_widget;
pub mod label;
pub mod list;
pub mod live_meter;
mod macros;

pub use button::*;
pub use clickable_label::*;
pub use edit::*;
pub use edit_number::*;
//pub use editbox::*;
//pub use empty::*;
pub use label::*;
pub use list::*;
pub use live_meter::*;
//pub use macros::*;
pub use date_time_widget::DateTimeItems;
pub use history_widget::HistoryType;

#[allow(dead_code)]
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Copy, Clone)]
pub enum UiEvent {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
}

pub trait CharacterDisplay: core::fmt::Write {
    fn set_position(&mut self, col: u8, row: u8);
    fn clear(&mut self);
    fn reset_custom_chars(&mut self);
    fn finish_line(&mut self, width: usize, len: usize) {
        if len >= width {
            return;
        }
        let remaining = width - len;
        for _ in 0..remaining {
            self.write_str(" ").unwrap();
        }
    }
}

pub trait Widget<S, A> {
    fn invalidate(&mut self);
    fn update(&mut self, _state: S);
    fn render(&mut self, display: &mut impl CharacterDisplay);
    fn event(&mut self, _e: UiEvent) -> Option<A> {
        None
    }
}
