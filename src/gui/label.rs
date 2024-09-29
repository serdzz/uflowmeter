use crate::*;
use core::marker::PhantomData;
use core::str::FromStr;
use heapless::String;
#[derive(Debug, Clone)]
pub struct Label<A, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String<LEN>,
    invalidate: bool,
    phantom: PhantomData<A>,
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Label<A, LEN, X, Y> {
    pub fn new(val: &str) -> Self {
        Self {
            state: String::from_str(val).expect("REASON"), //String::from(val),
            invalidate: true,
            phantom: PhantomData,
        }
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> core::fmt::Write for Label<A, LEN, X, Y> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.state.push_str(s).is_err();
        self.invalidate = true;
        Ok(())
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Widget<&str, A> for Label<A, LEN, X, Y> {
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, state: &str) {
        if self.state != state {
            self.state = String::from_str(state).expect("REASON");
            self.invalidate = true;
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.invalidate {
            display.set_position(X, Y);
            write!(display, "{}", self.state).unwrap();
            display.finish_line(LEN, self.state.len() + X as usize);
            self.invalidate = false;
        }
    }
}
