use crate::*;
use core::fmt::Write;
use core::marker::PhantomData;
use heapless::String;

#[derive(Debug, Clone)]
pub struct EditBox<A, B: 'static, const LEN: usize, const X: u8, const Y: u8> {
    pub state: String<LEN>,
    boxs: &'static [B],
    cursor: usize,
    editable: bool,
    edit_state: bool,
    invalidate: bool,
    phantom: PhantomData<A>,
}

impl<A, B: 'static + core::fmt::Display, const LEN: usize, const X: u8, const Y: u8>
    EditBox<A, B, LEN, X, Y>
{
    pub fn new(boxs: &'static [B]) -> Self {
        Self {
            state: String::from(""),
            boxs,
            cursor: 0,
            editable: false,
            edit_state: false,
            invalidate: true,
            phantom: PhantomData,
        }
    }

    pub fn editable(&self) -> bool {
        self.editable
    }

    fn move_up(&mut self) {
        if self.editable {
            self.cursor = if self.cursor == 0 {
                self.boxs.len() - 1
            } else {
                self.cursor - 1
            }
        }
    }

    fn move_down(&mut self) {
        if self.editable {
            self.cursor = (self.cursor + 1) % self.boxs.len();
        }
    }

    fn cursor(&self) -> usize {
        self.cursor % self.boxs.len()
    }

    pub fn selected(&self) -> &B {
        let mut boxs = self.boxs.iter().cycle().skip(self.cursor());
        boxs.next().unwrap() as _
    }

    pub fn selected_str(&self) -> String<LEN> {
        let mut sel_str = String::<LEN>::from("");
        write!(sel_str, "{}", self.selected()).unwrap();
        sel_str
    }
}

impl<A, B, const LEN: usize, const X: u8, const Y: u8> core::fmt::Write
    for EditBox<A, B, LEN, X, Y>
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.state.push_str(s).is_err();
        self.invalidate = true;
        Ok(())
    }
}

impl<A, B: 'static + core::fmt::Display, const LEN: usize, const X: u8, const Y: u8> Widget<&App, A>
    for EditBox<A, B, LEN, X, Y>
{
    fn invalidate(&mut self) {
        self.invalidate = true;
    }

    fn update(&mut self, _state: &App) {
        if self.state != self.selected_str() {
            self.state = self.selected_str();
            self.invalidate = true;
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<A> {
        match event {
            UiEvent::Left => {
                self.move_up();
                None
            }
            UiEvent::Right => {
                self.move_down();
                None
            }
            UiEvent::Enter => {
                if self.editable {
                    self.editable = false;
                    self.invalidate = true;
                } else {
                    self.editable = true;
                }
                None
            }
            _ => None,
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        if self.editable {
            display.set_position(X, Y);
            if self.edit_state {
                self.edit_state = false;
                display.finish_line(LEN, X as usize);
            } else {
                self.edit_state = true;
                write!(display, "{}", self.state).unwrap();
                display.finish_line(LEN, self.state.len() + X as usize);
            }
        } else if self.invalidate {
            display.set_position(X, Y);
            write!(display, "{}", self.state).unwrap();
            display.finish_line(LEN, self.state.len() + X as usize);
            self.invalidate = false;
        }
    }
}
