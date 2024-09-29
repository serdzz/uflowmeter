use core::str::FromStr;

use crate::ui::ViewportNode;

use heapless::String;
use time::{
    macros::{date, time},
    PrimitiveDateTime,
};

#[derive(Debug, Copy, Clone)]
pub enum Actions {
    Label,
    Label1,
    DateTime,
    SetDateTime(PrimitiveDateTime),
    ActionA,
    ActionB,
}

#[derive(Debug)]
pub enum AppRequest {
    Process,
    LcdLed(bool),
    SetDateTime(PrimitiveDateTime),
    DeepSleep,
}

pub struct App {
    pub text: &'static str,
    pub num: u8,
    pub label_title: &'static str,
    pub label_value: String<16>,
    pub datetime: PrimitiveDateTime,
    pub active_widget: ViewportNode,
}

impl App {
    pub fn new() -> Self {
        Self {
            text: "Hello world!!",
            num: 34,
            label_title: "ASASSAS",
            label_value: String::from_str("123213").expect("RESON"),
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            active_widget: ViewportNode::Label,
        }
    }

    pub fn handle_event(&mut self, action: Option<Actions>) -> Option<AppRequest> {
        if let Some(action) = action {
            match action {
                Actions::ActionA => {
                    self.num = self.num.wrapping_sub(1);
                    return None;
                }
                Actions::ActionB => {
                    self.num = self.num.wrapping_add(1);
                    return None;
                }
                Actions::SetDateTime(dt) => return Some(AppRequest::SetDateTime(dt)),
                _ => return None,
            }
        }
        None
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
