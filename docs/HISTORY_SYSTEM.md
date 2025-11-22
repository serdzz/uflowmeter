# History System

## Обзор

Система истории предназначена для хранения и извлечения исторических данных о расходе воды за разные периоды времени. Данные хранятся в энергонезависимой памяти (EEPROM) в виде кольцевых буферов с фиксированными временными интервалами.

## Типы истории

```rust
pub enum HistoryType {
    Hour,   // Почасовая история
    Day,    // Дневная история
    Month,  // Месячная история
}
```

### Параметры хранения

| Тип | Интервал | Размер буфера | Период хранения |
|-----|----------|---------------|-----------------|
| Hour | 3600 сек (1 час) | 2160 записей | ~90 дней |
| Day | 86400 сек (1 день) | 1116 записей | ~3 года |
| Month | ~2592000 сек (~30 дней) | 120 записей | ~10 лет |

## AppRequest::SetHistory

### Назначение

`AppRequest::SetHistory(history_type, timestamp)` - запрос на получение исторических данных о расходе за указанный период времени.

### Параметры

```rust
AppRequest::SetHistory(
    history_type: HistoryType,  // Тип истории (Hour/Day/Month)
    timestamp: u32              // Unix timestamp (секунды с 1970-01-01)
)
```

- **`history_type`** - определяет, из какого буфера извлекать данные
- **`timestamp`** - временная метка для поиска записи (автоматически округляется до минут)

### Процесс обработки

```
1. UI Widget генерирует Actions::SetHistory
   └─> Пользователь изменяет дату/время в History виджете
   
2. App::handle_event преобразует в AppRequest::SetHistory
   └─> Маршрутизирует запрос в RTIC task
   
3. app_request task обрабатывает запрос
   ├─> Определяет тип истории (Hour/Day/Month)
   ├─> Блокирует доступ к ресурсам (app, history, storage)
   ├─> Вызывает history.find(storage, timestamp)
   └─> Обновляет app.history_state.flow
   
4. UI обновляется на следующем цикле
   └─> Widget::update() читает app.history_state
```

## Реализация в main.rs

```rust
#[task(capacity = 8, priority = 1, 
       shared = [power, lcd, rtc, app, 
                 hour_history, day_history, month_history, storage])]
fn app_request(ctx: app_request::Context, req: AppRequest) {
    match req {
        AppRequest::SetHistory(history_type, timestamp) => {
            defmt::info!("SetHistory");
            
            match history_type {
                HistoryType::Hour => {
                    // Блокировка ресурсов для атомарного доступа
                    (app, hour_history, storage).lock(|app, hour_history, storage| {
                        // Поиск записи по timestamp
                        if let Ok(Some(flow)) = hour_history.find(storage, timestamp) {
                            app.history_state.flow = Some(flow as f32);
                        } else {
                            app.history_state.flow = None;
                        }
                    });
                }
                HistoryType::Day => {
                    (app, day_history, storage).lock(|app, day_history, storage| {
                        if let Ok(Some(flow)) = day_history.find(storage, timestamp) {
                            app.history_state.flow = Some(flow as f32);
                        } else {
                            app.history_state.flow = None;
                        }
                    });
                }
                HistoryType::Month => {
                    (app, month_history, storage).lock(|app, month_history, storage| {
                        if let Ok(Some(flow)) = month_history.find(storage, timestamp) {
                            app.history_state.flow = Some(flow as f32);
                        } else {
                            app.history_state.flow = None;
                        }
                    });
                }
            }
        }
    }
}
```

## Структура RingStorage

### Определение

```rust
pub struct RingStorage<const OFFSET: usize, const SIZE: i32, const ELEMENT_SIZE: i32> {
    pub data: ServiceData,
}

#[bitfield]
pub struct ServiceData {
    pub size: u32,              // Количество записей в буфере
    pub offset_of_last: u32,    // Индекс последней записи
    pub time_of_last: u32,      // Timestamp последней записи
    crc: u16,                   // Контрольная сумма
}
```

### Параметры типа

- **`OFFSET`** - смещение в EEPROM (байты от начала страницы статистики)
- **`SIZE`** - максимальное количество записей
- **`ELEMENT_SIZE`** - интервал между записями (секунды)

### Примеры инициализации

```rust
// Почасовая история: 2160 записей × 1 час
type HourHistory = RingStorage<0, 2160, 3600>;

// Дневная история: 1116 записей × 1 день
type DayHistory = RingStorage<8640, 1116, 86400>;

// Месячная история: 120 записей × ~30 дней
type MonthHistory = RingStorage<13104, 120, 2592000>;
```

## Метод find()

### Сигнатура

```rust
pub fn find(&mut self, storage: &mut MyStorage, time: u32) 
    -> Result<Option<i32>>
```

### Алгоритм

1. **Нормализация timestamp**
   ```rust
   let time = time - time % 60;  // Округление до минут
   ```

2. **Проверка наличия данных**
   ```rust
   if self.data.size() == 0 {
       return Ok(None);  // Нет записей
   }
   ```

3. **Обратный обход кольцевого буфера**
   ```rust
   let mut index = self.data.offset_of_last() as usize;
   for _ in 0..self.data.size() {
       // Вычисление ожидаемого timestamp для текущего индекса
       let expected_time = self.data.time_of_last()
           - (self.data.size() - 1 - index as u32) * ELEMENT_SIZE as u32;
       
       if expected_time == time {
           // Найдена запись, читаем значение из EEPROM
           let offset = self.offset(index);
           let mut buf = [0_u8; size_of::<i32>()];
           storage.read(offset, &mut buf)?;
           let value = i32::from_le_bytes(buf);
           return Ok(Some(value));
       }
       
       // Переход к предыдущей записи (с wraparound)
       if index == 0 {
           index = SIZE as usize - 1;
       } else {
           index -= 1;
       }
   }
   ```

4. **Результат**
   - `Ok(Some(value))` - запись найдена
   - `Ok(None)` - запись не найдена (нет данных за этот период)
   - `Err(...)` - ошибка чтения из EEPROM

### Временная сложность

- **O(n)** где n = количество записей в буфере
- В худшем случае проверяется весь буфер

## Пример использования в UI

### HistoryWidget

```rust
impl Widget<&App, Actions> for HistoryWidget {
    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Right => {
                    // Пользователь увеличивает дату/время
                    self.inc();  // datetime += интервал
                    
                    // Пересчитываем timestamp
                    self.timestamp = self.datetime
                        .assume_utc()
                        .unix_timestamp() as u32;
                    
                    // Запрашиваем данные из истории
                    Some(Actions::SetHistory(
                        self.history_type, 
                        self.timestamp
                    ))
                }
                UiEvent::Left => {
                    // Пользователь уменьшает дату/время
                    self.dec();  // datetime -= интервал
                    self.timestamp = self.datetime
                        .assume_utc()
                        .unix_timestamp() as u32;
                    Some(Actions::SetHistory(
                        self.history_type, 
                        self.timestamp
                    ))
                }
                _ => None,
            }
        } else {
            None
        }
    }
    
    fn update(&mut self, state: &App) {
        // Отображение результата запроса
        if let Some(flow) = state.history_state.flow {
            write!(self.value, "{flow}").ok();
        } else {
            write!(self.value, "None").ok();
        }
    }
}
```

## Сценарии использования

### 1. Просмотр истории за конкретный час

```
Пользователь:
1. Переходит на экран Hour History
2. Нажимает Enter → входит в режим редактирования
3. Изменяет дату/время кнопками Left/Right
   
Система:
1. Widget генерирует Actions::SetHistory(Hour, timestamp)
2. App преобразует в AppRequest::SetHistory(Hour, timestamp)
3. RTIC task вызывает hour_history.find(storage, timestamp)
4. Результат записывается в app.history_state.flow
5. Widget отображает значение на дисплее
```

### 2. Навигация по дням

```
Пользователь:
1. Переходит на экран Day History
2. Входит в режим редактирования
3. Листает дни: Left (назад) / Right (вперед)

Система:
- При каждом нажатии:
  - timestamp += 86400 (или -= 86400)
  - Запрос SetHistory(Day, new_timestamp)
  - Обновление отображения
```

### 3. Просмотр месячной статистики

```
Пользователь:
1. Переходит на экран Month History
2. Выбирает месяц/год

Система:
- Интервал ~30 дней (2592000 секунд)
- Меньше точность, больше период хранения
```

## Запись данных

Данные записываются автоматически в `app_request(AppRequest::Process)`:

```rust
AppRequest::Process => {
    // ... измерения ...
    
    // Каждую минуту (когда second < 5)
    if datetime.time().second() < 5 {
        let timestamp = datetime.as_utc().unix_timestamp();
        
        // Каждый час (minute == 0)
        if datetime.time().minute() == 0 {
            hour_history.add(storage, hour_flow as i32, timestamp as u32);
            
            // Каждый день (hour == 0)
            if datetime.time().hour() == 0 {
                day_history.add(storage, day_flow as i32, timestamp as u32);
                
                // Каждый месяц (day == 1)
                if datetime.date().day() == 1 {
                    month_history.add(storage, month_flow as i32, timestamp as u32);
                }
            }
        }
    }
}
```

## Обработка ошибок

### Возможные ошибки

```rust
pub enum Error {
    NoRecords,       // Буфер пуст
    Unitialized,     // Система не инициализирована
    Storage,         // Ошибка EEPROM
    WrongCrc,        // Неверная контрольная сумма
    Spi(spi::Error), // Ошибка SPI
}
```

### Обработка в UI

```rust
match hour_history.find(storage, timestamp) {
    Ok(Some(flow)) => {
        // Данные найдены
        app.history_state.flow = Some(flow as f32);
    }
    Ok(None) => {
        // Нет данных за этот период
        app.history_state.flow = None;
        // UI отобразит "None"
    }
    Err(e) => {
        // Ошибка чтения
        defmt::error!("History read error: {:?}", e);
        app.history_state.flow = None;
    }
}
```

## Память

### Расчет размера на EEPROM

```rust
const SIZE_ON_FLASH: usize = 
    size_of::<u32>()           // Заголовок
    + SIZE * size_of::<i32>()  // Данные (SIZE записей × 4 байта)
    + size_of::<ServiceData>() // Метаданные (16 байт)
    + size_of::<u16>();        // CRC (2 байта)
```

### Примеры

- **Hour History**: 4 + 2160×4 + 16 + 2 = **8662 байта**
- **Day History**: 4 + 1116×4 + 16 + 2 = **4486 байт**
- **Month History**: 4 + 120×4 + 16 + 2 = **502 байта**

**Итого**: ~13.7 КБ из доступных 128 КБ EEPROM

## Кольцевой буфер

### Структура

```
┌───────────────────────────────────────────┐
│ ServiceData (метаданные)                  │
│  - size: 100                              │
│  - offset_of_last: 75                     │
│  - time_of_last: 1700000000               │
│  - crc: 0xABCD                            │
├───────────────────────────────────────────┤
│ Data[0]: 1234   ← самая старая запись    │
│ Data[1]: 2345                             │
│ ...                                       │
│ Data[75]: 9876  ← последняя запись        │
│ Data[76]: (пусто)                         │
│ ...                                       │
│ Data[SIZE-1]: (пусто)                     │
└───────────────────────────────────────────┘
```

### Операции

- **Запись новых данных**: перезапись самой старой записи
- **Чтение**: обратный обход от последней записи
- **Переполнение**: автоматический wraparound индекса

## Тестирование

### Unit тесты

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_existing_record() {
        let mut storage = MockStorage::new();
        let mut history = RingStorage::<0, 100, 3600>::new(&mut storage).unwrap();
        
        // Добавляем тестовые данные
        history.add(&mut storage, 1234, 1700000000).ok();
        
        // Ищем запись
        let result = history.find(&mut storage, 1700000000);
        assert_eq!(result, Ok(Some(1234)));
    }
    
    #[test]
    fn test_find_nonexistent_record() {
        let mut storage = MockStorage::new();
        let mut history = RingStorage::<0, 100, 3600>::new(&mut storage).unwrap();
        
        // Ищем несуществующую запись
        let result = history.find(&mut storage, 1700000000);
        assert_eq!(result, Ok(None));
    }
}
```

## Производительность

- **Чтение из EEPROM**: ~1-2 мс на запись
- **Поиск в буфере**: O(n), где n ≤ SIZE
- **Worst case**: 2160 операций для Hour History
- **Типичный случай**: несколько итераций (недавние данные)

## Оптимизации

### Возможные улучшения

1. **Бинарный поиск** (требует sorted buffer)
2. **Кеширование** последних запросов
3. **Индексирование** по диапазонам дат
4. **Сжатие** данных (дельта-кодирование)

### Текущие оптимизации

- Нормализация timestamp до минут (уменьшение вариантов)
- Кольцевой буфер (O(1) запись)
- CRC защита метаданных
- Прямой доступ к EEPROM (без буферизации)

## Диаграмма взаимодействия

```
┌──────────┐  Left/Right  ┌──────────────┐
│   User   │─────────────>│ HistoryWidget│
└──────────┘              └──────┬───────┘
                                 │ inc()/dec()
                                 │ timestamp = datetime.unix_timestamp()
                                 v
                          ┌──────────────┐
                          │   Actions    │
                          │ SetHistory() │
                          └──────┬───────┘
                                 │
                                 v
                          ┌──────────────┐
                          │     App      │
                          │handle_event()│
                          └──────┬───────┘
                                 │
                                 v
                          ┌──────────────┐
                          │ AppRequest   │
                          │ SetHistory() │
                          └──────┬───────┘
                                 │ RTIC queue
                                 v
                          ┌──────────────┐
                          │ app_request  │
                          │    task      │
                          └──────┬───────┘
                                 │ lock resources
                                 v
            ┌────────────────────┼────────────────────┐
            │                    │                    │
            v                    v                    v
    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
    │hour_history  │    │ day_history  │    │month_history │
    │   .find()    │    │   .find()    │    │   .find()    │
    └──────┬───────┘    └──────┬───────┘    └──────┬───────┘
           │                   │                    │
           └───────────────────┼────────────────────┘
                               │ value or None
                               v
                        ┌──────────────┐
                        │  app.history │
                        │    _state    │
                        └──────┬───────┘
                               │
                               v
                        ┌──────────────┐
                        │ Widget       │
                        │ .update()    │
                        └──────┬───────┘
                               │
                               v
                        ┌──────────────┐
                        │   Display    │
                        │   (16x2)     │
                        └──────────────┘
```

## См. также

- [UI_ARCHITECTURE.md](UI_ARCHITECTURE.md) - общая архитектура UI
- `src/history.rs` - реализация RingStorage
- `src/apps.rs` - App state и Actions
- `src/main.rs` - RTIC task app_request
