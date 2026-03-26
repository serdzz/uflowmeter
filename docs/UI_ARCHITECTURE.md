# UI Architecture

## Overview

The UI system is built on a unidirectional data flow pattern with separation of state and presentation. The architecture is optimized for embedded systems (`no_std`) and character displays (16x2 LCD).

## Core Components

### Widget Trait

The primary interface for all UI elements:

```rust
pub trait Widget<S, A> {
    fn invalidate(&mut self);              // Mark for re-render
    fn update(&mut self, state: S);        // Sync from application state
    fn render(&mut self, display: ...);    // Draw to display
    fn event(&mut self, e: UiEvent) -> Option<A>; // Handle input event
}
```

**Type parameters:**
- `S` — application state type (typically `&App`)
- `A` — action type (typically `Actions`)

### CharacterDisplay Trait

Abstraction for character displays:

```rust
pub trait CharacterDisplay: core::fmt::Write {
    fn set_position(&mut self, col: u8, row: u8);
    fn clear(&mut self);
    fn reset_custom_chars(&mut self);
    fn finish_line(&mut self, width: usize, len: usize);
}
```

### Primitive Widgets

Located in `src/gui/`:

- **`Label`** — static text with formatting support
- **`Edit`** — editable text field with blinking support
- **`EditBox`** — text field with cursor

### Events

```rust
pub enum UiEvent {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
}
```

## Widget Composition Macros

### widget_group!

Creates a widget container where **all child widgets are rendered simultaneously**:

```rust
widget_group!(
    MyView<&App, Actions>,
    {
        label: Label<Actions, 16, 0, 0>;
        edit: Edit<Actions, 16, 8, 1>;
    },
    |view, state: &App| {
        // Update function
        view.label.update(state);
        view.edit.update(state);
    },
    |view, event: UiEvent| -> Option<Actions> {
        // Event handling
        view.edit.event(event)
    }
)
```

**Usage:**
- Group related widgets
- Composite screens with multiple elements
- All widgets are rendered on every `render()` call

### widget_mux!

Creates a widget multiplexer where **only the active widget is rendered**:

```rust
widget_mux!(
    Viewport<&App, Actions>,
    ViewportNode::Label,  // default active widget
    {
        label: LabelWidget;
        datetime: DateTimeWidget;
        hour_history: HistoryWidget<HourKind>;
        day_history: HistoryWidget<DayKind>;
        month_history: HistoryWidget<MonthKind>;
    },
    |viewport, state: &App| {
        viewport.label.update(state);
        viewport.datetime.update(state.datetime);
        viewport.hour_history.update(state);
        viewport.day_history.update(state);
        viewport.month_history.update(state);
    },
    |viewport, event: UiEvent| -> Option<Actions> {
        match viewport.active {
            ViewportNode::Label => viewport.label.event(event),
            ViewportNode::Datetime => viewport.datetime.event(event),
            ViewportNode::HourHistory => viewport.hour_history.event(event),
            ViewportNode::DayHistory => viewport.day_history.event(event),
            ViewportNode::MonthHistory => viewport.month_history.event(event),
        }
    }
)
```

**Auto-generated:**
- Enum `ViewportNode` with a variant per widget
- Method `set_active(node: ViewportNode)` for switching
- Rendering logic for only the active widget

**Usage:**
- Navigation between screens
- Modes (view / edit)
- Resource efficiency (only the visible screen is rendered)

## Application Architecture

### App State

Central application state (`src/apps.rs`):

```rust
pub struct App {
    pub datetime: PrimitiveDateTime,
    pub active_widget: ViewportNode,     // currently active screen
    pub flow: f32,                       // current flow rate
    pub hour_flow: f32,                  // hourly accumulator
    pub day_flow: f32,                   // daily accumulator
    pub month_flow: f32,                 // monthly accumulator
    pub history_state: HistoryState,     // history query state
    // ...
}

impl App {
    pub fn handle_event(&mut self, action: Option<Actions>)
        -> Option<AppRequest> {
        // Convert actions into system requests
    }
}
```

### Actions

Typed user actions:

```rust
pub enum Actions {
    Label,
    DateTime,
    SetDateTime(PrimitiveDateTime),
    ActionA,
    ActionB,
    SetHistory(HistoryType, u32),
    HourHistory,
    DayHistory,
    MonthHistory,
}
```

### AppRequest

Requests to system components:

```rust
pub enum AppRequest {
    Process,                              // Trigger measurement
    LcdLed(bool),                        // Control backlight
    SetDateTime(PrimitiveDateTime),      // Set RTC time
    SetHistory(HistoryType, u32),        // Query history
    DeepSleep,                           // Enter low-power sleep
}
```

## Data Flow

Unidirectional processing pipeline:

```
┌─────────────┐
│   Events    │  Button presses (Up/Down/Left/Right/Enter/Back)
│  (Buttons)  │
└──────┤──────┘
       │
       v
┌─────────────────┐
│ Widget::event() │  Processed by the active widget
└──────┤──────────┘
       │
       v
┌─────────────┐
│   Actions   │  Typed action value
└──────┤──────┘
       │
       v
┌──────────────────────┐
│ App::handle_event()  │  Application business logic
└──────┤───────────────┘
       │
       v
┌─────────────┐
│ AppRequest  │  Request to system (RTC, EEPROM, measurement)
└──────┤──────┘
       │
       v
┌──────────────┐
│   System    │  Execute operations (RTC, EEPROM, TDC7200)
└──────┤───────┘
       │
       v
┌──────────────┐
│  App State   │  State updated
└──────┤───────┘
       │
       v
┌──────────────────┐
│ Widget::update() │  Sync with state
└──────┤───────────┘
       │
       v
┌──────────────────┐
│ Widget::render() │  Draw to display
└──────┤───────────┘
       │
       v
┌─────────────┐
│   Display   │  16x2 LCD
└─────────────┘
```

## Example: History Widget

Generic widget for browsing history (`src/gui/history_widget.rs`).

The type parameter `K: HistoryKind` determines the history type and navigation:

```rust
/// Marker trait — defines the history type and screen navigation
pub trait HistoryKind {
    fn history_type() -> HistoryType;
    fn nav_left() -> Actions;
    fn nav_right() -> Actions;
}

pub struct HourKind;   // Hour: Left=Label, Right=DayHistory
pub struct DayKind;    // Day:  Left=HourHistory, Right=MonthHistory
pub struct MonthKind;  // Month: Left=DayHistory, Right=Label

/// Single widget for all three history screens
pub struct HistoryWidget<K: HistoryKind> {
    pub date: Edit<Actions, 16, 8, 0>,
    pub time: Edit<Actions, 16, 8, 1>,
    pub label: Label<Actions, 16, 0, 0>,
    pub value: Label<Actions, 8, 0, 1>,
    pub items: DateTimeItems,
    pub editable: bool,
    // private: datetime, timestamp, first_render, _kind
}
```

Usage in `widget_mux!`:

```rust
hour_history: HistoryWidget<HourKind>;
day_history:  HistoryWidget<DayKind>;
month_history: HistoryWidget<MonthKind>;
```

Public methods: `get_datetime`, `get_timestamp`, `set_timestamp`, `get_items`, `set_items`, `set_datetime`, `get_editable`, `set_editable`, `get_history_type`, `inc`, `dec`, `next_item`.

## Key Principles

### 1. State Separation

State lives in `App`; widgets only render and emit actions:

```rust
// ✅ Correct — state in App
pub struct App {
    pub datetime: PrimitiveDateTime,
}

pub struct DateTimeWidget {
    edit: Edit<...>,  // UI elements only
}

// ❌ Wrong — duplicated state
pub struct DateTimeWidget {
    datetime: PrimitiveDateTime,  // unnecessary!
}
```

### 2. Action-based Communication

Events are converted into typed actions:

```rust
// ✅ Correct — typed actions
fn event(&mut self, event: UiEvent) -> Option<Actions> {
    match event {
        UiEvent::Enter => Some(Actions::SetDateTime(self.datetime)),
        _ => None,
    }
}

// ❌ Wrong — direct state mutation
fn event(&mut self, event: UiEvent, app: &mut App) {
    app.datetime = self.datetime;  // breaks the architecture!
}
```

### 3. Composability

Complex UI is built from simple widgets:

```rust
// Primitives
Edit + Label + EditBox

// Behavioral widgets
DateTimeWidget (src/gui/date_time_widget.rs)
HistoryWidget<K> (src/gui/history_widget.rs)

// Screens via widget_group!
LabelScreen = Label + Label
LabelsWidget = Label + Edit

// Navigation via widget_mux!
Viewport = Label | DateTime | HourHistory | DayHistory | MonthHistory
```

### 4. No Allocation

Zero dynamic memory allocation:

```rust
// Fixed-size buffers
pub struct Edit<A, const N: usize, const COL: u8, const ROW: u8> {
    state: String<N>,  // heapless::String
}

// Write via format macros
write!(self.edit, "{:02}:{:02}", hour, minute).ok();
```

### 5. Character Display Optimization

Optimized for 16x2 character LCD displays:

```rust
// Positioning
Edit<Actions, 16, 8, 0>  // 16 chars, column 8, row 0

// Blink mask (for editing)
edit.blink_mask(0x03);    // Last 2 chars blink (bits 0-1)
edit.blink_mask(0xc0);    // First 2 chars blink (bits 6-7)
```

## Extending the System

### Adding a New Widget

1. **Define the widget struct:**

```rust
pub struct MyWidget {
    label: Label<Actions, 16, 0, 0>,
    value: Edit<Actions, 16, 8, 1>,
    count: u32,
}
```

2. **Implement the Widget trait:**

```rust
impl Widget<&App, Actions> for MyWidget {
    fn invalidate(&mut self) {}

    fn update(&mut self, state: &App) {
        self.value.state.clear();
        write!(self.value, "{}", state.some_value).ok();
    }

    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        match event {
            UiEvent::Enter => Some(Actions::MyAction),
            _ => None,
        }
    }

    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.label.render(display);
        self.value.render(display);
    }
}
```

3. **Add to Viewport:**

```rust
widget_mux!(
    Viewport<&App, Actions>,
    ViewportNode::Label,
    {
        label: LabelWidget;
        my_widget: MyWidget;  // <-- add here
    },
    update_fn,
    event_fn
)
```

### Adding a New Action

1. **Extend the Actions enum:**

```rust
pub enum Actions {
    // ...
    MyNewAction(u32),
}
```

2. **Handle in App::handle_event:**

```rust
impl App {
    pub fn handle_event(&mut self, action: Option<Actions>)
        -> Option<AppRequest> {
        match action {
            Some(Actions::MyNewAction(value)) => {
                self.process_value(value);
                Some(AppRequest::Process)
            }
            // ...
        }
    }
}
```

## Testing

UI logic is tested using mock displays:

```rust
#[cfg(test)]
mod tests {
    struct MockDisplay {
        buffer: String,
    }
    
    impl CharacterDisplay for MockDisplay {
        fn set_position(&mut self, col: u8, row: u8) { /* ... */ }
        fn clear(&mut self) { self.buffer.clear(); }
        // ...
    }
    
    #[test]
    fn test_widget_update() {
        let mut widget = MyWidget::new();
        let app = App::new();
        let mut display = MockDisplay::new();
        
        widget.update(&app);
        widget.render(&mut display);
        
        assert!(display.buffer.contains("expected"));
    }
}
```

## Performance

- **Update**: O(n) where n = number of widgets in group/mux
- **Render**: O(1) for mux (active only), O(n) for group
- **Event**: O(1) — direct routing to the active widget
- **Memory**: zero allocation, all buffers are static

## File Structure

```
src/
├── gui/                          # GUI components
│   ├── mod.rs                   # Widget, UiEvent, CharacterDisplay; re-exports DateTimeItems, HistoryType
│   ├── label.rs                 # Label widget
│   ├── edit.rs                  # Edit widget
│   ├── editbox.rs               # EditBox widget
│   ├── macros.rs                # widget_group!, widget_mux!
│   ├── date_time_widget.rs      # DateTimeWidget + DateTimeItems
│   └── history_widget.rs        # HistoryWidget<K>, HistoryKind, HourKind/DayKind/MonthKind, HistoryType
├── apps.rs                      # App state, Actions, AppRequest
├── ui.rs                        # Viewport (widget_mux), LabelScreen, LabelsWidget
└── main.rs                      # RTIC integration
```

### Type Locations

| Type | File |
|---|---|
| `DateTimeItems` | `gui/date_time_widget.rs` (re-exported as `gui::DateTimeItems`) |
| `HistoryType` | `gui/history_widget.rs` (re-exported as `gui::HistoryType`) |
| `DateTimeWidget` | `gui/date_time_widget.rs` |
| `HistoryWidget<K>` | `gui/history_widget.rs` |
| `HistoryKind`, `HourKind`, `DayKind`, `MonthKind` | `gui/history_widget.rs` |
| `Actions`, `App`, `AppRequest` | `apps.rs` |
