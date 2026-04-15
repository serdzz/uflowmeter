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
}

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
        // Cache enabled state to avoid borrow conflict
        let comm_cursor = self.comm_type.cursor;
        match event {
            UiEvent::Up => {
                self.current_list_mut().next_enabled(|s: ScreenId| match s {
                    ScreenId::SlaveAddress => comm_cursor != 0,
                    _ => true,
                });
                None
            }
            UiEvent::Down => {
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

            // ── DateTime: Enter starts editing, Left/Right change fields ──
            ScreenId::DateTime => self.datetime_key_event(event),

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
                MenuController::editbox_key_event(&mut self.sensor_type, 5, event, |_idx| {
                    // TODO: Calculator::init(sensor_type)
                    AppRequest::Process
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
                state.editable = !state.editable;
                if !state.editable {
                    Some(on_change(state.cursor))
                } else {
                    None
                }
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
                state.editable = !state.editable;
                if !state.editable {
                    Some(on_change(state.value))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ─── DateTime key handler ──
    fn datetime_key_event(&mut self, event: UiEvent) -> Option<AppRequest> {
        match self.datetime_item {
            DateTimeEditItem::None => {
                if event == UiEvent::Enter {
                    self.datetime_item = DateTimeEditItem::Seconds;
                    None
                } else {
                    None
                }
            }
            DateTimeEditItem::Seconds => match event {
                UiEvent::Left | UiEvent::Right => {
                    // TODO: increment/decrement seconds
                    None
                }
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::Minutes;
                    None
                }
                _ => None,
            },
            DateTimeEditItem::Minutes => match event {
                UiEvent::Left | UiEvent::Right => None,
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::Hours;
                    None
                }
                _ => None,
            },
            DateTimeEditItem::Hours => match event {
                UiEvent::Left | UiEvent::Right => None,
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::Day;
                    None
                }
                _ => None,
            },
            DateTimeEditItem::Day => match event {
                UiEvent::Left | UiEvent::Right => None,
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::Month;
                    None
                }
                _ => None,
            },
            DateTimeEditItem::Month => match event {
                UiEvent::Left | UiEvent::Right => None,
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::Year;
                    None
                }
                _ => None,
            },
            DateTimeEditItem::Year => match event {
                UiEvent::Left | UiEvent::Right => None,
                UiEvent::Enter => {
                    self.datetime_item = DateTimeEditItem::None;
                    // TODO: return SetDateTime
                    None
                }
                _ => None,
            },
        }
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
    fn history_key_event(&mut self, event: UiEvent, _htype: HistoryType) -> Option<AppRequest> {
        match event {
            UiEvent::Enter => {
                // Enter date editing mode
                // TODO: implement date navigation
                None
            }
            _ => None,
        }
    }

    // ─── Rendering ───────────────────────────────────────────────────
    /// Render current screen to LCD
    pub fn render(&mut self, app: &App, display: &mut impl CharacterDisplay) {
        let screen = self.current_screen();
        let title = self.title(screen);

        display.set_position(0, 0);
        write!(display, "{}", title).ok();
        display.finish_line(16, title.len());

        display.set_position(0, 1);
        let value = self.format_value(screen, app);
        write!(display, "{:>16}", value.as_str()).ok();
        display.finish_line(16, value.len());
    }

    /// Update live values from measurement (C++ Menu::init statistics handler)
    pub fn update_live_values(&mut self, _app: &App) {
        // TODO: called from statistics update handler
        // hour_consumption_screen_->set_value(statistics::get_immediate())
        // channel_1_screen_->set_text("работает"/"отсутствует")
        // etc.
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
        let always_enabled = |_s: ScreenId| true;
        for _ in 0..10 {
            ctrl.main_menu.next_enabled(|_s: ScreenId| true);
        }
        assert_eq!(ctrl.current_screen(), ScreenId::CommType);

        // Enter edit mode
        ctrl.key_event(UiEvent::Enter, &app);
        assert!(ctrl.comm_type.editable);

        // Cycle through types
        let req = ctrl.key_event(UiEvent::Right, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(1)));
        let req = ctrl.key_event(UiEvent::Right, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(2)));
        let req = ctrl.key_event(UiEvent::Right, &app);
        assert_eq!(req, Some(AppRequest::SetCommType(3)));
        let req = ctrl.key_event(UiEvent::Right, &app);
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

        // Enter edit mode
        ctrl.key_event(UiEvent::Enter, &app);
        assert!(ctrl.slave_address.editable);

        // Increment
        ctrl.key_event(UiEvent::Up, &app);
        assert_eq!(ctrl.slave_address.value, 2);

        // Exit edit mode
        let req = ctrl.key_event(UiEvent::Enter, &app);
        assert_eq!(req, Some(AppRequest::SetAddress(2)));
        assert!(!ctrl.slave_address.editable);
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
    fn test_editbox_toggle() {
        let mut state = EditBoxState::default();
        assert!(!state.editable);

        // Enter toggles edit mode
        let _ = MenuController::editbox_key_event(&mut state, 4, UiEvent::Enter, |i| {
            AppRequest::SetCommType(i)
        });
        assert!(state.editable);

        let _ = MenuController::editbox_key_event(&mut state, 4, UiEvent::Enter, |i| {
            AppRequest::SetCommType(i)
        });
        assert!(!state.editable);
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
