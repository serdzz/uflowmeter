use crate::gui::HistoryType;
use time::PrimitiveDateTime;

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

#[derive(Debug, PartialEq)]
pub enum AppRequest {
    Process,
    LcdLed(bool),
    SetDateTime(PrimitiveDateTime),
    SetHistory(HistoryType, u32),
    DeepSleep,
    SetCommType(u8),
    SetAddress(u8),
    SetMuster(bool),
    SetNegative(bool),
    ExitShell,
    SystemReset,
    EnterCalibration,
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub history_type: HistoryType,
    pub flow: Option<f32>,
    pub datetime: u32,
}

pub struct App {
    pub text: &'static str,
    pub num: u64,
    pub label_title: &'static str,
    pub label_value: alloc::string::String,
    pub datetime: PrimitiveDateTime,
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
            label_value: alloc::string::String::from("123456"),
            datetime: time::macros::datetime!(2023-01-01 00:00:00),
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

    pub fn handle_event(&mut self, _action: Option<Actions>) -> Option<AppRequest> {
        None
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
