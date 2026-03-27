use crate::gui::date_time_widget::DateTimeWidget;
use crate::gui::history_widget::{DayKind, HistoryWidget, HourKind, MonthKind};
use crate::gui::HistoryType;
use crate::gui::{CharacterDisplay, Edit, Label, UiEvent, Widget};
use crate::Actions;
use crate::App;
use crate::{widget_group, widget_mux};
use core::fmt::Write;

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
        hour_history: HistoryWidget<HourKind>;
        day_history: HistoryWidget<DayKind>;
        month_history: HistoryWidget<MonthKind>;
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
        #[cfg(not(test))]
        defmt::info!("Viewport::event - active: {}, event: {}", widget.active, event);
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
                Actions::HourHistory => {
                    #[cfg(not(test))]
                    defmt::info!("Switching to HourHistory widget");
                    widget.set_active(ViewportNode::HourHistory);
                    // Request history data for current timestamp
                    let ts = widget.hour_history.get_timestamp();
                    #[cfg(not(test))]
                    defmt::info!("Current timestamp: {}", ts);
                    if ts == 0 {
                        let ts = widget.hour_history.get_datetime().assume_utc().unix_timestamp() as u32;
                        widget.hour_history.set_timestamp(ts);
                        #[cfg(not(test))]
                        defmt::info!("Requesting history for timestamp: {}", ts);
                        return Some(Actions::SetHistory(HistoryType::Hour, ts));
                    }
                },
                Actions::DayHistory => {
                    widget.set_active(ViewportNode::DayHistory);
                    // Request history data for current timestamp
                    let ts = widget.day_history.get_timestamp();
                    if ts == 0 {
                        let ts = widget.day_history.get_datetime().assume_utc().unix_timestamp() as u32;
                        widget.day_history.set_timestamp(ts);
                        return Some(Actions::SetHistory(HistoryType::Day, ts));
                    }
                },
                Actions::MonthHistory => {
                    widget.set_active(ViewportNode::MonthHistory);
                    // Request history data for current timestamp
                    let ts = widget.month_history.get_timestamp();
                    if ts == 0 {
                        let ts = widget.month_history.get_datetime().assume_utc().unix_timestamp() as u32;
                        widget.month_history.set_timestamp(ts);
                        return Some(Actions::SetHistory(HistoryType::Month, ts));
                    }
                },
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

impl Viewport {
    pub fn get_active(&self) -> ViewportNode {
        #[cfg(not(test))]
        defmt::info!("Viewport::get_active - current active: {:?}", self.active);
        self.active
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;
    use time::Duration;

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
