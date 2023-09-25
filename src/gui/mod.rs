mod edit;
mod editbox;
mod empty;
mod label;
mod macros;

pub use edit::*;
pub use editbox::*;
pub use empty::*;
pub use label::*;
pub use macros::*;

#[allow(dead_code)]
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
    fn finish_line(&mut self, width: usize, len: usize) {
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
