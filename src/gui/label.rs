use crate::*;
use alloc::string::String;
use core::marker::PhantomData;
use core::str::FromStr;
#[derive(Debug, Clone)]
pub struct Label<A, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String,
    invalidate: bool,
    phantom: PhantomData<A>,
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Label<A, LEN, X, Y> {
    pub fn new(val: &str) -> Self {
        Self {
            state: String::from(val), //String::from(val),
            invalidate: true,
            phantom: PhantomData,
        }
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> core::fmt::Write for Label<A, LEN, X, Y> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if !s.is_empty() {
            self.state.push_str(s); //String::from(s);
                                    //defmt::info!("write_str {}", self.state.as_str());

            self.invalidate = true;
        }
        Ok(())
    }
}

impl<A, const LEN: usize, const X: u8, const Y: u8> Widget<&str, A> for Label<A, LEN, X, Y> {
    fn invalidate(&mut self) {
        //defmt::info!("invalidate = true");
        self.invalidate = true;
    }

    fn update(&mut self, state: &str) {
        if self.state != state {
            self.state = String::from(state);
            //defmt::info!("update {}", self.state.as_str());
            self.invalidate = true;
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.invalidate {
            display.set_position(X, Y);
            //defmt::info!("display {}", self.state.as_str());
            write!(display, "{}", self.state).unwrap();
            if self.state.len() + (X as usize) < LEN {
                display.finish_line(LEN, self.state.len() + X as usize);
            }
            self.invalidate = false;
        }
    }
}
