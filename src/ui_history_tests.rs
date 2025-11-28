// ============================================================================
// UI HISTORY TESTS WITH MOCKS
// ============================================================================

#[cfg(test)]
mod ui_history_tests {
    use crate::gui::{CharacterDisplay, UiEvent, Widget};
    use crate::ui::{
        DateTimeItems, DayHistoryWidget, HistoryType, HistoryWidget, HistoryWidgetTrait,
        MonthHistoryWidget,
    };
    use crate::{Actions, App};
    use alloc::string::String;
    use core::fmt::Write;
    use time::macros::{date, time};
    use time::PrimitiveDateTime;

    // ========================================================================
    // MOCK DISPLAY
    // ========================================================================

    #[derive(Debug, Clone)]
    struct MockHistoryDisplay {
        buffer: String,
        position: (u8, u8),
        clear_count: usize,
        write_count: usize,
    }

    impl MockHistoryDisplay {
        fn new() -> Self {
            Self {
                buffer: String::new(),
                position: (0, 0),
                clear_count: 0,
                write_count: 0,
            }
        }

        fn get_content(&self) -> &str {
            &self.buffer
        }

        fn assertions(&self) -> MockHistoryDisplayAssertions {
            MockHistoryDisplayAssertions {
                buffer: self.buffer.clone(),
                position: self.position,
                clear_count: self.clear_count,
                write_count: self.write_count,
            }
        }
    }

    struct MockHistoryDisplayAssertions {
        buffer: String,
        position: (u8, u8),
        clear_count: usize,
        write_count: usize,
    }

    impl MockHistoryDisplayAssertions {
        fn contains(&self, text: &str) -> bool {
            self.buffer.contains(text)
        }

        fn position_is(&self, col: u8, row: u8) -> bool {
            self.position == (col, row)
        }

        fn cleared(&self) -> bool {
            self.clear_count > 0
        }
    }

    impl CharacterDisplay for MockHistoryDisplay {
        fn set_position(&mut self, col: u8, row: u8) {
            self.position = (col, row);
        }

        fn clear(&mut self) {
            self.buffer.clear();
            self.clear_count += 1;
        }

        fn reset_custom_chars(&mut self) {
            // Mock implementation
        }
    }

    impl Write for MockHistoryDisplay {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            self.write_count += 1;
            self.buffer.push_str(s);
            Ok(())
        }
    }

    // ========================================================================
    // MOCK APP STATE
    // ========================================================================

    struct MockAppState {
        datetime: PrimitiveDateTime,
        history_state_flow: Option<f32>,
    }

    impl MockAppState {
        fn new() -> Self {
            Self {
                datetime: PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45)),
                history_state_flow: Some(42.5),
            }
        }

        fn with_flow(mut self, flow: f32) -> Self {
            self.history_state_flow = Some(flow);
            self
        }

        fn with_no_flow(mut self) -> Self {
            self.history_state_flow = None;
            self
        }

        fn to_app(&self) -> App {
            let mut app = App::new();
            app.datetime = self.datetime;
            app.history_state.flow = self.history_state_flow;
            app
        }
    }

    // ========================================================================
    // DATETIME ITEMS TRANSITION TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_datetime_items_none_to_seconds() {
        let mut widget = HistoryWidget::new();
        assert_eq!(widget.get_items(), DateTimeItems::None);

        let result = widget.next_item();
        assert_eq!(widget.get_items(), DateTimeItems::Seconds);
        assert!(result);
    }

    #[test]
    fn test_history_widget_datetime_items_progression() {
        let mut widget = HistoryWidget::new();
        let items_sequence = vec![
            DateTimeItems::None,
            DateTimeItems::Seconds,
            DateTimeItems::Minutes,
            DateTimeItems::Hours,
            DateTimeItems::Day,
            DateTimeItems::Month,
            DateTimeItems::Year,
            DateTimeItems::None,
        ];

        for expected in items_sequence.iter() {
            assert_eq!(widget.get_items(), *expected);
            widget.next_item();
        }
    }

    #[test]
    fn test_day_history_widget_items_progression() {
        let mut widget = DayHistoryWidget::new();
        widget.next_item(); // None -> Seconds
        assert_eq!(widget.get_items(), DateTimeItems::Seconds);

        widget.next_item(); // Seconds -> Minutes
        assert_eq!(widget.get_items(), DateTimeItems::Minutes);

        widget.next_item(); // Minutes -> Hours
        assert_eq!(widget.get_items(), DateTimeItems::Hours);

        widget.next_item(); // Hours -> Day
        assert_eq!(widget.get_items(), DateTimeItems::Day);
    }

    #[test]
    fn test_month_history_widget_items_progression() {
        let mut widget = MonthHistoryWidget::new();
        for _ in 0..5 {
            widget.next_item();
        }
        assert_eq!(widget.get_items(), DateTimeItems::Month);
    }

    // ========================================================================
    // INCREMENT/DECREMENT TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_increment_seconds() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Seconds);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.second(), 46);
    }

    #[test]
    fn test_history_widget_increment_minutes() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Minutes);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.minute(), 31);
    }

    #[test]
    fn test_history_widget_increment_hours() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Hours);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.hour(), 15);
    }

    #[test]
    fn test_history_widget_increment_day() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Day);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.day(), 16);
    }

    #[test]
    fn test_history_widget_increment_month() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Month);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.month() as u8, 2);
    }

    #[test]
    fn test_history_widget_increment_year() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Year);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.year(), 2024);
    }

    #[test]
    fn test_history_widget_decrement_seconds() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Seconds);

        widget.dec();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.second(), 44);
    }

    #[test]
    fn test_history_widget_decrement_minutes() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Minutes);

        widget.dec();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.minute(), 29);
    }

    #[test]
    fn test_history_widget_decrement_day() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Day);

        widget.dec();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.day(), 14);
    }

    // ========================================================================
    // UPDATE TESTS WITH MOCK APP STATE
    // ========================================================================

    #[test]
    fn test_history_widget_update_with_flow() {
        let mut widget = HistoryWidget::new();
        let app_state = MockAppState::new().with_flow(123.45);
        let app = app_state.to_app();

        widget.update(&app);

        assert!(widget.value.state.contains("123.45"));
    }

    #[test]
    fn test_history_widget_update_no_flow() {
        let mut widget = HistoryWidget::new();
        let app_state = MockAppState::new().with_no_flow();
        let app = app_state.to_app();

        widget.update(&app);

        assert!(widget.value.state.contains("None"));
    }

    #[test]
    fn test_history_widget_update_date_format() {
        let mut widget = HistoryWidget::new();
        let app_state = MockAppState::new();
        let app = app_state.to_app();

        widget.update(&app);

        // Should have updated the date state with something
        assert!(!widget.date.state.is_empty());
        // Date should contain slashes (DD/MM/YY format)
        assert!(widget.date.state.contains("/"));
    }

    #[test]
    fn test_day_history_widget_update_with_flow() {
        let mut widget = DayHistoryWidget::new();
        let app_state = MockAppState::new().with_flow(99.99);
        let app = app_state.to_app();

        widget.update(&app);

        assert!(widget.value.state.contains("99.99"));
    }

    #[test]
    fn test_month_history_widget_update_with_flow() {
        let mut widget = MonthHistoryWidget::new();
        let app_state = MockAppState::new().with_flow(55.0);
        let app = app_state.to_app();

        widget.update(&app);

        assert!(widget.value.state.contains("55"));
    }

    // ========================================================================
    // EVENT HANDLING TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_event_left_editable() {
        let mut widget = HistoryWidget::new();
        widget.set_editable(true);
        widget.set_timestamp(1000);

        let result = widget.event(UiEvent::Left);

        assert!(result.is_some());
        assert!(matches!(
            result,
            Some(Actions::SetHistory(HistoryType::Hour, _))
        ));
    }

    #[test]
    fn test_history_widget_event_right_editable() {
        let mut widget = HistoryWidget::new();
        widget.set_editable(true);
        widget.set_timestamp(1000);

        let result = widget.event(UiEvent::Right);

        assert!(result.is_some());
        assert!(matches!(
            result,
            Some(Actions::SetHistory(HistoryType::Hour, _))
        ));
    }

    #[test]
    fn test_history_widget_event_enter_editable() {
        let mut widget = HistoryWidget::new();
        widget.set_editable(true);

        let result = widget.event(UiEvent::Enter);

        assert!(result.is_none());
        assert_eq!(widget.get_items(), DateTimeItems::Seconds);
    }

    #[test]
    fn test_day_history_widget_event_left_not_editable() {
        let mut widget = DayHistoryWidget::new();
        widget.set_editable(false);

        let result = widget.event(UiEvent::Left);

        assert!(result.is_some());
        assert!(matches!(result, Some(Actions::HourHistory)));
    }

    #[test]
    fn test_day_history_widget_event_right_not_editable() {
        let mut widget = DayHistoryWidget::new();
        widget.set_editable(false);

        let result = widget.event(UiEvent::Right);

        assert!(result.is_some());
        assert!(matches!(result, Some(Actions::MonthHistory)));
    }

    #[test]
    fn test_month_history_widget_event_left_not_editable() {
        let mut widget = MonthHistoryWidget::new();
        widget.set_editable(false);

        let result = widget.event(UiEvent::Left);

        assert!(result.is_some());
        assert!(matches!(result, Some(Actions::DayHistory)));
    }

    #[test]
    fn test_month_history_widget_event_right_not_editable() {
        let mut widget = MonthHistoryWidget::new();
        widget.set_editable(false);

        let result = widget.event(UiEvent::Right);

        assert!(result.is_some());
        assert!(matches!(result, Some(Actions::Label)));
    }

    #[test]
    fn test_history_widget_event_ignored() {
        let mut widget = HistoryWidget::new();
        widget.set_editable(true);

        let result = widget.event(UiEvent::Back);

        assert!(result.is_none());
    }

    // ========================================================================
    // TIMESTAMP TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_timestamp_update_on_inc() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Hours);
        widget.set_timestamp(initial_dt.assume_utc().unix_timestamp() as u32);

        widget.inc();

        let expected_ts = widget.get_datetime().assume_utc().unix_timestamp() as u32;
        assert_eq!(widget.get_timestamp(), expected_ts);
    }

    #[test]
    fn test_history_widget_timestamp_update_on_dec() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Day);
        widget.set_timestamp(initial_dt.assume_utc().unix_timestamp() as u32);

        widget.dec();

        let expected_ts = widget.get_datetime().assume_utc().unix_timestamp() as u32;
        assert_eq!(widget.get_timestamp(), expected_ts);
    }

    #[test]
    fn test_day_history_widget_timestamp_consistency() {
        let mut widget = DayHistoryWidget::new();
        let initial_ts = 1673792400u32; // 2023-01-15 14:00:00 UTC

        widget.set_timestamp(initial_ts);

        // After setting timestamp, it should remain consistent
        assert_eq!(widget.get_timestamp(), initial_ts);
    }

    // ========================================================================
    // RENDER TESTS WITH MOCK DISPLAY
    // ========================================================================

    #[test]
    fn test_history_widget_render_displays_content() {
        let mut widget = HistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new();
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        assert!(!display.get_content().is_empty());
    }

    #[test]
    fn test_history_widget_render_contains_date() {
        let mut widget = HistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new();
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        // Check that date is rendered with slashes (DD/MM/YY format)
        let content = display.get_content();
        assert!(!content.is_empty());
        assert!(content.contains("/"));
    }

    #[test]
    fn test_history_widget_render_contains_flow_value() {
        let mut widget = HistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new().with_flow(42.5);
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        assert!(display.assertions().contains("42.5"));
    }

    #[test]
    fn test_day_history_widget_render() {
        let mut widget = DayHistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new().with_flow(10.0);
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        assert!(!display.get_content().is_empty());
        assert!(display.assertions().contains("10"));
    }

    #[test]
    fn test_month_history_widget_render() {
        let mut widget = MonthHistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new().with_flow(5.5);
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        assert!(!display.get_content().is_empty());
        assert!(display.assertions().contains("5.5"));
    }

    // ========================================================================
    // EDITABLE STATE TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_editable_after_creation() {
        let widget = HistoryWidget::new();
        assert!(!widget.get_editable());
    }

    #[test]
    fn test_history_widget_toggle_editable() {
        let mut widget = HistoryWidget::new();
        assert!(!widget.get_editable());

        widget.set_editable(true);
        assert!(widget.get_editable());

        widget.set_editable(false);
        assert!(!widget.get_editable());
    }

    #[test]
    fn test_day_history_widget_editable_state() {
        let mut widget = DayHistoryWidget::new();
        widget.set_editable(false);
        assert!(!widget.get_editable());

        widget.next_item(); // Transitions through items
        assert!(widget.get_editable());
    }

    // ========================================================================
    // HISTORY TYPE TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_type_is_hour() {
        let widget = HistoryWidget::new();
        assert_eq!(widget.get_history_type(), HistoryType::Hour);
    }

    #[test]
    fn test_day_history_widget_type_is_day() {
        let widget = DayHistoryWidget::new();
        assert_eq!(widget.get_history_type(), HistoryType::Day);
    }

    #[test]
    fn test_month_history_widget_type_is_month() {
        let widget = MonthHistoryWidget::new();
        assert_eq!(widget.get_history_type(), HistoryType::Month);
    }

    // ========================================================================
    // COMPLEX INTERACTION TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_full_edit_sequence() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);

        // Start editing
        assert!(widget.next_item()); // None -> Seconds
        assert_eq!(widget.get_items(), DateTimeItems::Seconds);

        // Increment seconds
        widget.inc();
        assert_eq!(widget.get_datetime().second(), 46);

        // Move to next item
        assert!(widget.next_item()); // Seconds -> Minutes
        assert_eq!(widget.get_items(), DateTimeItems::Minutes);

        // Increment minutes
        widget.inc();
        assert_eq!(widget.get_datetime().minute(), 31);

        // Move to next item
        assert!(widget.next_item()); // Minutes -> Hours
        assert_eq!(widget.get_items(), DateTimeItems::Hours);

        // Increment hours
        widget.inc();
        assert_eq!(widget.get_datetime().hour(), 15);
    }

    #[test]
    fn test_day_history_widget_left_right_navigation_sequence() {
        let mut widget = DayHistoryWidget::new();
        widget.set_editable(false);

        // Left from Day should go to Hour
        let left_result = widget.event(UiEvent::Left);
        assert!(matches!(left_result, Some(Actions::HourHistory)));

        // Reset to not editable state
        widget.set_editable(false);

        // Right from Day should go to Month
        let right_result = widget.event(UiEvent::Right);
        assert!(matches!(right_result, Some(Actions::MonthHistory)));
    }

    #[test]
    fn test_history_widget_month_wraparound() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 11 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Month);

        widget.inc();

        // November + 1 month should be December of same year
        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.month() as u8, 12);
        assert_eq!(new_dt.year(), 2023);
    }

    #[test]
    fn test_history_widget_multiple_increments() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::Seconds);

        for _ in 0..10 {
            widget.inc();
        }

        // After 10 increments, should be at second 55 (45 + 10)
        assert_eq!(widget.get_datetime().second(), 55);
    }

    #[test]
    fn test_history_widget_update_respects_editable_state() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_editable(true);

        let mut app = App::new();
        app.datetime = PrimitiveDateTime::new(date!(2023 - 06 - 20), time!(10:00:00));

        widget.update(&app);

        // When editable is true, datetime should not change from app state
        assert_eq!(widget.get_datetime(), initial_dt);

        widget.set_editable(false);
        widget.update(&app);

        // When editable is false, datetime should come from app state
        assert_eq!(widget.get_datetime(), app.datetime);
    }

    #[test]
    fn test_history_widget_none_item_does_nothing() {
        let mut widget = HistoryWidget::new();
        let initial_dt = PrimitiveDateTime::new(date!(2023 - 01 - 15), time!(14:30:45));
        widget.set_datetime(initial_dt);
        widget.set_items(DateTimeItems::None);

        widget.inc();
        assert_eq!(widget.get_datetime(), initial_dt);

        widget.dec();
        assert_eq!(widget.get_datetime(), initial_dt);
    }

    // ========================================================================
    // EDGE CASE TESTS
    // ========================================================================

    #[test]
    fn test_history_widget_leap_year_handling() {
        let mut widget = HistoryWidget::new();
        let leap_year_dt = PrimitiveDateTime::new(date!(2024 - 02 - 28), time!(14:30:45));
        widget.set_datetime(leap_year_dt);
        widget.set_items(DateTimeItems::Day);

        widget.inc();

        // Should increment to Feb 29 (leap year)
        assert_eq!(widget.get_datetime().day(), 29);
    }

    #[test]
    fn test_history_widget_year_boundary() {
        let mut widget = HistoryWidget::new();
        let end_of_year = PrimitiveDateTime::new(date!(2023 - 12 - 31), time!(23:59:59));
        widget.set_datetime(end_of_year);
        widget.set_items(DateTimeItems::Seconds);

        widget.inc();

        let new_dt = widget.get_datetime();
        assert_eq!(new_dt.year(), 2024);
        assert_eq!(new_dt.month() as u8, 1);
        assert_eq!(new_dt.day(), 1);
        assert_eq!(new_dt.second(), 0);
    }

    #[test]
    fn test_history_widget_render_multiple_times() {
        let mut widget = HistoryWidget::new();
        let mut display = MockHistoryDisplay::new();
        let app_state = MockAppState::new();
        let app = app_state.to_app();

        widget.update(&app);
        widget.render(&mut display);

        // Update with different flow value
        let new_app_state = MockAppState::new().with_flow(99.99);
        let new_app = new_app_state.to_app();
        widget.update(&new_app);
        widget.render(&mut display);

        let second_content = display.get_content();

        // Second render should contain the new flow value
        assert!(second_content.contains("99.99"));
    }

    #[test]
    fn test_history_widget_event_returns_set_history_action() {
        let mut widget = HistoryWidget::new();
        widget.set_editable(true);
        widget.set_datetime(PrimitiveDateTime::new(
            date!(2023 - 01 - 15),
            time!(14:30:45),
        ));
        widget.set_items(DateTimeItems::Hours);

        let result = widget.event(UiEvent::Right);

        assert!(result.is_some());
        match result {
            Some(Actions::SetHistory(history_type, timestamp)) => {
                assert_eq!(history_type, HistoryType::Hour);
                assert!(timestamp > 0);
            }
            _ => panic!("Expected SetHistory action"),
        }
    }
}
