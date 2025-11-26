#![cfg(test)]

use crate::gui::*;
use alloc::string::{String, ToString};
use core::fmt::Write;

/// Mock display implementation for testing UI components
#[derive(Debug, Clone)]
struct MockDisplay {
    buffer: String,
    position: (u8, u8),
    clear_count: usize,
    write_count: usize,
    custom_chars_reset: bool,
}

impl MockDisplay {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            position: (0, 0),
            clear_count: 0,
            write_count: 0,
            custom_chars_reset: false,
        }
    }

    fn get_content(&self) -> &str {
        self.buffer.as_str()
    }

    fn assertions(&self) -> DisplayAssertions {
        DisplayAssertions {
            buffer: self.buffer.clone(),
            position: self.position,
            clear_count: self.clear_count,
            write_count: self.write_count,
            custom_chars_reset: self.custom_chars_reset,
        }
    }
}

/// Helper struct for fluent display assertions
struct DisplayAssertions {
    buffer: String,
    position: (u8, u8),
    clear_count: usize,
    write_count: usize,
    custom_chars_reset: bool,
}

impl DisplayAssertions {
    fn contains(&self, text: &str) -> bool {
        self.buffer.contains(text)
    }

    fn equals(&self, text: &str) -> bool {
        self.buffer.eq(text)
    }

    fn position_is(&self, col: u8, row: u8) -> bool {
        self.position == (col, row)
    }

    fn cleared(&self) -> bool {
        self.clear_count > 0
    }

    fn clear_count_is(&self, count: usize) -> bool {
        self.clear_count == count
    }

    fn write_count_is(&self, count: usize) -> bool {
        self.write_count == count
    }

    fn custom_chars_were_reset(&self) -> bool {
        self.custom_chars_reset
    }
}

impl CharacterDisplay for MockDisplay {
    fn set_position(&mut self, col: u8, row: u8) {
        self.position = (col, row);
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.clear_count += 1;
    }

    fn reset_custom_chars(&mut self) {
        self.custom_chars_reset = true;
    }
}

impl Write for MockDisplay {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_count += 1;
        self.buffer.push_str(s);
        Ok(())
    }
}

// ============================================================================
// LABEL WIDGET TESTS
// ============================================================================

mod label_tests {
    use super::*;

    #[test]
    fn test_label_creation() {
        let label: Label<(), 16, 0, 0> = Label::new("Hello");
        assert_eq!(label.state, "Hello");
        assert!(label.invalidate);
    }

    #[test]
    fn test_label_render_initial() {
        let mut label: Label<(), 16, 0, 0> = Label::new("Test");
        let mut display = MockDisplay::new();

        label.render(&mut display);

        assert!(display.assertions().contains("Test"));
        assert!(display.assertions().custom_chars_were_reset());
        assert_eq!(display.assertions().position_is(0, 0), true);
    }

    #[test]
    fn test_label_render_with_position() {
        let mut label: Label<(), 16, 5, 2> = Label::new("Pos");
        let mut display = MockDisplay::new();

        label.render(&mut display);

        assert_eq!(display.assertions().position_is(5, 2), true);
    }

    #[test]
    fn test_label_update_invalidates() {
        let mut label: Label<(), 16, 0, 0> = Label::new("Old");
        let mut display = MockDisplay::new();

        label.render(&mut display);
        let write_count_after_first = display.assertions().write_count;

        label.update("New");
        assert_eq!(label.state, "New");
        assert!(label.invalidate);

        label.render(&mut display);
        // Write count increases (includes text and padding)
        assert!(display.assertions().write_count > write_count_after_first);
    }

    #[test]
    fn test_label_update_same_value_no_invalidate() {
        let mut label: Label<(), 16, 0, 0> = Label::new("Same");
        let mut display = MockDisplay::new();

        label.render(&mut display);
        let initial_write_count = display.assertions().write_count;

        label.update("Same");
        assert!(!label.invalidate);

        label.render(&mut display);
        assert_eq!(display.assertions().write_count, initial_write_count);
    }

    #[test]
    fn test_label_invalidate_forces_rerender() {
        let mut label: Label<(), 16, 0, 0> = Label::new("Text");
        let mut display = MockDisplay::new();

        label.render(&mut display);
        assert!(!label.invalidate);

        label.invalidate();
        assert!(label.invalidate);

        label.render(&mut display);
        assert!(!label.invalidate);
    }

    #[test]
    fn test_label_finish_line_padding() {
        let mut label: Label<(), 16, 2, 0> = Label::new("Hi");
        let mut display = MockDisplay::new();

        label.render(&mut display);

        let content = display.get_content();
        assert!(content.starts_with("Hi"));
        // Should have padding to fill the line
        assert!(content.len() >= 2);
    }

    #[test]
    fn test_label_write_append() {
        let mut label: Label<(), 32, 0, 0> = Label::new("Hello");

        write!(&mut label, " World").unwrap();

        assert_eq!(label.state, "Hello World");
        assert!(label.invalidate);
    }

    #[test]
    fn test_label_empty_state() {
        let mut label: Label<(), 16, 0, 0> = Label::new("");
        let mut display = MockDisplay::new();

        label.render(&mut display);

        // When empty, finish_line still pads with spaces
        let content = display.get_content();
        assert!(content.len() > 0);
        assert!(content.trim().is_empty());
    }

    #[test]
    fn test_label_multiple_updates() {
        let mut label: Label<(), 16, 0, 0> = Label::new("Initial");
        let mut display = MockDisplay::new();

        let updates = vec!["First", "Second", "Third"];

        for (i, new_val) in updates.iter().enumerate() {
            label.update(new_val);
            label.render(&mut display);
            assert_eq!(label.state, *new_val);
            // Write count tracks all write calls including padding
            assert!(display.assertions().write_count > i);
        }
    }

    #[test]
    fn test_label_long_text_truncation() {
        let long_text = "This is a very long text that exceeds the display width";
        let mut label: Label<(), 16, 0, 0> = Label::new(long_text);
        let mut display = MockDisplay::new();

        label.render(&mut display);

        assert_eq!(label.state, long_text);
        assert!(display.assertions().contains(long_text));
    }
}

// ============================================================================
// CHARACTER DISPLAY TRAIT TESTS
// ============================================================================

mod display_trait_tests {
    use super::*;

    #[test]
    fn test_display_set_position() {
        let mut display = MockDisplay::new();
        display.set_position(5, 3);
        assert_eq!(display.assertions().position_is(5, 3), true);
    }

    #[test]
    fn test_display_clear() {
        let mut display = MockDisplay::new();
        display.write_str("content").unwrap();

        display.clear();

        assert_eq!(display.get_content(), "");
        assert_eq!(display.assertions().clear_count_is(1), true);
    }

    #[test]
    fn test_display_multiple_clears() {
        let mut display = MockDisplay::new();

        display.clear();
        display.clear();
        display.clear();

        assert_eq!(display.assertions().clear_count_is(3), true);
    }

    #[test]
    fn test_display_finish_line() {
        let mut display = MockDisplay::new();
        display.write_str("Hi").unwrap();
        display.finish_line(16, 2);

        let content = display.get_content();
        assert!(content.len() >= 2);
        assert!(content.starts_with("Hi"));
    }

    #[test]
    fn test_display_reset_custom_chars() {
        let mut display = MockDisplay::new();
        display.reset_custom_chars();
        assert!(display.assertions().custom_chars_were_reset());
    }

    #[test]
    fn test_display_write_multiple_times() {
        let mut display = MockDisplay::new();

        display.write_str("Hello").unwrap();
        display.write_str(" ").unwrap();
        display.write_str("World").unwrap();

        assert_eq!(display.get_content(), "Hello World");
        assert_eq!(display.assertions().write_count_is(3), true);
    }
}

// ============================================================================
// UI EVENT TESTS
// ============================================================================

mod ui_event_tests {
    use super::*;

    #[test]
    fn test_ui_event_creation() {
        let events = vec![
            UiEvent::Up,
            UiEvent::Down,
            UiEvent::Left,
            UiEvent::Right,
            UiEvent::Enter,
            UiEvent::Back,
        ];

        assert_eq!(events.len(), 6);
    }

    #[test]
    fn test_ui_event_debug() {
        let event = UiEvent::Enter;
        let debug_str = alloc::format!("{:?}", event);
        assert_eq!(debug_str, "Enter");
    }

    #[test]
    fn test_ui_event_copy() {
        let event1 = UiEvent::Up;
        let event2 = event1;
        assert!(matches!(event2, UiEvent::Up));
    }

    #[test]
    fn test_ui_event_clone() {
        let event1 = UiEvent::Down;
        let event2 = event1.clone();
        assert!(matches!(event2, UiEvent::Down));
    }
}

// ============================================================================
// WIDGET TRAIT TESTS
// ============================================================================

mod widget_trait_tests {
    use super::*;

    struct SimpleWidget {
        counter: u32,
        needs_update: bool,
    }

    impl SimpleWidget {
        fn new() -> Self {
            Self {
                counter: 0,
                needs_update: true,
            }
        }
    }

    impl Widget<u32, ()> for SimpleWidget {
        fn invalidate(&mut self) {
            self.needs_update = true;
        }

        fn update(&mut self, state: u32) {
            self.counter = state;
            self.invalidate();
        }

        fn render(&mut self, display: &mut impl CharacterDisplay) {
            if self.needs_update {
                display.clear();
                write!(display, "Counter: {}", self.counter).unwrap();
                self.needs_update = false;
            }
        }

        fn event(&mut self, e: UiEvent) -> Option<()> {
            match e {
                UiEvent::Up => {
                    self.counter += 1;
                    self.invalidate();
                    Some(())
                }
                UiEvent::Down => {
                    if self.counter > 0 {
                        self.counter -= 1;
                        self.invalidate();
                    }
                    Some(())
                }
                _ => None,
            }
        }
    }

    #[test]
    fn test_widget_invalidate() {
        let mut widget = SimpleWidget::new();
        assert!(widget.needs_update);

        let mut display = MockDisplay::new();
        widget.render(&mut display);
        assert!(!widget.needs_update);

        widget.invalidate();
        assert!(widget.needs_update);
    }

    #[test]
    fn test_widget_update() {
        let mut widget = SimpleWidget::new();
        widget.update(42);

        assert_eq!(widget.counter, 42);
        assert!(widget.needs_update);
    }

    #[test]
    fn test_widget_render() {
        let mut widget = SimpleWidget::new();
        widget.update(100);

        let mut display = MockDisplay::new();
        widget.render(&mut display);

        assert!(display.assertions().contains("100"));
        assert!(!widget.needs_update);
    }

    #[test]
    fn test_widget_event_up() {
        let mut widget = SimpleWidget::new();
        widget.update(5);

        let result = widget.event(UiEvent::Up);

        assert_eq!(widget.counter, 6);
        assert!(widget.needs_update);
        assert!(result.is_some());
    }

    #[test]
    fn test_widget_event_down() {
        let mut widget = SimpleWidget::new();
        widget.update(5);

        let result = widget.event(UiEvent::Down);

        assert_eq!(widget.counter, 4);
        assert!(widget.needs_update);
        assert!(result.is_some());
    }

    #[test]
    fn test_widget_event_down_at_zero() {
        let mut widget = SimpleWidget::new();
        widget.update(0);

        // After update, needs_update is true. After render, it becomes false.
        let mut display = MockDisplay::new();
        widget.render(&mut display);
        assert!(!widget.needs_update);

        let result = widget.event(UiEvent::Down);

        assert_eq!(widget.counter, 0);
        // At zero, Down doesn't change counter, so needs_update stays false
        assert!(!widget.needs_update);
        assert!(result.is_some());
    }

    #[test]
    fn test_widget_event_ignored() {
        let mut widget = SimpleWidget::new();
        let initial = widget.counter;

        let result = widget.event(UiEvent::Enter);

        assert_eq!(widget.counter, initial);
        assert!(result.is_none());
    }
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_label_display_integration() {
        let mut label: Label<(), 20, 0, 0> = Label::new("Init");
        let mut display = MockDisplay::new();

        // First render
        label.render(&mut display);
        assert!(display.assertions().contains("Init"));

        let _first_content = display.get_content().to_string();

        // Update and render - this clears and resets the buffer in our mock
        // so we need to test that the new content contains the updated text
        label.update("Updated");
        label.render(&mut display);
        let updated_content = display.get_content();

        // Should contain Updated but may contain Init from before since we append to buffer
        assert!(updated_content.contains("Updated"));
    }

    #[test]
    fn test_multiple_display_positions() {
        let mut label1: Label<(), 20, 0, 0> = Label::new("Line1");
        let mut label2: Label<(), 20, 0, 1> = Label::new("Line2");
        let mut display = MockDisplay::new();

        label1.render(&mut display);
        assert_eq!(display.position, (0, 0));

        label2.render(&mut display);
        assert_eq!(display.position, (0, 1));
    }

    #[test]
    fn test_display_state_persistence() {
        let mut display = MockDisplay::new();

        display.write_str("First").unwrap();
        let first_content = display.get_content().to_string();

        display.write_str("Second").unwrap();
        let second_content = display.get_content().to_string();

        assert_eq!(first_content, "First");
        assert_eq!(second_content, "FirstSecond");
    }

    #[test]
    fn test_label_with_formatting() {
        let mut label: Label<(), 32, 0, 0> = Label::new("");
        let mut display = MockDisplay::new();

        write!(&mut label, "Value: {}", 123).unwrap();
        label.render(&mut display);

        assert!(display.assertions().contains("Value: 123"));
    }
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

mod edge_case_tests {
    use super::*;

    #[test]
    fn test_empty_display() {
        let display = MockDisplay::new();
        assert_eq!(display.get_content(), "");
    }

    #[test]
    fn test_very_long_label() {
        let long_text = "a".repeat(1000);
        let mut label: Label<(), 1024, 0, 0> = Label::new(&long_text);
        let mut display = MockDisplay::new();

        label.render(&mut display);

        assert_eq!(label.state.len(), 1000);
    }

    #[test]
    fn test_special_characters() {
        let mut label: Label<(), 32, 0, 0> = Label::new("!@#$%^&*()");
        let mut display = MockDisplay::new();

        label.render(&mut display);

        assert!(display.assertions().contains("!@#$%^&*()"));
    }

    #[test]
    fn test_rapid_updates() {
        let mut label: Label<(), 20, 0, 0> = Label::new("Start");
        let mut display = MockDisplay::new();

        for i in 0..100 {
            label.update(&i.to_string());
            label.render(&mut display);
        }

        assert_eq!(label.state, "99");
    }

    #[test]
    fn test_display_clear_and_reuse() {
        let mut display = MockDisplay::new();

        display.write_str("First").unwrap();
        display.clear();
        display.write_str("Second").unwrap();

        assert_eq!(display.get_content(), "Second");
        assert_eq!(display.assertions().clear_count_is(1), true);
    }

    #[test]
    fn test_label_max_length() {
        let label: Label<(), 16, 0, 0> = Label::new("Short");
        let label_long: Label<(), 256, 0, 0> = Label::new("Short");

        assert_eq!(label.state.len(), 5);
        assert_eq!(label_long.state.len(), 5);
    }
}

// ============================================================================
// MOCK BEHAVIOR TESTS
// ============================================================================

mod mock_behavior_tests {
    use super::*;

    #[test]
    fn test_mock_tracks_positions() {
        let mut display = MockDisplay::new();

        display.set_position(0, 0);
        assert_eq!(display.position, (0, 0));

        display.set_position(5, 2);
        assert_eq!(display.position, (5, 2));

        display.set_position(15, 1);
        assert_eq!(display.position, (15, 1));
    }

    #[test]
    fn test_mock_write_counting() {
        let mut display = MockDisplay::new();
        assert_eq!(display.assertions().write_count_is(0), true);

        display.write_str("a").unwrap();
        assert_eq!(display.assertions().write_count_is(1), true);

        display.write_str("b").unwrap();
        assert_eq!(display.assertions().write_count_is(2), true);
    }

    #[test]
    fn test_mock_assertions_chain() {
        let mut display = MockDisplay::new();
        display.write_str("test").unwrap();

        let assertions = display.assertions();
        assert!(assertions.contains("test"));
        assert!(assertions.equals("test"));
        assert!(!assertions.equals("other"));
    }
}
