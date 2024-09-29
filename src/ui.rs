use crate::Actions;
use crate::App;
use crate::CharacterDisplay;
use crate::Edit;
use crate::Label;
use crate::UiEvent;
use crate::Widget;
use crate::{widget_group, widget_mux};
use core::fmt::Write;
use time::{
    macros::{date, time},
    Duration, PrimitiveDateTime,
};

#[derive(PartialEq, Eq)]
pub enum DateTimeItems {
    None,
    Seconds,
    Minutes,
    Hours,
    Day,
    Month,
    Year,
}

pub struct DateTimeWidget {
    date: Edit<Actions, 16, 0, 0>,
    time: Edit<Actions, 16, 0, 1>,
    items: DateTimeItems,
    datetime: PrimitiveDateTime,
    editable: bool,
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
        title: Label<Actions, 16, 0, 0>, "";
        value: Label<Actions, 16, 0, 1>, "";
    },
    |widget: &mut LabelScreen, state: &App| {
        widget.title.state.clear();
        widget.value.state.clear();
        write!(widget.title, "{:^16}", state.label_title ).ok();
        write!(widget.value, "{:^16}", state.label_value ).ok();
    },
    |_widget: &mut LabelScreen, event: UiEvent| {
        match event {
            UiEvent::Enter => Some(Actions::Label1),
            UiEvent::Left => Some(Actions::DateTime),
            UiEvent::Right => Some(Actions::Label),
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
    },
    |widget: &mut Viewport, state: &App| {
        widget.label.update(state);
        widget.label1.update(state);
        widget.datetime.update(state.datetime);
        widget.set_active(widget.active);
    },
    |widget: &mut Viewport, event: UiEvent| {
        let action = match widget.active {
            ViewportNode::Label => widget.label.event(event),
            ViewportNode::Label1 => widget.label1.event(event),
            ViewportNode::Datetime => widget.datetime.event(event),
        };
        if let Some(act) = action {
            match act {
                Actions::Label => widget.set_active(ViewportNode::Label),
                Actions::Label1 => widget.set_active(ViewportNode::Label1),
                Actions::DateTime => widget.set_active(ViewportNode::Datetime),
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
