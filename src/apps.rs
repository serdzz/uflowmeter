use crate::ui::{HistoryType, ViewportNode};
use alloc::string::String;
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
    SetHistory(HistoryType, u32),
    HourHistory,
    DayHistory,
    MonthHistory,
}

#[derive(Debug)]
pub enum AppRequest {
    Process,
    LcdLed(bool),
    SetDateTime(PrimitiveDateTime),
    SetHistory(HistoryType, u32),
    DeepSleep,
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub history_type: HistoryType,
    pub flow: Option<f32>,
    pub datetime: u32,
}

pub struct App {
    pub text: &'static str,
    pub num: u8,
    pub label_title: &'static str,
    pub label_value: String,
    pub datetime: PrimitiveDateTime,
    pub active_widget: ViewportNode,
    pub flow: f32,
    pub hour_flow: f32,
    pub day_flow: f32,
    pub month_flow: f32,
    pub history_state: HistoryState,
}

impl App {
    pub fn new() -> Self {
        Self {
            text: "Привет",
            num: 34,
            label_title: "Uptime",
            label_value: String::from("123456"),
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            active_widget: ViewportNode::Label,
            flow: 0.0,
            hour_flow: 0.0,
            day_flow: 0.0,
            month_flow: 0.0,
            history_state: HistoryState {
                history_type: HistoryType::Hour,
                flow: Some(0.0),
                datetime: 0,
            },
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
                Actions::SetHistory(t, datetime) => {
                    return Some(AppRequest::SetHistory(t, datetime))
                }
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
