use crate::gui::{CharacterDisplay, Edit, Label, UiEvent, Widget};
use crate::gui::date_time_widget::DateTimeItems;

#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
pub enum HistoryType {
    #[default]
    Hour,
    Day,
    Month,
}
use crate::App;
use crate::Actions;
use core::fmt::Write;
use core::marker::PhantomData;
use time::{
    macros::{date, time},
    Duration, PrimitiveDateTime,
};

pub trait HistoryKind {
    fn history_type() -> HistoryType;
    fn nav_left() -> Actions;
    fn nav_right() -> Actions;
}

pub struct HourKind;
pub struct DayKind;
pub struct MonthKind;

impl HistoryKind for HourKind {
    fn history_type() -> HistoryType { HistoryType::Hour }
    fn nav_left() -> Actions { Actions::Label }
    fn nav_right() -> Actions { Actions::DayHistory }
}

impl HistoryKind for DayKind {
    fn history_type() -> HistoryType { HistoryType::Day }
    fn nav_left() -> Actions { Actions::HourHistory }
    fn nav_right() -> Actions { Actions::MonthHistory }
}

impl HistoryKind for MonthKind {
    fn history_type() -> HistoryType { HistoryType::Month }
    fn nav_left() -> Actions { Actions::DayHistory }
    fn nav_right() -> Actions { Actions::Label }
}

pub struct HistoryWidget<K: HistoryKind> {
    pub date: Edit<Actions, 16, 8, 0>,
    pub time: Edit<Actions, 16, 8, 1>,
    pub label: Label<Actions, 16, 0, 0>,
    pub value: Label<Actions, 8, 0, 1>,
    pub items: DateTimeItems,
    datetime: PrimitiveDateTime,
    pub editable: bool,
    timestamp: u32,
    first_render: bool,
    _kind: PhantomData<K>,
}

impl<K: HistoryKind> Default for HistoryWidget<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: HistoryKind> HistoryWidget<K> {
    pub fn new() -> Self {
        Self {
            date: Edit::<Actions, 16, 8, 0>::new(""),
            time: Edit::<Actions, 16, 8, 1>::new(""),
            label: Label::<Actions, 16, 0, 0>::new("From"),
            value: Label::<Actions, 8, 0, 1>::new(""),
            items: DateTimeItems::None,
            datetime: PrimitiveDateTime::new(date!(2023 - 01 - 01), time!(00:00:00)),
            editable: false,
            timestamp: 0,
            first_render: true,
            _kind: PhantomData,
        }
    }

    pub fn get_timestamp(&self) -> u32 {
        self.timestamp
    }

    pub fn get_datetime(&self) -> PrimitiveDateTime {
        self.datetime
    }

    pub fn set_timestamp(&mut self, timestamp: u32) {
        self.timestamp = timestamp;
    }

    pub fn get_items(&self) -> DateTimeItems {
        self.items
    }

    pub fn set_items(&mut self, items: DateTimeItems) {
        self.items = items;
    }

    pub fn set_datetime(&mut self, datetime: PrimitiveDateTime) {
        self.datetime = datetime;
    }

    pub fn get_editable(&self) -> bool {
        self.editable
    }

    pub fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }

    pub fn get_history_type(&self) -> HistoryType {
        K::history_type()
    }

    pub fn inc(&mut self) {
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
        self.timestamp = self.datetime.assume_utc().unix_timestamp() as u32;
    }

    pub fn dec(&mut self) {
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
        self.timestamp = self.datetime.assume_utc().unix_timestamp() as u32;
    }

    pub fn next_item(&mut self) -> bool {
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

impl<K: HistoryKind> Widget<&App, Actions> for HistoryWidget<K> {
    fn invalidate(&mut self) {
        self.label.invalidate();
        self.value.invalidate();
        self.date.invalidate();
        self.time.invalidate();
        self.first_render = true;
    }

    fn update(&mut self, state: &App) {
        #[cfg(not(test))]
        defmt::debug!("HistoryWidget::update called, editable={}", self.editable);
        if !self.editable {
            self.datetime = state.datetime;
        }

        // Update date/time text ONLY when not in editable mode to prevent interfering with blinking
        if !self.date.editable {
            self.date.state.clear();
            write!(
                self.date,
                "{:02}/{:02}/{:02}",
                self.datetime.day(),
                self.datetime.month() as u8,
                self.datetime.year() - 2000
            )
            .ok();
        }
        if !self.time.editable {
            self.time.state.clear();
            write!(
                self.time,
                "{:>8}",
                alloc::format!("{:02}:00:00", self.datetime.hour())
            )
            .ok();
        }
        #[cfg(not(test))]
        defmt::debug!("HistoryWidget date.editable={}, date.invalidate={}, time.editable={}, time.invalidate={}",
            self.date.editable, self.date.invalidate, self.time.editable, self.time.invalidate);

        // Update value - only when changed to prevent flickering
        if let Some(flow) = state.history_state.flow {
            let mut value_str = alloc::string::String::new();
            write!(value_str, "{flow}").ok();
            if self.value.state != value_str {
                self.value.update(&value_str);
            }
            #[cfg(not(test))]
            defmt::debug!("HistoryWidget flow value: {}", flow);
        } else if self.value.state != "None" {
            self.value.update("None");
            #[cfg(not(test))]
            defmt::debug!("HistoryWidget flow value: None");
        }
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        #[cfg(not(test))]
        defmt::info!(
            "HistoryWidget::event, editable={}, event={}",
            self.editable,
            event
        );
        if self.editable {
            match event {
                UiEvent::Left => {
                    self.dec();
                    Some(Actions::SetHistory(K::history_type(), self.timestamp))
                }
                UiEvent::Right => {
                    self.inc();
                    Some(Actions::SetHistory(K::history_type(), self.timestamp))
                }
                UiEvent::Enter => {
                    #[cfg(not(test))]
                    defmt::info!(
                        "HistoryWidget: Enter pressed in editable mode, calling next_item()"
                    );
                    self.next_item();
                    #[cfg(not(test))]
                    defmt::info!(
                        "HistoryWidget: after next_item(), editable={}, items={:?}",
                        self.editable,
                        self.items
                    );
                    None
                }
                _ => None,
            }
        } else {
            match event {
                UiEvent::Enter => {
                    #[cfg(not(test))]
                    defmt::info!(
                        "HistoryWidget: Enter pressed in non-editable mode, calling next_item()"
                    );
                    self.next_item();
                    #[cfg(not(test))]
                    defmt::info!(
                        "HistoryWidget: after next_item(), editable={}, items={:?}",
                        self.editable,
                        self.items
                    );
                    None
                }
                UiEvent::Left => Some(K::nav_left()),
                UiEvent::Right => Some(K::nav_right()),
                _ => None,
            }
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        #[cfg(not(test))]
        defmt::debug!(
            "HistoryWidget::render called, editable={}, first_render={}",
            self.editable,
            self.first_render
        );
        if self.first_render {
            display.clear();
            self.first_render = false;
        }
        #[cfg(not(test))]
        defmt::trace!("HistoryWidget: before render - date.editable={}, date.blink_state={}, date.invalidate={}",
            self.date.editable, self.date.blink_state, self.date.invalidate);
        #[cfg(not(test))]
        defmt::trace!("HistoryWidget: before render - time.editable={}, time.blink_state={}, time.invalidate={}",
            self.time.editable, self.time.blink_state, self.time.invalidate);
        self.label.render(display);
        self.value.render(display);
        self.date.render(display);
        self.time.render(display);
        #[cfg(not(test))]
        defmt::trace!("HistoryWidget: after render - date.editable={}, date.blink_state={}, date.invalidate={}",
            self.date.editable, self.date.blink_state, self.date.invalidate);
        #[cfg(not(test))]
        defmt::trace!("HistoryWidget: after render - time.editable={}, time.blink_state={}, time.invalidate={}",
            self.time.editable, self.time.blink_state, self.time.invalidate);
    }
}
