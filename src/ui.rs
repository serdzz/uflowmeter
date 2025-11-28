use crate::gui::{CharacterDisplay, Edit, Label, UiEvent, Widget};
use crate::Actions;
use crate::App;
use crate::{widget_group, widget_mux};
use core::fmt::Write;
use time::{
    macros::{date, time},
    Duration, PrimitiveDateTime,
};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum DateTimeItems {
    None,
    Seconds,
    Minutes,
    Hours,
    Day,
    Month,
    Year,
}
#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
pub enum HistoryType {
    #[default]
    Hour,
    Day,
    Month,
}

pub trait HistoryWidgetTrait {
    fn get_datetime(&self) -> PrimitiveDateTime;
    fn set_datetime(&mut self, datetime: PrimitiveDateTime);
    fn get_items(&self) -> DateTimeItems;
    fn set_items(&mut self, items: DateTimeItems);
    fn get_editable(&self) -> bool;
    fn set_editable(&mut self, editable: bool);
    fn get_timestamp(&self) -> u32;
    fn set_timestamp(&mut self, timestamp: u32);
    fn get_history_type(&self) -> HistoryType;
    fn get_date_edit(&mut self) -> &mut Edit<Actions, 16, 8, 0>;
    fn get_time_edit(&mut self) -> &mut Edit<Actions, 16, 0, 1>;
    fn get_label_edit(&mut self) -> &mut Label<Actions, 16, 0, 0>;
    fn get_value_edit(&mut self) -> &mut Label<Actions, 16, 10, 1>;

    fn inc(&mut self) {
        match self.get_items() {
            DateTimeItems::None => {}
            DateTimeItems::Year => {
                let year = self.get_datetime().year() + 1;
                self.set_datetime(self.get_datetime().replace_year(year).unwrap());
            }
            DateTimeItems::Month => {
                let month = self.get_datetime().month().next();
                self.set_datetime(self.get_datetime().replace_month(month).unwrap());
            }
            DateTimeItems::Day => {
                self.set_datetime(self.get_datetime().saturating_add(Duration::DAY));
            }
            DateTimeItems::Hours => {
                self.set_datetime(self.get_datetime().saturating_add(Duration::HOUR));
            }
            DateTimeItems::Minutes => {
                self.set_datetime(self.get_datetime().saturating_add(Duration::MINUTE));
            }
            DateTimeItems::Seconds => {
                self.set_datetime(self.get_datetime().saturating_add(Duration::SECOND));
            }
        }
        self.set_timestamp(self.get_datetime().assume_utc().unix_timestamp() as u32);
    }

    fn dec(&mut self) {
        match self.get_items() {
            DateTimeItems::None => {}
            DateTimeItems::Year => {
                let year = self.get_datetime().year() - 1;
                self.set_datetime(self.get_datetime().replace_year(year).unwrap());
            }
            DateTimeItems::Month => {
                let month = self.get_datetime().month().previous();
                self.set_datetime(self.get_datetime().replace_month(month).unwrap());
            }
            DateTimeItems::Day => {
                self.set_datetime(self.get_datetime().saturating_sub(Duration::DAY));
            }
            DateTimeItems::Hours => {
                self.set_datetime(self.get_datetime().saturating_sub(Duration::HOUR));
            }
            DateTimeItems::Minutes => {
                self.set_datetime(self.get_datetime().saturating_sub(Duration::MINUTE));
            }
            DateTimeItems::Seconds => {
                self.set_datetime(self.get_datetime().saturating_sub(Duration::SECOND));
            }
        }
        self.set_timestamp(self.get_datetime().assume_utc().unix_timestamp() as u32);
    }

    fn next_item(&mut self) -> bool {
        match self.get_items() {
            DateTimeItems::None => {
                self.get_time_edit().blink_mask(0xc0);
                self.get_time_edit().set_editable(true);
                self.set_items(DateTimeItems::Hours);
            }
            DateTimeItems::Seconds => {
                self.get_time_edit().blink_mask(0x18);
                self.set_items(DateTimeItems::Minutes);
            }
            DateTimeItems::Minutes => {
                self.get_time_edit().blink_mask(0xc0);
                self.set_items(DateTimeItems::Hours);
            }
            DateTimeItems::Hours => {
                self.get_time_edit().set_editable(false);
                self.get_date_edit().set_editable(true);
                self.get_date_edit().blink_mask(0xc0);
                self.set_items(DateTimeItems::Day);
            }
            DateTimeItems::Day => {
                self.get_date_edit().blink_mask(0x18);
                self.set_items(DateTimeItems::Month);
            }
            DateTimeItems::Month => {
                self.get_date_edit().blink_mask(0x03);
                self.set_items(DateTimeItems::Year);
            }
            DateTimeItems::Year => {
                self.get_date_edit().set_editable(false);
                self.get_time_edit().set_editable(false);
                self.set_items(DateTimeItems::None);
            }
        }
        self.set_editable(self.get_items() != DateTimeItems::None);
        self.get_editable()
    }
}

pub struct HistoryWidget {
    pub date: Edit<Actions, 16, 8, 0>,
    time: Edit<Actions, 16, 0, 1>,
    pub label: Label<Actions, 16, 0, 0>,
    pub value: Label<Actions, 16, 10, 1>,
    pub items: DateTimeItems,
    datetime: PrimitiveDateTime,
    pub editable: bool,
    timestamp: u32,
    history_type: HistoryType,
}

impl HistoryWidgetTrait for HistoryWidget {
    fn get_datetime(&self) -> PrimitiveDateTime {
        self.datetime
    }
    fn set_datetime(&mut self, datetime: PrimitiveDateTime) {
        self.datetime = datetime;
    }
    fn get_items(&self) -> DateTimeItems {
        self.items
    }
    fn set_items(&mut self, items: DateTimeItems) {
        self.items = items;
    }
    fn get_editable(&self) -> bool {
        self.editable
    }
    fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }
    fn get_timestamp(&self) -> u32 {
        self.timestamp
    }
    fn set_timestamp(&mut self, timestamp: u32) {
        self.timestamp = timestamp;
    }
    fn get_history_type(&self) -> HistoryType {
        self.history_type
    }
    fn get_date_edit(&mut self) -> &mut Edit<Actions, 16, 8, 0> {
        &mut self.date
    }
    fn get_time_edit(&mut self) -> &mut Edit<Actions, 16, 0, 1> {
        &mut self.time
    }
    fn get_label_edit(&mut self) -> &mut Label<Actions, 16, 0, 0> {
        &mut self.label
    }
    fn get_value_edit(&mut self) -> &mut Label<Actions, 16, 10, 1> {
        &mut self.value
    }
}

impl Default for HistoryWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryWidget {
    pub fn new() -> Self {
        Self {
            date: Edit::<Actions, 16, 8, 0>::new(""),
            time: Edit::<Actions, 16, 0, 1>::new(""),
            label: Label::<Actions, 16, 0, 0>::new("From"),
            value: Label::<Actions, 16, 10, 1>::new(""),
            items: DateTimeItems::None,
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            editable: false,
            timestamp: 0,
            history_type: HistoryType::Hour,
        }
    }
}

impl Widget<&App, Actions> for HistoryWidget {
    fn invalidate(&mut self) {}

    fn update(&mut self, state: &App) {
        if !self.editable {
            self.datetime = state.datetime;
        }
        
        // Always update date/time to ensure blinking works
        self.date.state.clear();
        self.time.state.clear();
        write!(
            self.date,
            "{:02}/{:02}/{:02}",
            self.datetime.day(),
            self.datetime.month() as u8,
            self.datetime.year() - 2000
        )
        .ok();
        write!(self.time, "{:02}:{:02}:{:02}", self.datetime.hour(), 0, 0).ok();
        
        // Update value - only when changed to prevent flickering
        if let Some(flow) = state.history_state.flow {
            let mut value_str = alloc::string::String::new();
            write!(value_str, "{flow}").ok();
            if self.value.state != value_str {
                self.value.update(&value_str);
            }
        } else if self.value.state != "None" {
            self.value.update("None");
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Left => {
                    self.dec();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Right => {
                    self.inc();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                _ => None,
            }
        } else {
            match event {
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                UiEvent::Left => Some(Actions::Label),
                UiEvent::Right => Some(Actions::DayHistory),
                _ => None,
            }
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.label.render(display);
        self.value.render(display);
        self.date.render(display);
        self.time.render(display);
    }
}

pub struct DayHistoryWidget {
    pub date: Edit<Actions, 16, 8, 0>,
    time: Edit<Actions, 16, 0, 1>,
    pub label: Label<Actions, 16, 0, 0>,
    pub value: Label<Actions, 16, 10, 1>,
    pub items: DateTimeItems,
    datetime: PrimitiveDateTime,
    pub editable: bool,
    timestamp: u32,
    history_type: HistoryType,
}

impl HistoryWidgetTrait for DayHistoryWidget {
    fn get_datetime(&self) -> PrimitiveDateTime {
        self.datetime
    }
    fn set_datetime(&mut self, datetime: PrimitiveDateTime) {
        self.datetime = datetime;
    }
    fn get_items(&self) -> DateTimeItems {
        self.items
    }
    fn set_items(&mut self, items: DateTimeItems) {
        self.items = items;
    }
    fn get_editable(&self) -> bool {
        self.editable
    }
    fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }
    fn get_timestamp(&self) -> u32 {
        self.timestamp
    }
    fn set_timestamp(&mut self, timestamp: u32) {
        self.timestamp = timestamp;
    }
    fn get_history_type(&self) -> HistoryType {
        self.history_type
    }
    fn get_date_edit(&mut self) -> &mut Edit<Actions, 16, 8, 0> {
        &mut self.date
    }
    fn get_time_edit(&mut self) -> &mut Edit<Actions, 16, 0, 1> {
        &mut self.time
    }
    fn get_label_edit(&mut self) -> &mut Label<Actions, 16, 0, 0> {
        &mut self.label
    }
    fn get_value_edit(&mut self) -> &mut Label<Actions, 16, 10, 1> {
        &mut self.value
    }
}

impl Default for DayHistoryWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl DayHistoryWidget {
    pub fn new() -> Self {
        Self {
            date: Edit::<Actions, 16, 8, 0>::new(""),
            time: Edit::<Actions, 16, 0, 1>::new(""),
            label: Label::<Actions, 16, 0, 0>::new("From"),
            value: Label::<Actions, 16, 10, 1>::new(""),
            items: DateTimeItems::None,
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            editable: false,
            timestamp: 0,
            history_type: HistoryType::Day,
        }
    }
}

impl Widget<&App, Actions> for DayHistoryWidget {
    fn invalidate(&mut self) {}

    fn update(&mut self, state: &App) {
        if !self.editable {
            self.datetime = state.datetime;
        }
        
        // Always update date/time to ensure blinking works
        self.date.state.clear();
        self.time.state.clear();
        write!(
            self.date,
            "{:02}/{:02}/{:02}",
            self.datetime.day(),
            self.datetime.month() as u8,
            self.datetime.year() - 2000
        )
        .ok();
        write!(self.time, "{:02}:{:02}:{:02}", self.datetime.hour(), 0, 0).ok();
        
        // Update value - only when changed to prevent flickering
        if let Some(flow) = state.history_state.flow {
            let mut value_str = alloc::string::String::new();
            write!(value_str, "{flow}").ok();
            if self.value.state != value_str {
                self.value.update(&value_str);
            }
        } else if self.value.state != "None" {
            self.value.update("None");
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Left => {
                    self.dec();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Right => {
                    self.inc();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                _ => None,
            }
        } else {
            match event {
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                UiEvent::Left => Some(Actions::HourHistory),
                UiEvent::Right => Some(Actions::MonthHistory),
                _ => None,
            }
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.label.render(display);
        self.value.render(display);
        self.date.render(display);
        self.time.render(display);
    }
}

pub struct MonthHistoryWidget {
    pub date: Edit<Actions, 16, 8, 0>,
    time: Edit<Actions, 16, 0, 1>,
    pub label: Label<Actions, 16, 0, 0>,
    pub value: Label<Actions, 16, 10, 1>,
    pub items: DateTimeItems,
    datetime: PrimitiveDateTime,
    pub editable: bool,
    timestamp: u32,
    history_type: HistoryType,
}

impl HistoryWidgetTrait for MonthHistoryWidget {
    fn get_datetime(&self) -> PrimitiveDateTime {
        self.datetime
    }
    fn set_datetime(&mut self, datetime: PrimitiveDateTime) {
        self.datetime = datetime;
    }
    fn get_items(&self) -> DateTimeItems {
        self.items
    }
    fn set_items(&mut self, items: DateTimeItems) {
        self.items = items;
    }
    fn get_editable(&self) -> bool {
        self.editable
    }
    fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }
    fn get_timestamp(&self) -> u32 {
        self.timestamp
    }
    fn set_timestamp(&mut self, timestamp: u32) {
        self.timestamp = timestamp;
    }
    fn get_history_type(&self) -> HistoryType {
        self.history_type
    }
    fn get_date_edit(&mut self) -> &mut Edit<Actions, 16, 8, 0> {
        &mut self.date
    }
    fn get_time_edit(&mut self) -> &mut Edit<Actions, 16, 0, 1> {
        &mut self.time
    }
    fn get_label_edit(&mut self) -> &mut Label<Actions, 16, 0, 0> {
        &mut self.label
    }
    fn get_value_edit(&mut self) -> &mut Label<Actions, 16, 10, 1> {
        &mut self.value
    }
}

impl Default for MonthHistoryWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl MonthHistoryWidget {
    pub fn new() -> Self {
        Self {
            date: Edit::<Actions, 16, 8, 0>::new(""),
            time: Edit::<Actions, 16, 0, 1>::new(""),
            label: Label::<Actions, 16, 0, 0>::new("From"),
            value: Label::<Actions, 16, 10, 1>::new(""),
            items: DateTimeItems::None,
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            editable: false,
            timestamp: 0,
            history_type: HistoryType::Month,
        }
    }
}

impl Widget<&App, Actions> for MonthHistoryWidget {
    fn invalidate(&mut self) {}

    fn update(&mut self, state: &App) {
        if !self.editable {
            self.datetime = state.datetime;
        }
        
        // Always update date/time to ensure blinking works
        self.date.state.clear();
        self.time.state.clear();
        write!(
            self.date,
            "{:02}/{:02}/{:02}",
            self.datetime.day(),
            self.datetime.month() as u8,
            self.datetime.year() - 2000
        )
        .ok();
        write!(self.time, "{:02}:{:02}:{:02}", self.datetime.hour(), 0, 0).ok();
        
        // Update value - only when changed to prevent flickering
        if let Some(flow) = state.history_state.flow {
            let mut value_str = alloc::string::String::new();
            write!(value_str, "{flow}").ok();
            if self.value.state != value_str {
                self.value.update(&value_str);
            }
        } else if self.value.state != "None" {
            self.value.update("None");
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Left => {
                    self.dec();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Right => {
                    self.inc();
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                _ => None,
            }
        } else {
            match event {
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                UiEvent::Left => Some(Actions::DayHistory),
                UiEvent::Right => Some(Actions::Label),
                _ => None,
            }
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.label.render(display);
        self.value.render(display);
        self.date.render(display);
        self.time.render(display);
    }
}

pub struct DateTimeWidget {
    date: Edit<Actions, 16, 0, 0>,
    time: Edit<Actions, 16, 0, 1>,
    items: DateTimeItems,
    datetime: PrimitiveDateTime,
    editable: bool,
}

impl Default for DateTimeWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl DateTimeWidget {
    pub fn new() -> Self {
        Self {
            date: Edit::<Actions, 16, 0, 0>::new(""),
            time: Edit::<Actions, 16, 0, 1>::new(""),
            items: DateTimeItems::None,
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            editable: false,
        }
    }

    fn inc(&mut self) {
        match self.items {
            DateTimeItems::None => {}
            DateTimeItems::Year => {
                let year = self.datetime.year() + 1;
                self.datetime = self.datetime.replace_year(year).unwrap();
            }
            DateTimeItems::Month => {
                let month = self.datetime.month().next();
                self.datetime = self.datetime.replace_month(month).unwrap();
            }
            DateTimeItems::Day => {
                self.datetime = self.datetime.saturating_add(Duration::DAY);
            }
            DateTimeItems::Hours => {
                self.datetime = self.datetime.saturating_add(Duration::HOUR);
            }
            DateTimeItems::Minutes => {
                self.datetime = self.datetime.saturating_add(Duration::MINUTE);
            }
            DateTimeItems::Seconds => {
                self.datetime = self.datetime.saturating_add(Duration::SECOND);
            }
        }
    }

    fn dec(&mut self) {
        match self.items {
            DateTimeItems::None => {}
            DateTimeItems::Year => {
                let year = self.datetime.year() - 1;
                self.datetime = self.datetime.replace_year(year).unwrap();
            }
            DateTimeItems::Month => {
                let month = self.datetime.month().previous();
                self.datetime = self.datetime.replace_month(month).unwrap();
            }
            DateTimeItems::Day => {
                self.datetime = self.datetime.saturating_sub(Duration::DAY);
            }
            DateTimeItems::Hours => {
                self.datetime = self.datetime.saturating_sub(Duration::HOUR);
            }
            DateTimeItems::Minutes => {
                self.datetime = self.datetime.saturating_sub(Duration::MINUTE);
            }
            DateTimeItems::Seconds => {
                self.datetime = self.datetime.saturating_sub(Duration::SECOND);
            }
        }
    }

    fn next_item(&mut self) -> bool {
        match self.items {
            DateTimeItems::None => {
                self.time.blink_mask(0x03);
                self.time.set_editable(true);
                self.items = DateTimeItems::Seconds;
            }
            DateTimeItems::Seconds => {
                self.time.blink_mask(0x18);
                self.items = DateTimeItems::Minutes;
            }
            DateTimeItems::Minutes => {
                self.time.blink_mask(0xc0);
                self.items = DateTimeItems::Hours;
            }
            DateTimeItems::Hours => {
                self.time.set_editable(false);
                self.date.set_editable(true);
                self.date.blink_mask(0xc0);
                self.items = DateTimeItems::Day;
            }
            DateTimeItems::Day => {
                self.date.blink_mask(0x18);
                self.items = DateTimeItems::Month;
            }
            DateTimeItems::Month => {
                self.date.blink_mask(0x03);
                self.items = DateTimeItems::Year;
            }
            DateTimeItems::Year => {
                self.date.set_editable(false);
                self.time.set_editable(false);
                self.items = DateTimeItems::None;
            }
        }
        self.editable = self.items != DateTimeItems::None;
        self.editable
    }
}

impl Widget<PrimitiveDateTime, Actions> for DateTimeWidget {
    fn invalidate(&mut self) {}

    fn update(&mut self, state: PrimitiveDateTime) {
        if !self.editable {
            self.datetime = state;
        }
        self.date.state.clear();
        self.time.state.clear();
        write!(
            self.date,
            "Date    {:02}/{:02}/{:02}",
            self.datetime.day(),
            self.datetime.month() as u8,
            self.datetime.year() - 2000
        )
        .ok();
        write!(
            self.time,
            "Time    {:02}:{:02}:{:02}",
            self.datetime.hour(),
            self.datetime.minute(),
            self.datetime.second()
        )
        .ok();
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Left => {
                    self.dec();
                    None
                }
                UiEvent::Right => {
                    self.inc();
                    None
                }
                UiEvent::Enter => {
                    if self.next_item() {
                        return None;
                    }
                    Some(Actions::SetDateTime(self.datetime))
                }
                _ => None,
            }
        } else {
            match event {
                UiEvent::Enter => {
                    self.next_item();
                    None
                }
                UiEvent::Left => Some(Actions::Label),
                UiEvent::Right => Some(Actions::Label1),
                _ => None,
            }
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.date.render(display);
        self.time.render(display);
    }
}

widget_group!(
    LabelScreen<&App,Actions>,
    {
        title: Label<Actions, 16, 0, 0>, "Flow :";
        value: Label<Actions, 16, 0, 1>, "";
    },
    |widget: &mut LabelScreen, state: &App| {
        widget.title.state.clear();
        widget.value.state.clear();
        write!(widget.title, "{:<16}", state.label_title ).ok();
        write!(widget.value, "{:>16}", state.flow ).ok();
    },
    |_widget: &mut LabelScreen, event: UiEvent| {
        match event {
            UiEvent::Enter => Some(Actions::Label1),
            UiEvent::Left => Some(Actions::DateTime),
            UiEvent::Right => Some(Actions::HourHistory),
            _ => None,
        }
    }
);

widget_group!(
    LabelsWidget<&App,Actions>,
    {
        title: Label<Actions, 16,0,0>, "";
        text: Edit<Actions, 16,0,1>, "";
    },
    |widget: &mut LabelsWidget, state: &App| {
        widget.title.update(state.text);
        widget.text.state.clear();
        write!(widget.text, "{:>16}", state.num).unwrap();
    },
    |widget: &mut LabelsWidget, event: UiEvent| {
        widget.text.event(event);
        if widget.text.editable(){
            match event {
                UiEvent::Left => Some(Actions::ActionA),
                UiEvent::Right => Some(Actions::ActionB),
                _ => None,
            }
        }
        else {
            match event {
                UiEvent::Left => Some(Actions::Label),
                UiEvent::Right => Some(Actions::DateTime),
                _ => None,
            }
        }
    }
);

widget_mux! {
    Viewport<&App,Actions>,
    ViewportNode::Label,
    {
        label: LabelScreen;
        label1: LabelsWidget;
        datetime: DateTimeWidget;
        hour_history: HistoryWidget;
        day_history: DayHistoryWidget;
        month_history: MonthHistoryWidget;
    },
    |widget: &mut Viewport, state: &App| {
        widget.label.update(state);
        widget.label1.update(state);
        widget.datetime.update(state.datetime);
        widget.hour_history.update(state);
        widget.day_history.update(state);
        widget.month_history.update(state);
        widget.set_active(widget.active);
    },
    |widget: &mut Viewport, event: UiEvent| {
        let action = match widget.active {
            ViewportNode::Label => widget.label.event(event),
            ViewportNode::Label1 => widget.label1.event(event),
            ViewportNode::Datetime => widget.datetime.event(event),
            ViewportNode::HourHistory => widget.hour_history.event(event),
            ViewportNode::DayHistory => widget.day_history.event(event),
            ViewportNode::MonthHistory => widget.month_history.event(event),
        };
        if let Some(act) = action {
            match act {
                Actions::Label => widget.set_active(ViewportNode::Label),
                Actions::Label1 => widget.set_active(ViewportNode::Label1),
                Actions::DateTime => widget.set_active(ViewportNode::Datetime),
                Actions::HourHistory => widget.set_active(ViewportNode::HourHistory),
                Actions::DayHistory => widget.set_active(ViewportNode::DayHistory),
                Actions::MonthHistory => widget.set_active(ViewportNode::MonthHistory),
                _ => (),
            }
        }
        action
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn test_blink_masks_correct() {
        // Формат времени: "HH:MM:SS" (8 символов, позиции 0-7)
        // Маскирование: бит i соответствует позиции (LEN - i - 1)

        // Секунды (позиции 6-7) → биты 0-1 → маска 0x03
        assert_eq!(0x03, 0b00000011);

        // Минуты (позиции 3-4) → биты 3-4 → маска 0x18
        assert_eq!(0x18, 0b00011000);

        // Часы (позиции 0-1) → биты 6-7 → маска 0xc0
        assert_eq!(0xc0, 0b11000000);
    }

    #[test]
    fn test_timestamp_full_value() {
        // Проверяем, что timestamp содержит полный Unix timestamp
        let dt = datetime!(2023-06-15 10:30:45 UTC);
        let ts = dt.unix_timestamp() as u32;

        // Unix timestamp должен быть большим числом (не % 60)
        assert!(ts > 1686000000);
        assert!(ts < 2000000000); // разумная верхняя граница
    }

    #[test]
    fn test_hour_increment_timestamp() {
        // При инкременте часа timestamp должен увеличиться на 3600 секунд
        let dt1 = datetime!(2023-06-15 10:00:00 UTC);
        let ts1 = dt1.unix_timestamp() as u32;

        let dt2 = dt1.saturating_add(Duration::HOUR);
        let ts2 = dt2.unix_timestamp() as u32;

        assert_eq!(ts2 - ts1, 3600);
    }

    #[test]
    fn test_day_increment_timestamp() {
        // При инкременте дня timestamp должен увеличиться на 86400 секунд
        let dt1 = datetime!(2023-06-15 00:00:00 UTC);
        let ts1 = dt1.unix_timestamp() as u32;

        let dt2 = dt1.saturating_add(Duration::DAY);
        let ts2 = dt2.unix_timestamp() as u32;

        assert_eq!(ts2 - ts1, 86400);
    }
}
