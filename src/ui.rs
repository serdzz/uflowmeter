//! Menu system matching C++ UsFlowMeter architecture.
//!
//! C++ design:
//!   - Menu holds 4 `UI::List` objects (main, user, calibration, configuration)
//!   - `UI::List` = `RingList<Widget*>` with Up/Down navigation
//!   - Widget::key_event returns bool — if false, List handles navigation
//!   - Special keys (Exit, Config, Manufacture) switch between menus
//!
//! Rust port:
//!   - `Screen` enum for all screen types (no heap allocation, no dyn trait)
//!   - `MenuList` = ring buffer of screens with Up/Down navigation
//!   - `MenuController` = 4 MenuLists + current_menu pointer + key dispatch

use crate::apps::AppRequest;
use crate::gui::{CharacterDisplay, HistoryType, UiEvent};
use crate::App;
use alloc::string::String;
use core::fmt::Write;
use time::{Duration, PrimitiveDateTime};

// ─── Screen enum ─────────────────────────────────────────────────────
/// All possible screen types — one enum variant per C++ screen.
/// Each variant carries the minimal state needed for that screen.
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    // Main menu (14 items matching C++)
    HourConsumption,
    DayConsumption,
    TotalVolume,
    Uptime,
    HourHistory,
    DayHistory,
    MonthHistory,
    DateTime,
    Version,
    Bootloader,
    CommType,
    SlaveAddress,
    Muster,
    Negative,
    // User menu (2 items)
    Channel1,
    Channel2,
    // Configuration menu (2 items)
    SensorType,
    SerialNumber,
    // Calibration menu (1 item)
    Calibration,
}

/// Which menu is currently active
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    None,
    Main,
    User,
    Calibration,
    Configuration,
}

/// Active edit field on a history screen. `None` = not in edit mode.
/// Enter steps forward through the sequence (per screen type) and exits.
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryEditField {
    None,
    Hour,
    Day,
    Month,
    Year,
}

// ─── Comm type strings ───────────────────────────────────────────────
const COMM_TYPES: [&str; 4] = ["ВЫКЛ", "M-BUS", "ModBus", "Выход 4-20mA"];
const SENSOR_TYPES: [&str; 5] = ["ДУ40", "ДУ50", "ДУ65", "ДУ80", "ДУ100"];
const ON_OFF: [&str; 2] = ["ВЫКЛ", "ВКЛ"];

// ─── MenuList ────────────────────────────────────────────────────────
/// Ring buffer of screen IDs. Up/Down navigates.
/// If current screen doesn't consume the key, List does navigation.
/// Ported from C++ UI::List + RingList.
pub struct MenuList {
    items: [ScreenId; 16],
    count: usize,
    index: usize,
}

impl Default for MenuList {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuList {
    pub fn new() -> Self {
        Self {
            items: [ScreenId::HourConsumption; 16],
            count: 0,
            index: 0,
        }
    }

    pub fn add(&mut self, screen: ScreenId) {
        if self.count < 16 {
            self.items[self.count] = screen;
            self.count += 1;
        }
    }

    pub fn current(&self) -> ScreenId {
        self.items[self.index % self.count]
    }

    pub fn index(&self) -> usize {
        self.index % self.count
    }

    /// Move to next enabled screen (wraps around)
    fn next_enabled(&mut self, is_enabled: impl Fn(ScreenId) -> bool) {
        if self.count == 0 {
            return;
        }
        let start = self.index;
        loop {
            self.index = (self.index + 1) % self.count;
            if is_enabled(self.current()) || self.index == start {
                break;
            }
        }
    }

    /// Move to previous enabled screen (wraps around)
    fn prev_enabled(&mut self, is_enabled: impl Fn(ScreenId) -> bool) {
        if self.count == 0 {
            return;
        }
        let start = self.index;
        loop {
            if self.index == 0 {
                self.index = self.count - 1;
            } else {
                self.index -= 1;
            }
            if is_enabled(self.current()) || self.index == start {
                break;
            }
        }
    }

    /// Reset to first item (C++ does this on hide/deselect)
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

// ─── Editable state for screens that need it ─────────────────────────
/// State for EditBox-like screens (comm type, muster, negative, sensor, day start)
#[derive(Default, Debug, Clone, Copy)]
pub struct EditBoxState {
    pub cursor: u8,
    pub editable: bool,
}

/// State for EditNumber-like screens (slave address)
#[derive(Debug, Clone, Copy)]
pub struct EditNumberState {
    pub value: u8,
    pub min: u8,
    pub max: u8,
    pub editable: bool,
}

impl Default for EditNumberState {
    fn default() -> Self {
        Self {
            value: 0,
            min: 0,
            max: 255,
            editable: false,
        }
    }
}

/// State for DateTime editing
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeEditItem {
    #[default]
    None,
    Seconds,
    Minutes,
    Hours,
    Day,
    Month,
    Year,
}

/// Pattern for ClickableLabel (version screen secret pattern)
#[derive(Debug, Clone, Copy, Default)]
pub struct PatternState {
    /// How many keys of the pattern have been matched
    pub matched: u8,
}

// ─── MenuController ──────────────────────────────────────────────────
/// Central menu controller — matches C++ Menu class.
/// Holds 4 menu lists and dispatches key events.
pub struct MenuController {
    pub current_menu: MenuId,
    main_menu: MenuList,
    user_menu: MenuList,
    calibration_menu: MenuList,
    configuration_menu: MenuList,
    // Editable states
    pub comm_type: EditBoxState,
    pub muster: EditBoxState,
    pub negative: EditBoxState,
    pub sensor_type: EditBoxState,
    pub slave_address: EditNumberState,
    pub datetime_item: DateTimeEditItem,
    pub pattern: PatternState,
    /// Idle counter for auto-hide (C++ IDLE_TIMEOUT)
    pub idle_counter: u8,
    /// Working copy of datetime being edited
    pub edited_datetime: PrimitiveDateTime,
    /// Datetime cursor for browsing history (hour/day/month screens)
    pub history_datetime: PrimitiveDateTime,
    /// Active edit field on the current history screen (None = not editing)
    pub history_edit: HistoryEditField,
    /// Frame counter for blink animation while editing history fields.
    /// Wraps at HISTORY_BLINK_PERIOD; visible when counter < HISTORY_BLINK_PERIOD/2.
    pub history_blink: u8,
    /// Frame counter for blink animation while editing the DateTime screen.
    pub datetime_blink: u8,
}

/// Frames-per-blink-cycle (render runs at 10 Hz → 6 frames ≈ 600 ms cycle,
/// 300 ms visible / 300 ms hidden ≈ 1.66 Hz blink).
const HISTORY_BLINK_PERIOD: u8 = 6;

impl Default for MenuController {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuController {
    pub fn new() -> Self {
        // Build main menu — matches C++ init_main()
        let mut main_menu = MenuList::new();
        main_menu.add(ScreenId::HourConsumption);
        main_menu.add(ScreenId::DayConsumption);
        main_menu.add(ScreenId::TotalVolume);
        main_menu.add(ScreenId::Uptime);
        main_menu.add(ScreenId::HourHistory);
        main_menu.add(ScreenId::DayHistory);
        main_menu.add(ScreenId::MonthHistory);
        main_menu.add(ScreenId::DateTime);
        main_menu.add(ScreenId::Version);
        main_menu.add(ScreenId::Bootloader);
        main_menu.add(ScreenId::CommType);
        main_menu.add(ScreenId::SlaveAddress);
        main_menu.add(ScreenId::Muster);
        main_menu.add(ScreenId::Negative);

        // Build user menu — matches C++ init_user()
        let mut user_menu = MenuList::new();
        user_menu.add(ScreenId::Channel1);
        user_menu.add(ScreenId::Channel2);

        // Build calibration menu — matches C++ init_calibration()
        let mut calibration_menu = MenuList::new();
        calibration_menu.add(ScreenId::Calibration);

        // Build configuration menu — matches C++ init_configuration()
        let mut configuration_menu = MenuList::new();
        configuration_menu.add(ScreenId::SensorType);
        configuration_menu.add(ScreenId::SerialNumber);

        Self {
            current_menu: MenuId::None,
            main_menu,
            user_menu,
            calibration_menu,
            configuration_menu,
            comm_type: EditBoxState::default(),
            muster: EditBoxState::default(),
            negative: EditBoxState::default(),
            sensor_type: EditBoxState::default(),
            slave_address: EditNumberState {
                value: 1,
                min: 1,
                max: 250,
                editable: false,
            },
            datetime_item: DateTimeEditItem::default(),
            pattern: PatternState::default(),
            idle_counter: 0,
            edited_datetime: time::macros::datetime!(2023-01-01 00:00:00),
            history_datetime: time::macros::datetime!(2024-01-01 00:00:00),
            history_edit: HistoryEditField::None,
            history_blink: 0,
            datetime_blink: 0,
        }
    }

    /// Check if a screen is enabled (for List navigation skipping).
    /// C++ disables slave_address when comm_type is None.
    #[allow(dead_code)]
    fn is_enabled(&self, screen: ScreenId) -> bool {
        match screen {
            ScreenId::SlaveAddress => {
                self.comm_type.cursor != 0 // not ВЫКЛ
            }
            _ => true,
        }
    }

    /// Get the current active menu list
    fn current_list(&self) -> &MenuList {
        match self.current_menu {
            MenuId::Main => &self.main_menu,
            MenuId::User => &self.user_menu,
            MenuId::Calibration => &self.calibration_menu,
            MenuId::Configuration => &self.configuration_menu,
            MenuId::None => &self.main_menu, // fallback
        }
    }

    fn current_list_mut(&mut self) -> &mut MenuList {
        match self.current_menu {
            MenuId::Main => &mut self.main_menu,
            MenuId::User => &mut self.user_menu,
            MenuId::Calibration => &mut self.calibration_menu,
            MenuId::Configuration => &mut self.configuration_menu,
            MenuId::None => &mut self.main_menu,
        }
    }

    /// Select a menu (C++ Menu::select)
    pub fn select(&mut self, menu: MenuId) -> Option<AppRequest> {
        if self.current_menu == menu {
            return None;
        }
        self.current_menu = menu;
        // On new menu selection, reset to first item
        // C++ shows current()->show() but doesn't reset index
        None
    }

    /// Deselect — exit menu (C++ Menu::deselect)
    pub fn deselect(&mut self) -> Option<AppRequest> {
        self.current_menu = MenuId::None;
        self.main_menu.reset();
        self.user_menu.reset();
        self.configuration_menu.reset();
        Some(AppRequest::DeepSleep)
    }

    /// Get current screen ID
    pub fn current_screen(&self) -> ScreenId {
        self.current_list().current()
    }

    // ─── Title line ──────────────────────────────────────────────────
    pub fn title(&self, screen: ScreenId) -> &'static str {
        match screen {
            ScreenId::HourConsumption => "Расход     Qм3/ч",
            ScreenId::DayConsumption => "Расход   Qм3/сут",
            ScreenId::TotalVolume => "Объем      Vм3  ",
            ScreenId::Uptime => "Время работы",
            ScreenId::HourHistory => "Расход за",
            ScreenId::DayHistory => "Расход за",
            ScreenId::MonthHistory => "Расход за",
            ScreenId::DateTime => "Дата/Время",
            ScreenId::Version => "Версия ПО",
            ScreenId::Bootloader => "Обновить ПО",
            ScreenId::CommType => "Тип связи",
            ScreenId::SlaveAddress => "Адрес",
            ScreenId::Muster => "Поверка",
            ScreenId::Negative => "Реверс",
            ScreenId::Channel1 => "01         луч 1",
            ScreenId::Channel2 => "02         луч 2",
            ScreenId::SensorType => "Датчик",
            ScreenId::SerialNumber => "Номер прибора",
            ScreenId::Calibration => "Calibration MODE",
        }
    }

    // ─── Value line ──────────────────────────────────────────────────
    pub fn format_value(&self, screen: ScreenId, app: &App) -> String {
        let mut s = String::new();
        match screen {
            ScreenId::HourConsumption => {
                write!(s, "{:.3}", app.flow).ok();
            }
            ScreenId::DayConsumption => {
                write!(s, "{:.3}", app.day_flow).ok();
            }
            ScreenId::TotalVolume => {
                write!(s, "{:.3}", app.month_flow).ok();
            }
            ScreenId::Uptime => {
                write!(s, "{:.0}m", app.num).ok();
            }
            ScreenId::HourHistory | ScreenId::DayHistory | ScreenId::MonthHistory => {
                // History screens show date/time + value — handled in render
                if let Some(flow) = app.history_state.flow {
                    write!(s, "{:.3}", flow).ok();
                } else {
                    s.push_str("None");
                }
            }
            ScreenId::DateTime => {
                // DateTime screen has its own rendering
                let dt = &app.datetime;
                write!(s, "{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second()).ok();
            }
            ScreenId::Version => {
                write!(s, "0.1.{}", app.num % 1000).ok();
            }
            ScreenId::Bootloader => {
                // Button — no value line
            }
            ScreenId::CommType => {
                let idx = self.comm_type.cursor as usize;
                if idx < COMM_TYPES.len() {
                    s.push_str(COMM_TYPES[idx]);
                }
            }
            ScreenId::SlaveAddress => {
                write!(s, "{}", self.slave_address.value).ok();
            }
            ScreenId::Muster => {
                let idx = self.muster.cursor as usize;
                if idx < ON_OFF.len() {
                    s.push_str(ON_OFF[idx]);
                }
            }
            ScreenId::Negative => {
                let idx = self.negative.cursor as usize;
                if idx < ON_OFF.len() {
                    s.push_str(ON_OFF[idx]);
                }
            }
            ScreenId::Channel1 | ScreenId::Channel2 => {
                // Channel status — "работает" / "отсутствует"
                s.push_str("отсутствует");
            }
            ScreenId::SensorType => {
                let idx = self.sensor_type.cursor as usize;
                if idx < SENSOR_TYPES.len() {
                    s.push_str(SENSOR_TYPES[idx]);
                }
            }
            ScreenId::SerialNumber => {
                write!(s, "{}", app.num).ok();
            }
            ScreenId::Calibration => {
                // Just a label
            }
        }
        s
    }

    // ─── Key event handling ──────────────────────────────────────────
    /// Handle a key event. Returns AppRequest if something needs to happen.
    /// Matches C++ Menu::process_key + UI::List::key_event pattern.
    pub fn key_event(&mut self, event: UiEvent, app: &App) -> Option<AppRequest> {
        self.idle_counter = 20; // reset idle counter on any key

        // Global keys first (matching C++ process_key)
        match event {
            UiEvent::Back => {
                // Exit key → deselect (C++ Exit)
                return self.deselect();
            }
            UiEvent::Enter if self.current_screen() == ScreenId::Uptime => {
                // Long Enter on Uptime → user menu (simplified: just Enter)
                // C++ uses long-press timer, we use plain Enter for now
                return self.select(MenuId::User);
            }
            _ => {}
        }

        // If no menu is active, first key press activates main menu
        if self.current_menu == MenuId::None {
            return self.select(MenuId::Main);
        }

        // Dispatch to current screen first
        let screen = self.current_screen();
        let consumed = self.screen_key_event(screen, event, app);

        if consumed.is_some() {
            return consumed;
        }

        // Screen didn't consume — List handles navigation (C++ UI::List::key_event)
        // Cache enabled state to avoid borrow conflict.
        // Hardware Up/Down emit UiEvent::Right/Left (see hardware/keyboard.rs::to_ui_event),
        // so the list must accept both forms.
        let comm_cursor = self.comm_type.cursor;
        match event {
            UiEvent::Up | UiEvent::Right => {
                self.current_list_mut().next_enabled(|s: ScreenId| match s {
                    ScreenId::SlaveAddress => comm_cursor != 0,
                    _ => true,
                });
                None
            }
            UiEvent::Down | UiEvent::Left => {
                self.current_list_mut().prev_enabled(|s: ScreenId| match s {
                    ScreenId::SlaveAddress => comm_cursor != 0,
                    _ => true,
                });
                None
            }
            _ => None,
        }
    }

    /// Screen-specific key handling. Returns Some if consumed, None if not.
    /// Matches C++ Widget::key_event pattern — each widget handles its own keys.
    fn screen_key_event(
        &mut self,
        screen: ScreenId,
        event: UiEvent,
        _app: &App,
    ) -> Option<AppRequest> {
        match screen {
            // ── LiveMeter screens: no keys consumed (navigation handled by List) ──
            ScreenId::HourConsumption
            | ScreenId::DayConsumption
            | ScreenId::TotalVolume
            | ScreenId::Channel1
            | ScreenId::Channel2
            | ScreenId::SerialNumber => None,

            // ── Uptime: long Enter → user menu (handled above) ──
            ScreenId::Uptime => None,

            // ── DateTime: Enter cycles YY→MM→dd→hh→mm→ss→exit ──
            ScreenId::DateTime => self.datetime_key_event(event, _app),

            // ── Version: secret pattern detection ──
            ScreenId::Version => self.version_key_event(event),

            // ── Bootloader: Enter → system reset ──
            ScreenId::Bootloader => {
                if event == UiEvent::Enter {
                    Some(AppRequest::SystemReset)
                } else {
                    None
                }
            }

            // ── EditBox screens: Left/Right cycle items, Enter toggles edit ──
            ScreenId::CommType => {
                MenuController::editbox_key_event(&mut self.comm_type, 4, event, |idx| {
                    AppRequest::SetCommType(idx)
                })
            }
            ScreenId::Muster => {
                MenuController::editbox_key_event(&mut self.muster, 2, event, |idx| {
                    AppRequest::SetMuster(idx > 0)
                })
            }
            ScreenId::Negative => {
                MenuController::editbox_key_event(&mut self.negative, 2, event, |idx| {
                    AppRequest::SetNegative(idx > 0)
                })
            }
            ScreenId::SensorType => {
                MenuController::editbox_key_event(&mut self.sensor_type, 5, event, |idx| {
                    AppRequest::SetCommType(idx) // reuse SetCommType to signal sensor change
                })
            }

            // ── EditNumber screens: Left/Right change value, Enter toggles edit ──
            ScreenId::SlaveAddress => {
                MenuController::editnumber_key_event(&mut self.slave_address, event, |v| {
                    AppRequest::SetAddress(v)
                })
            }

            // ── History screens: Enter starts date navigation ──
            ScreenId::HourHistory => self.history_key_event(event, HistoryType::Hour),
            ScreenId::DayHistory => self.history_key_event(event, HistoryType::Day),
            ScreenId::MonthHistory => self.history_key_event(event, HistoryType::Month),

            // ── Calibration: just a label ──
            ScreenId::Calibration => None,
        }
    }

    // ─── EditBox key handler (shared for CommType, Muster, Negative, SensorType) ──
    fn editbox_key_event(
        state: &mut EditBoxState,
        max_items: u8,
        event: UiEvent,
        on_change: fn(u8) -> AppRequest,
    ) -> Option<AppRequest> {
        match event {
            UiEvent::Left => {
                if state.editable {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    } else {
                        state.cursor = max_items - 1;
                    }
                    Some(on_change(state.cursor))
                } else {
                    None // let List handle
                }
            }
            UiEvent::Right => {
                if state.editable {
                    state.cursor = (state.cursor + 1) % max_items;
                    Some(on_change(state.cursor))
                } else {
                    None
                }
            }
            UiEvent::Enter => {
                // Enter advances the value directly and commits — applies to
                // all editbox screens (CommType 4-value, SensorType 5-value,
                // Muster/Negative 2-value). No separate edit mode: with only
                // 4 hardware buttons (Set, Enter, Up, Down) and Up/Down used
                // for screen navigation, Enter is the natural value-cycle key.
                state.cursor = (state.cursor + 1) % max_items;
                Some(on_change(state.cursor))
            }
            _ => None,
        }
    }

    // ─── EditNumber key handler (shared for SlaveAddress) ──
    fn editnumber_key_event(
        state: &mut EditNumberState,
        event: UiEvent,
        on_change: fn(u8) -> AppRequest,
    ) -> Option<AppRequest> {
        match event {
            UiEvent::Up => {
                if state.editable {
                    if state.value < state.max {
                        state.value += 1;
                    } else {
                        state.value = state.min;
                    }
                    Some(AppRequest::Process) // consumed
                } else {
                    None // let List handle
                }
            }
            UiEvent::Down => {
                if state.editable {
                    if state.value > state.min {
                        state.value -= 1;
                    } else {
                        state.value = state.max;
                    }
                    Some(AppRequest::Process) // consumed
                } else {
                    None
                }
            }
            UiEvent::Enter => {
                // Enter increments the value and commits — wraps min..=max.
                // No edit mode: with Up/Down already used for screen
                // navigation, Enter is the only way to change the value.
                // The keyboard's repeat (150 ms after 1 s hold) gives fast
                // increment when held.
                if state.value < state.max {
                    state.value += 1;
                } else {
                    state.value = state.min;
                }
                Some(on_change(state.value))
            }
            _ => None,
        }
    }

    // ─── DateTime key handler ──
    /// Edit cycle: None → Year → Month → Day → Hours → Minutes → Seconds → None.
    /// In edit mode, Up/Down inc/dec the active field and dispatch SetDateTime
    /// so the RTC tracks each step. The active field blinks on the LCD.
    fn datetime_key_event(&mut self, event: UiEvent, app: &App) -> Option<AppRequest> {
        // Out of edit mode: Enter starts editing at Year.
        if self.datetime_item == DateTimeEditItem::None {
            return match event {
                UiEvent::Enter => {
                    // Snapshot the live RTC value into the edit buffer so the
                    // first inc/dec works on the current time, not stale state.
                    self.edited_datetime = app.datetime;
                    self.datetime_item = DateTimeEditItem::Year;
                    self.datetime_blink = 0;
                    None
                }
                _ => None,
            };
        }

        // In edit mode.
        match event {
            UiEvent::Enter => {
                self.datetime_item = match self.datetime_item {
                    DateTimeEditItem::Year => DateTimeEditItem::Month,
                    DateTimeEditItem::Month => DateTimeEditItem::Day,
                    DateTimeEditItem::Day => DateTimeEditItem::Hours,
                    DateTimeEditItem::Hours => DateTimeEditItem::Minutes,
                    DateTimeEditItem::Minutes => DateTimeEditItem::Seconds,
                    DateTimeEditItem::Seconds | DateTimeEditItem::None => {
                        DateTimeEditItem::None
                    }
                };
                self.datetime_blink = 0;
                if self.datetime_item == DateTimeEditItem::None {
                    return Some(AppRequest::SetDateTime(self.edited_datetime));
                }
                None
            }
            // Hardware Up = UiEvent::Right, Down = UiEvent::Left.
            UiEvent::Up | UiEvent::Right => Some(AppRequest::SetDateTime(match self.datetime_item {
                DateTimeEditItem::Year => self.increment_year(),
                DateTimeEditItem::Month => self.increment_month(),
                DateTimeEditItem::Day => self.increment_day(),
                DateTimeEditItem::Hours => self.increment_hours(),
                DateTimeEditItem::Minutes => self.increment_minutes(),
                DateTimeEditItem::Seconds => self.increment_seconds(),
                DateTimeEditItem::None => self.edited_datetime,
            })),
            UiEvent::Down | UiEvent::Left => Some(AppRequest::SetDateTime(match self.datetime_item {
                DateTimeEditItem::Year => self.decrement_year(),
                DateTimeEditItem::Month => self.decrement_month(),
                DateTimeEditItem::Day => self.decrement_day(),
                DateTimeEditItem::Hours => self.decrement_hours(),
                DateTimeEditItem::Minutes => self.decrement_minutes(),
                DateTimeEditItem::Seconds => self.decrement_seconds(),
                DateTimeEditItem::None => self.edited_datetime,
            })),
            _ => None,
        }
    }

    // ─── DateTime edit helpers ──
    fn increment_seconds(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_add(time::Duration::seconds(1));
        self.edited_datetime
    }

    fn decrement_seconds(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_sub(time::Duration::seconds(1));
        self.edited_datetime
    }

    fn increment_minutes(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_add(time::Duration::minutes(1));
        self.edited_datetime
    }

    fn decrement_minutes(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_sub(time::Duration::minutes(1));
        self.edited_datetime
    }

    fn increment_hours(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_add(time::Duration::hours(1));
        self.edited_datetime
    }

    fn decrement_hours(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self
            .edited_datetime
            .saturating_sub(time::Duration::hours(1));
        self.edited_datetime
    }

    fn increment_day(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self.edited_datetime.saturating_add(time::Duration::days(1));
        self.edited_datetime
    }

    fn decrement_day(&mut self) -> PrimitiveDateTime {
        self.edited_datetime = self.edited_datetime.saturating_sub(time::Duration::days(1));
        self.edited_datetime
    }

    fn increment_month(&mut self) -> PrimitiveDateTime {
        let d = self.edited_datetime;
        let m = d.month().next();
        self.edited_datetime = d.replace_month(m).unwrap_or(d);
        self.edited_datetime
    }

    fn decrement_month(&mut self) -> PrimitiveDateTime {
        let d = self.edited_datetime;
        let m = d.month().previous();
        self.edited_datetime = d.replace_month(m).unwrap_or(d);
        self.edited_datetime
    }

    fn increment_year(&mut self) -> PrimitiveDateTime {
        let d = self.edited_datetime;
        if d.year() < 2099 {
            self.edited_datetime = d.replace_year(d.year() + 1).unwrap_or(d);
        }
        self.edited_datetime
    }

    fn decrement_year(&mut self) -> PrimitiveDateTime {
        let d = self.edited_datetime;
        if d.year() > 2000 {
            self.edited_datetime = d.replace_year(d.year() - 1).unwrap_or(d);
        }
        self.edited_datetime
    }

    /// Snapshot app datetime into edited_datetime when starting date edit
    pub fn begin_datetime_edit(&mut self, app: &App) {
        self.edited_datetime = app.datetime;
    }

    // ─── Version secret pattern ──
    /// C++ pattern: Enter,None,Enter,None,Enter,None,Up,None,Up,None,Down,None,Down
    /// Rust: without key-release events, pattern is Enter,Enter,Enter,Up,Up,Down,Down
    /// The pattern keys are consumed by this screen to prevent List navigation.
    fn version_key_event(&mut self, event: UiEvent) -> Option<AppRequest> {
        const PATTERN: [UiEvent; 7] = [
            UiEvent::Enter,
            UiEvent::Enter,
            UiEvent::Enter,
            UiEvent::Up,
            UiEvent::Up,
            UiEvent::Down,
            UiEvent::Down,
        ];

        let idx = self.pattern.matched as usize;
        if idx < PATTERN.len() && event == PATTERN[idx] {
            self.pattern.matched += 1;
            if self.pattern.matched as usize == PATTERN.len() {
                self.pattern.matched = 0;
                // Switch to calibration + shell mode (C++ Manufacture key)
                return Some(AppRequest::EnterCalibration);
            }
            return Some(AppRequest::Process); // consumed, don't navigate
        } else {
            self.pattern.matched = 0;
        }
        None
    }

    // ─── History key handler ──
    /// HourHistory: Enter cycles edit field Hour → Day → Month → Year → None.
    /// DayHistory: Day → Month → Year → None.
    /// MonthHistory: Month → Year → None.
    /// In edit mode: Up/Down inc/dec the active field, do not navigate screens.
    /// Out of edit mode: Up/Down fall through to screen-list navigation.
    fn history_key_event(&mut self, event: UiEvent, htype: HistoryType) -> Option<AppRequest> {
        // Out of edit mode: Enter starts editing at the screen's first field.
        if self.history_edit == HistoryEditField::None {
            return match event {
                UiEvent::Enter => {
                    self.history_edit = match htype {
                        HistoryType::Hour => HistoryEditField::Hour,
                        HistoryType::Day => HistoryEditField::Day,
                        HistoryType::Month => HistoryEditField::Month,
                    };
                    self.history_blink = 0;
                    None
                }
                _ => None,
            };
        }

        // In edit mode.
        match event {
            UiEvent::Enter => {
                // Advance to the next field, or exit edit mode after Year.
                self.history_edit = match self.history_edit {
                    HistoryEditField::Hour => HistoryEditField::Day,
                    HistoryEditField::Day => HistoryEditField::Month,
                    HistoryEditField::Month => HistoryEditField::Year,
                    HistoryEditField::Year | HistoryEditField::None => HistoryEditField::None,
                };
                self.history_blink = 0;
                // When exiting, dispatch the final flow lookup at the cursor.
                if self.history_edit == HistoryEditField::None {
                    let timestamp = self.history_datetime.assume_utc().unix_timestamp() as u32;
                    return Some(AppRequest::SetHistory(htype, timestamp));
                }
                None
            }
            // Hardware Up = UiEvent::Right (see keyboard.rs::to_ui_event).
            UiEvent::Up | UiEvent::Right => {
                self.history_inc();
                let timestamp = self.history_datetime.assume_utc().unix_timestamp() as u32;
                Some(AppRequest::SetHistory(htype, timestamp))
            }
            UiEvent::Down | UiEvent::Left => {
                self.history_dec();
                let timestamp = self.history_datetime.assume_utc().unix_timestamp() as u32;
                Some(AppRequest::SetHistory(htype, timestamp))
            }
            UiEvent::Back => {
                // Cancel edit mode without changing fields further.
                self.history_edit = HistoryEditField::None;
                None
            }
        }
    }

    fn history_inc(&mut self) {
        let dt = self.history_datetime;
        self.history_datetime = match self.history_edit {
            HistoryEditField::Hour => {
                let h = (dt.hour() + 1) % 24;
                dt.replace_hour(h).unwrap_or(dt)
            }
            HistoryEditField::Day => dt.saturating_add(Duration::DAY),
            HistoryEditField::Month => dt.replace_month(dt.month().next()).unwrap_or(dt),
            HistoryEditField::Year => dt.replace_year(dt.year() + 1).unwrap_or(dt),
            HistoryEditField::None => dt,
        };
    }

    fn history_dec(&mut self) {
        let dt = self.history_datetime;
        self.history_datetime = match self.history_edit {
            HistoryEditField::Hour => {
                let h = if dt.hour() == 0 { 23 } else { dt.hour() - 1 };
                dt.replace_hour(h).unwrap_or(dt)
            }
            HistoryEditField::Day => dt.saturating_sub(Duration::DAY),
            HistoryEditField::Month => dt.replace_month(dt.month().previous()).unwrap_or(dt),
            HistoryEditField::Year => dt.replace_year(dt.year() - 1).unwrap_or(dt),
            HistoryEditField::None => dt,
        };
    }

    // ─── Rendering ───────────────────────────────────────────────────
    /// Render current screen to LCD
    pub fn render(&mut self, app: &App, display: &mut impl CharacterDisplay) {
        // Free CGRAM bookkeeping at the start of each frame. Without this,
        // load_custom_char (display.rs) consumes a fresh slot every render
        // for the same Cyrillic glyph; after 8 frames all 8 slots are taken
        // and subsequent frames fall back to the Latin lookalike (e.g. 'д' → 'd').
        display.reset_custom_chars();

        let screen = self.current_screen();

        // History screens have a custom layout: title shows the unit being
        // browsed (hour/day/month) and value line shows date + flow.
        if matches!(
            screen,
            ScreenId::HourHistory | ScreenId::DayHistory | ScreenId::MonthHistory
        ) {
            self.render_history(screen, app, display);
            return;
        }

        // DateTime screen: two-line layout (date + time) with blinking on the
        // active edit field.
        if screen == ScreenId::DateTime {
            self.render_datetime(app, display);
            return;
        }

        let title = self.title(screen);

        // Use char count, not byte length — title and value may contain
        // multi-byte UTF-8 (Cyrillic). Without this, finish_line under-pads
        // and leaves stale DDRAM content in the trailing columns.
        display.set_position(0, 0);
        write!(display, "{}", title).ok();
        display.finish_line(16, title.chars().count());

        display.set_position(0, 1);
        let value = self.format_value(screen, app);
        write!(display, "{:>16}", value.as_str()).ok();
        display.finish_line(16, value.chars().count());
    }

    /// Render hour/day/month history screen with cursor + flow lookup result.
    /// Layout:
    ///   line 0: "Расход за час HH" (HourHistory)
    ///         | "Расход за день  " (DayHistory)
    ///         | "Расход за месяц " (MonthHistory)
    ///   line 1: "DD/MM/YY {value:>7}" — date in cols 0..8, value in 9..16.
    /// While editing, the active field renders as blanks for half the cycle
    /// (HISTORY_BLINK_PERIOD / 2 frames) to give a visible blink.
    fn render_history(
        &mut self,
        screen: ScreenId,
        app: &App,
        display: &mut impl CharacterDisplay,
    ) {
        // Tick blink counter on every frame.
        self.history_blink = (self.history_blink + 1) % HISTORY_BLINK_PERIOD;
        let blink_off = self.history_edit != HistoryEditField::None
            && self.history_blink >= HISTORY_BLINK_PERIOD / 2;
        let dt = self.history_datetime;

        // Helper: render field content unless the field is the active edit
        // field and we're in the "off" half of the blink cycle.
        let field = |edit: HistoryEditField, content: &str, blank: &str| -> alloc::string::String {
            if blink_off && self.history_edit == edit {
                blank.into()
            } else {
                content.into()
            }
        };

        // ─── Line 0: title ─────────────────────────────────────────────
        let mut line0 = alloc::string::String::new();
        match screen {
            ScreenId::HourHistory => {
                let hh = alloc::format!("{:02}", dt.hour());
                let hh_disp = field(HistoryEditField::Hour, &hh, "  ");
                write!(line0, "Расход за час {}", hh_disp).ok();
            }
            ScreenId::DayHistory => {
                line0.push_str("Расход за день");
            }
            ScreenId::MonthHistory => {
                line0.push_str("Расход за месяц");
            }
            _ => {}
        }
        display.set_position(0, 0);
        write!(display, "{}", line0).ok();
        display.finish_line(16, line0.chars().count());

        // ─── Line 1: date + value ─────────────────────────────────────
        // Date is "DD/MM/YY" (8 chars). Each numeric pair blinks when its
        // field is active. Slashes stay solid.
        let dd = field(
            HistoryEditField::Day,
            &alloc::format!("{:02}", dt.day()),
            "  ",
        );
        let mm = field(
            HistoryEditField::Month,
            &alloc::format!("{:02}", dt.month() as u8),
            "  ",
        );
        let yy = field(
            HistoryEditField::Year,
            &alloc::format!("{:02}", dt.year() % 100),
            "  ",
        );
        let date_str = alloc::format!("{}/{}/{}", dd, mm, yy);

        let value_str = match app.history_state.flow {
            Some(flow) => alloc::format!("{:.3}", flow),
            None => alloc::string::String::from("None"),
        };
        let line1 = alloc::format!("{} {:>7}", date_str, value_str);

        display.set_position(0, 1);
        write!(display, "{}", line1).ok();
        display.finish_line(16, line1.chars().count());
    }

    /// Render the DateTime screen.
    /// Layout (16x2):
    ///   line 0: "Дата   DD/MM/YY"  ("Дата " 4 chars + 3 spaces + 8-char date)
    ///   line 1: "Время  HH:MM:SS"  ("Время" 5 chars + 2 spaces + 8-char time)
    /// Date and time are column-aligned at col 7. While editing, the active
    /// numeric pair (YY/MM/DD/HH/MM/SS) blinks. When not editing, the live RTC
    /// time is shown; while editing, the working buffer is shown.
    fn render_datetime(&mut self, app: &App, display: &mut impl CharacterDisplay) {
        self.datetime_blink = (self.datetime_blink + 1) % HISTORY_BLINK_PERIOD;
        let blink_off = self.datetime_item != DateTimeEditItem::None
            && self.datetime_blink >= HISTORY_BLINK_PERIOD / 2;

        let dt = if self.datetime_item == DateTimeEditItem::None {
            app.datetime
        } else {
            self.edited_datetime
        };

        let field = |edit: DateTimeEditItem, content: &str| -> alloc::string::String {
            if blink_off && self.datetime_item == edit {
                "  ".into()
            } else {
                content.into()
            }
        };

        let dd = field(DateTimeEditItem::Day, &alloc::format!("{:02}", dt.day()));
        let mm_d = field(
            DateTimeEditItem::Month,
            &alloc::format!("{:02}", dt.month() as u8),
        );
        let yy = field(
            DateTimeEditItem::Year,
            &alloc::format!("{:02}", dt.year() % 100),
        );
        let hh = field(
            DateTimeEditItem::Hours,
            &alloc::format!("{:02}", dt.hour()),
        );
        let mm_t = field(
            DateTimeEditItem::Minutes,
            &alloc::format!("{:02}", dt.minute()),
        );
        let ss = field(
            DateTimeEditItem::Seconds,
            &alloc::format!("{:02}", dt.second()),
        );

        let line0 = alloc::format!("Дата    {}/{}/{}", dd, mm_d, yy);
        let line1 = alloc::format!("Время   {}:{}:{}", hh, mm_t, ss);

        display.set_position(0, 0);
        write!(display, "{}", line0).ok();
        display.finish_line(16, line0.chars().count());

        display.set_position(0, 1);
        write!(display, "{}", line1).ok();
        display.finish_line(16, line1.chars().count());
    }

    /// Update live values from measurement (C++ Menu::init statistics handler)
    pub fn update_live_values(&mut self, app: &App) {
        // Update live meter values from current app state
        // C++ version: hour_consumption_screen_->set_value(statistics::get_immediate())
        //   channel_1_screen_->set_text("работает"/"отсутствует")
        // Values are read from App when rendering via format_value()
        let _ = app; // values already accessible through app reference
    }

    /// Tick idle counter. Returns true if menu should auto-hide.
    pub fn tick_idle(&mut self) -> bool {
        if self.current_menu != MenuId::None && self.current_menu != MenuId::Calibration {
            if self.idle_counter > 0 {
                self.idle_counter -= 1;
            } else {
                return true; // auto-hide
            }
        }
        false
    }

    // ─── Compatibility methods for main.rs ───────────────────────────
    pub fn event(&mut self, e: UiEvent, app: &App) -> Option<AppRequest> {
        self.key_event(e, app)
    }

    pub fn invalidate(&mut self) {
        // Always re-render
    }

    pub fn update(&mut self, app: &App) {
        self.update_live_values(app);
    }

    pub fn get_active(&self) -> MenuId {
        self.current_menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new()
    }

    #[test]
    fn test_menu_list_navigation() {
        let mut list = MenuList::new();
        list.add(ScreenId::HourConsumption);
        list.add(ScreenId::DayConsumption);
        list.add(ScreenId::TotalVolume);
        assert_eq!(list.current(), ScreenId::HourConsumption);

        // next_enabled
        let always_enabled = |_s: ScreenId| true;
        list.next_enabled(always_enabled);
        assert_eq!(list.current(), ScreenId::DayConsumption);

        list.next_enabled(always_enabled);
        assert_eq!(list.current(), ScreenId::TotalVolume);

        list.next_enabled(always_enabled);
        assert_eq!(list.current(), ScreenId::HourConsumption); // wraps
    }

    #[test]
    fn test_menu_list_prev() {
        let mut list = MenuList::new();
        list.add(ScreenId::HourConsumption);
        list.add(ScreenId::DayConsumption);
        list.add(ScreenId::TotalVolume);

        let always_enabled = |_s: ScreenId| true;
        list.prev_enabled(always_enabled);
        assert_eq!(list.current(), ScreenId::TotalVolume); // wraps back
    }

    #[test]
    fn test_menu_controller_select() {
        let mut ctrl = MenuController::new();
        assert_eq!(ctrl.current_menu, MenuId::None);

        ctrl.select(MenuId::Main);
        assert_eq!(ctrl.current_menu, MenuId::Main);
        assert_eq!(ctrl.current_screen(), ScreenId::HourConsumption);
    }

    #[test]
    fn test_menu_controller_navigation() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // First key press when menu is None → activates main menu
        ctrl.current_menu = MenuId::None;
        let req = ctrl.key_event(UiEvent::Up, &app);
        assert_eq!(ctrl.current_menu, MenuId::Main);
    }

    #[test]
    fn test_menu_controller_exit() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        let req = ctrl.key_event(UiEvent::Back, &app);
        assert_eq!(req, Some(AppRequest::DeepSleep));
        assert_eq!(ctrl.current_menu, MenuId::None);
    }

    #[test]
    fn test_version_pattern() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // Navigate to version screen
        let always_enabled = |_s: ScreenId| true;
        for _ in 0..8 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true); // index 8 = Version
        }
        assert_eq!(ctrl.current_screen(), ScreenId::Version);

        // Enter the pattern — partial matches return Process to prevent navigation
        let result = ctrl.key_event(UiEvent::Enter, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Enter, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Enter, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Up, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Up, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Down, &app);
        assert!(result.is_some());
        let result = ctrl.key_event(UiEvent::Down, &app);
        assert_eq!(result, Some(AppRequest::EnterCalibration));
    }

    #[test]
    fn test_version_pattern_wrong_key_resets() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // Navigate to version
        let always_enabled = |_s: ScreenId| true;
        for _ in 0..8 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true);
        }

        // Partial pattern then wrong key
        ctrl.key_event(UiEvent::Enter, &app);
        ctrl.key_event(UiEvent::Enter, &app);
        let result = ctrl.key_event(UiEvent::Down, &app); // wrong!
        assert!(result.is_none());
        assert_eq!(ctrl.pattern.matched, 0); // reset
    }

    #[test]
    fn test_comm_type_cycling() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // Navigate to comm type screen (index 10)
        for _ in 0..10 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true);
        }
        assert_eq!(ctrl.current_screen(), ScreenId::CommType);

        // Enter cycles through the four comm types and wraps back to 0.
        // No edit mode — Enter advances directly.
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(1)));
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(2)));
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(3)));
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(0))); // wraps
    }

    #[test]
    fn test_slave_address_edit() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // Enable slave address by setting comm_type to M-BUS
        ctrl.comm_type.cursor = 1; // M-BUS

        // Navigate to slave address (index 11)
        for _ in 0..11 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true);
        }
        assert_eq!(ctrl.current_screen(), ScreenId::SlaveAddress);
        assert_eq!(ctrl.slave_address.value, 1);

        // Enter increments and commits — no separate edit mode.
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetAddress(2)));
        assert_eq!(ctrl.slave_address.value, 2);

        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetAddress(3)));

        // Wrap from max → min.
        ctrl.slave_address.value = ctrl.slave_address.max;
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetAddress(ctrl.slave_address.min)));
    }

    #[test]
    fn test_bootloader_reset() {
        let mut ctrl = MenuController::new();
        let app = test_app();
        ctrl.select(MenuId::Main);

        // Navigate to bootloader (index 9)
        let always_enabled = |_s: ScreenId| true;
        for _ in 0..9 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true);
        }

        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SystemReset));
    }

    #[test]
    fn test_editbox_enter_cycles() {
        // Enter advances the cursor through values and emits the change
        // request each press — no separate edit mode.
        let mut state = EditBoxState::default();
        assert_eq!(state.cursor, 0);

        let req = MenuController::editbox_key_event(&mut state, 4, UiEvent::Enter, |i| {
            AppRequest::SetCommType(i)
        });
        assert_eq!(req, Some(AppRequest::SetCommType(1)));
        assert_eq!(state.cursor, 1);

        let req = MenuController::editbox_key_event(&mut state, 4, UiEvent::Enter, |i| {
            AppRequest::SetCommType(i)
        });
        assert_eq!(req, Some(AppRequest::SetCommType(2)));

        // Two-value editbox wraps after one press.
        let mut two = EditBoxState::default();
        let req = MenuController::editbox_key_event(&mut two, 2, UiEvent::Enter, |i| {
            AppRequest::SetNegative(i > 0)
        });
        assert_eq!(req, Some(AppRequest::SetNegative(true)));
        let req = MenuController::editbox_key_event(&mut two, 2, UiEvent::Enter, |i| {
            AppRequest::SetNegative(i > 0)
        });
        assert_eq!(req, Some(AppRequest::SetNegative(false)));
    }

    #[test]
    fn test_idle_timeout() {
        let mut ctrl = MenuController::new();
        ctrl.select(MenuId::Main);
        ctrl.idle_counter = 3;

        assert!(!ctrl.tick_idle()); // 3→2
        assert!(!ctrl.tick_idle()); // 2→1
        assert!(!ctrl.tick_idle()); // 1→0
        assert!(ctrl.tick_idle()); // 0 → auto-hide
    }

    #[test]
    fn test_title_all_screens() {
        let ctrl = MenuController::new();
        // Every screen should have a non-empty title
        for i in 0..ctrl.main_menu.count {
            let title = ctrl.title(ctrl.main_menu.items[i]);
            assert!(
                !title.is_empty(),
                "Screen {:?} has empty title",
                ctrl.main_menu.items[i]
            );
        }
    }
}
