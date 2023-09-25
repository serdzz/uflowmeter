use crate::*;
use core::marker::PhantomData;

#[derive(Debug, Clone)]
pub struct Empty<A> {
    phantom: PhantomData<A>,
}

impl<A> Empty<A> {
    pub fn new() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<A> Widget<(), A> for Empty<A> {
    fn invalidate(&mut self) {}
    fn update(&mut self, _state: ()) {}
    fn render(&mut self, _display: &mut impl CharacterDisplay) {}
}
