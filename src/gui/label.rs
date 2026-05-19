use crate::gui::{push_truncating, CharacterDisplay, Widget};
use core::marker::PhantomData;
use heapless::String;

#[derive(Debug, Clone)]
pub struct Label<A, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String<LEN>,
    pub invalidate: bool,
    phantom: PhantomData<A>,
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Label<A, LEN, X, Y> {
    pub fn new(val: &str) -> Self {
        let mut state: String<LEN> = String::new();
        push_truncating(&mut state, val);
        Self {
            state,
            invalidate: true,
            phantom: PhantomData,
        }
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> core::fmt::Write for Label<A, LEN, X, Y> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if !s.is_empty() {
            push_truncating(&mut self.state, s);
            self.invalidate = true;
        }
        Ok(())
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Widget<&str, A> for Label<A, LEN, X, Y> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, state: &str) {
        if self.state.as_str() != state {
            self.state.clear();
            push_truncating(&mut self.state, state);
            self.invalidate = true;
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.invalidate {
            display.reset_custom_chars();
            display.set_position(X, Y);
            write!(display, "{}", self.state.as_str()).unwrap();
            if self.state.len() + (X as usize) < LEN {
                display.finish_line(LEN, self.state.len() + X as usize);
            }
            self.invalidate = false;
        }
    }
}
