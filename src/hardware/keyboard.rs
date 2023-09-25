use crate::app::*;
use crate::*;
use bitflags::bitflags;
use core::fmt;
use embedded_hal::digital::v2::InputPin;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct ButtonFlags: u8 {
        const None   = 0b00000000;
        const Config = 0b00000001;
        const Enter  = 0b00000010;
        const Down   = 0b00000100;
        const Up     = 0b00001000;
        const Exit = Self::Up.bits() | Self::Down.bits();
        const Manufacture = Self::Config.bits() | Self::Enter.bits() | Self::Down.bits();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ButtonEvent {
    Pressed(ButtonFlags),
    Released(ButtonFlags),
}

impl fmt::Display for ButtonEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl core::fmt::Debug for ButtonFlags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

impl core::fmt::Display for ButtonFlags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

impl core::str::FromStr for ButtonFlags {
    type Err = bitflags::parser::ParseError;

    fn from_str(flags: &str) -> Result<Self, Self::Err> {
        bitflags::parser::from_str(flags)
    }
}

impl ButtonFlags {
    pub fn to_ui_event(button: ButtonFlags) -> Option<gui::UiEvent> {
        let mut ret = None;
        if button == ButtonFlags::Enter {
            ret = Some(gui::UiEvent::Enter);
        } else if button == ButtonFlags::Up {
            ret = Some(gui::UiEvent::Right);
        } else if button == ButtonFlags::Down {
            ret = Some(gui::UiEvent::Left);
        }
        ret
    }
}

pub struct Button<IN> {
    button: IN,
    flag: ButtonFlags,
    state: bool,
    active: u64,
}

impl<IN, PinError> Button<IN>
where
    IN: InputPin<Error = PinError>,
    PinError: core::fmt::Debug,
{
    const REPEAT_DELAY: u64 = 1000_u64;
    const REPEAT_INTERVAL: u64 = 150_u64;

    pub fn new(button: IN, flag: ButtonFlags) -> Self {
        Self {
            button,
            flag,
            state: false,
            active: 0_u64,
        }
    }

    pub fn read(&mut self) -> Option<ButtonEvent> {
        if self.button.is_low().unwrap() && !self.state {
            self.state = true;
            self.active = monotonics::now().ticks() + Self::REPEAT_DELAY;
            return Some(ButtonEvent::Pressed(self.flag));
        } else if self.button.is_high().unwrap() && self.state {
            self.state = false;
            return Some(ButtonEvent::Released(self.flag));
        }
        if self.state && monotonics::now().ticks() >= self.active {
            self.active = monotonics::now().ticks() + Self::REPEAT_INTERVAL;
            return Some(ButtonEvent::Pressed(self.flag));
        }
        None
    }
}

pub struct Keyboard {
    button_set: Button<ButtonSet>,
    button_enter: Button<ButtonEnter>,
    button_down: Button<ButtonDown>,
    button_up: Button<ButtonUp>,
    pressed: Option<ButtonFlags>,
}

impl Keyboard {
    pub fn new(
        button_set: ButtonSet,
        button_enter: ButtonEnter,
        button_down: ButtonDown,
        button_up: ButtonUp,
    ) -> Self {
        Self {
            button_set: Button::new(button_set, ButtonFlags::Config),
            button_enter: Button::new(button_enter, ButtonFlags::Enter),
            button_down: Button::new(button_down, ButtonFlags::Down),
            button_up: Button::new(button_up, ButtonFlags::Up),
            pressed: None,
        }
    }

    pub fn read_ui_keys(&mut self) -> Option<UiEvent> {
        let mut event = None;
        if let Some(btn) = self.read_keys() {
            event = match btn {
                ButtonEvent::Pressed(press) => ButtonFlags::to_ui_event(press),
                ButtonEvent::Released(_) => None,
            };
        }
        event
    }

    pub fn read_keys(&mut self) -> Option<ButtonEvent> {
        let mut event = self.button_set.read();
        if let Some(btn) = self.button_enter.read() {
            event = Some(btn);
        }
        if let Some(btn) = self.button_up.read() {
            event = Some(btn);
        }
        if let Some(btn) = self.button_down.read() {
            event = Some(btn);
        }
        if let Some(btn) = event {
            let mut pressed = ButtonFlags::None;
            let mut released = ButtonFlags::None;
            match btn {
                ButtonEvent::Pressed(p) => pressed = p,
                ButtonEvent::Released(r) => released = r,
            }
            let pres = match self.pressed {
                Some(b) => (b | pressed) ^ released,
                None => pressed ^ released,
            };
            self.pressed = Some(pres);
            if ButtonFlags::None != pres {
                return Some(ButtonEvent::Pressed(pres));
            }
        }
        None
    }
}
