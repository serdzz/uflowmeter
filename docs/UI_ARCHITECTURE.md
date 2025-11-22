# UI Architecture

## Обзор

UI система построена на паттерне однонаправленного потока данных с разделением состояния и представления. Архитектура оптимизирована для встраиваемых систем (`no_std`) и символьных дисплеев (16x2 LCD).

## Базовые компоненты

### Widget Trait

Основной интерфейс для всех UI элементов:

```rust
pub trait Widget<S, A> {
    fn invalidate(&mut self);              // Пометить для перерисовки
    fn update(&mut self, state: S);        // Обновить из состояния приложения
    fn render(&mut self, display: ...);    // Отрисовать на дисплей
    fn event(&mut self, e: UiEvent) -> Option<A>; // Обработать событие
}
```

**Параметры типа:**
- `S` - тип состояния приложения (обычно `&App`)
- `A` - тип действий (обычно `Actions`)

### CharacterDisplay Trait

Абстракция для символьных дисплеев:

```rust
pub trait CharacterDisplay: core::fmt::Write {
    fn set_position(&mut self, col: u8, row: u8);
    fn clear(&mut self);
    fn reset_custom_chars(&mut self);
    fn finish_line(&mut self, width: usize, len: usize);
}
```

### Примитивные виджеты

Расположены в `src/gui/`:

- **`Label`** - статический текст с поддержкой форматирования
- **`Edit`** - редактируемое текстовое поле с поддержкой мигания
- **`EditBox`** - текстовое поле с курсором

### События

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

## Макросы для композиции виджетов

### widget_group!

Создает виджет-контейнер, где **все дочерние виджеты отображаются одновременно**:

```rust
widget_group!(
    MyView<&App, Actions>,
    {
        label: Label<Actions, 16, 0, 0>;
        edit: Edit<Actions, 16, 8, 1>;
    },
    |view, state: &App| {
        // Функция обновления
        view.label.update(state);
        view.edit.update(state);
    },
    |view, event: UiEvent| -> Option<Actions> {
        // Обработка событий
        view.edit.event(event)
    }
)
```

**Использование:**
- Группировка связанных виджетов
- Составные экраны с несколькими элементами
- Все виджеты рендерятся при каждом вызове `render()`

### widget_mux!

Создает мультиплексор виджетов, где **отображается только один активный**:

```rust
widget_mux!(
    Viewport<&App, Actions>,
    ViewportNode::Label,  // активный виджет по умолчанию
    {
        label: LabelWidget;
        datetime: DateTimeWidget;
        hour_history: HistoryWidget;
    },
    |viewport, state: &App| {
        // Обновление всех виджетов
        viewport.label.update(state);
        viewport.datetime.update(state);
        viewport.hour_history.update(state);
    },
    |viewport, event: UiEvent| -> Option<Actions> {
        // Маршрутизация событий к активному виджету
        match viewport.active {
            ViewportNode::Label => viewport.label.event(event),
            ViewportNode::Datetime => viewport.datetime.event(event),
            ViewportNode::HourHistory => viewport.hour_history.event(event),
        }
    }
)
```

**Автоматически генерируется:**
- Enum `ViewportNode` с вариантами для каждого виджета
- Метод `set_active(node: ViewportNode)` для переключения
- Логика рендеринга только активного виджета

**Использование:**
- Навигация между экранами
- Режимы работы (просмотр/редактирование)
- Экономия ресурсов (рендерится только видимый экран)

## Архитектура приложения

### App State

Центральное состояние приложения (`src/apps.rs`):

```rust
pub struct App {
    pub datetime: PrimitiveDateTime,
    pub active_widget: ViewportNode,     // текущий активный экран
    pub flow: f32,                       // текущий расход
    pub hour_flow: f32,                  // часовой расход
    pub day_flow: f32,                   // дневной расход
    pub month_flow: f32,                 // месячный расход
    pub history_state: HistoryState,     // состояние истории
    // ...
}

impl App {
    pub fn handle_event(&mut self, action: Option<Actions>) 
        -> Option<AppRequest> {
        // Преобразование действий в системные запросы
    }
}
```

### Actions

Типизированные действия пользователя:

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

Запросы к системным компонентам:

```rust
pub enum AppRequest {
    Process,                              // Запустить измерение
    LcdLed(bool),                        // Управление подсветкой
    SetDateTime(PrimitiveDateTime),      // Установить время
    SetHistory(HistoryType, u32),        // Запросить историю
    DeepSleep,                           // Перейти в сон
}
```

## Поток данных

Однонаправленный поток обработки:

```
┌─────────────┐
│   События   │  Нажатия кнопок (Up/Down/Left/Right/Enter/Back)
│  (Кнопки)   │
└──────┬──────┘
       │
       v
┌─────────────────┐
│ Widget::event() │  Обработка в активном виджете
└──────┬──────────┘
       │
       v
┌─────────────┐
│   Actions   │  Типизированное действие
└──────┬──────┘
       │
       v
┌──────────────────────┐
│ App::handle_event()  │  Бизнес-логика приложения
└──────┬───────────────┘
       │
       v
┌─────────────┐
│ AppRequest  │  Запрос к системе (RTC, EEPROM, измерения)
└──────┬──────┘
       │
       v
┌──────────────┐
│   Система    │  Выполнение операций (RTC, EEPROM, TDC7200)
└──────┬───────┘
       │
       v
┌──────────────┐
│  App State   │  Обновление состояния
└──────┬───────┘
       │
       v
┌──────────────────┐
│ Widget::update() │  Синхронизация с состоянием
└──────┬───────────┘
       │
       v
┌──────────────────┐
│ Widget::render() │  Отрисовка на дисплей
└──────┬───────────┘
       │
       v
┌─────────────┐
│   Display   │  16x2 LCD
└─────────────┘
```

## Пример: History Widget

Полный пример виджета для просмотра истории (`src/ui.rs`):

```rust
pub struct HistoryWidget {
    date: Edit<Actions, 16, 8, 0>,      // Поле даты
    time: Edit<Actions, 16, 0, 1>,      // Поле времени
    label: Edit<Actions, 16, 0, 0>,     // Метка "From"
    value: Edit<Actions, 16, 10, 1>,    // Значение расхода
    items: DateTimeItems,                // Текущее редактируемое поле
    datetime: PrimitiveDateTime,         // Дата/время
    editable: bool,                      // Режим редактирования
    timestamp: u32,                      // Unix timestamp
    history_type: HistoryType,           // Тип истории (Hour/Day/Month)
}

impl Widget<&App, Actions> for HistoryWidget {
    fn invalidate(&mut self) {}
    
    fn update(&mut self, state: &App) {
        // Синхронизация с состоянием приложения
        if !self.editable {
            self.datetime = state.datetime;
        }
        
        // Очистка буферов
        self.date.state.clear();
        self.time.state.clear();
        self.value.state.clear();
        
        // Форматирование данных
        write!(
            self.date,
            "{:02}/{:02}/{:02}",
            self.datetime.day(),
            self.datetime.month() as u8,
            self.datetime.year() - 2000
        ).ok();
        
        write!(
            self.time, 
            "{:02}:{:02}:{:02}", 
            self.datetime.hour(), 
            0, 
            0
        ).ok();
        
        if let Some(flow) = state.history_state.flow {
            write!(self.value, "{flow}").ok();
        } else {
            write!(self.value, "None").ok();
        }
    }
    
    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            // Режим редактирования
            match event {
                UiEvent::Left => {
                    self.dec();  // Уменьшить значение
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Right => {
                    self.inc();  // Увеличить значение
                    Some(Actions::SetHistory(self.history_type, self.timestamp))
                }
                UiEvent::Enter => {
                    self.next_item();  // Следующее поле
                    None
                }
                _ => None,
            }
        } else {
            // Режим навигации
            match event {
                UiEvent::Enter => {
                    self.next_item();  // Войти в режим редактирования
                    None
                }
                UiEvent::Left => Some(Actions::Label),
                UiEvent::Right => Some(Actions::DayHistory),
                _ => None,
            }
        }
    }
    
    fn render(&mut self, display: &mut impl CharacterDisplay) {
        self.date.render(display);
        self.label.render(display);
        self.value.render(display);
        self.time.render(display);
    }
}
```

## Ключевые принципы

### 1. State Separation

Состояние хранится в `App`, виджеты только отображают и генерируют действия:

```rust
// ✅ Правильно - состояние в App
pub struct App {
    pub datetime: PrimitiveDateTime,
}

pub struct DateTimeWidget {
    edit: Edit<...>,  // только UI элементы
}

// ❌ Неправильно - дублирование состояния
pub struct DateTimeWidget {
    datetime: PrimitiveDateTime,  // не нужно!
}
```

### 2. Action-based Communication

События преобразуются в типизированные действия:

```rust
// ✅ Правильно - типизированные действия
fn event(&mut self, event: UiEvent) -> Option<Actions> {
    match event {
        UiEvent::Enter => Some(Actions::SetDateTime(self.datetime)),
        _ => None,
    }
}

// ❌ Неправильно - прямое изменение состояния
fn event(&mut self, event: UiEvent, app: &mut App) {
    app.datetime = self.datetime;  // нарушает архитектуру!
}
```

### 3. Composability

Сложные UI строятся из простых виджетов:

```rust
// Примитивы
Edit + Label + EditBox

// Композиция через widget_group!
HistoryWidget = Edit (date) + Edit (time) + Edit (label) + Edit (value)

// Навигация через widget_mux!
Viewport = Label | DateTime | HourHistory | DayHistory | MonthHistory
```

### 4. No Allocation

Работа без динамической аллокации памяти:

```rust
// Фиксированные буферы
pub struct Edit<A, const N: usize, const COL: u8, const ROW: u8> {
    state: String<N>,  // heapless::String
}

// Запись через форматирование
write!(self.edit, "{:02}:{:02}", hour, minute).ok();
```

### 5. Character Display Optimization

Оптимизация для символьных дисплеев 16x2:

```rust
// Позиционирование
Edit<Actions, 16, 8, 0>  // 16 символов, колонка 8, строка 0

// Мигание маски (для редактирования)
edit.blink_mask(0x03);    // Мигают последние 2 символа (биты 0-1)
edit.blink_mask(0xc0);    // Мигают первые 2 символа (биты 6-7)
```

## Расширение системы

### Добавление нового виджета

1. **Создать структуру виджета:**

```rust
pub struct MyWidget {
    label: Label<Actions, 16, 0, 0>,
    value: Edit<Actions, 16, 8, 1>,
    count: u32,
}
```

2. **Реализовать Widget trait:**

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

3. **Добавить в Viewport:**

```rust
widget_mux!(
    Viewport<&App, Actions>,
    ViewportNode::Label,
    {
        label: LabelWidget;
        my_widget: MyWidget;  // <-- добавить
    },
    update_fn,
    event_fn
)
```

### Добавление нового действия

1. **Расширить enum Actions:**

```rust
pub enum Actions {
    // ...
    MyNewAction(u32),
}
```

2. **Обработать в App::handle_event:**

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

## Тестирование

UI логика тестируется через mock дисплеи:

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

## Производительность

- **Update**: O(n) где n - количество виджетов в группе/мультиплексоре
- **Render**: O(1) для mux (только активный), O(n) для group
- **Event**: O(1) - прямая маршрутизация к активному виджету
- **Memory**: Zero allocation, все буферы статические

## Файловая структура

```
src/
├── gui/                    # Базовые GUI компоненты
│   ├── mod.rs             # Widget trait, UiEvent, CharacterDisplay
│   ├── label.rs           # Label виджет
│   ├── edit.rs            # Edit виджет
│   ├── editbox.rs         # EditBox виджет
│   └── macros.rs          # widget_group!, widget_mux!
├── apps.rs                # App state, Actions, AppRequest
├── ui.rs                  # Составные виджеты (HistoryWidget, etc)
└── main.rs                # Интеграция с RTIC
```
