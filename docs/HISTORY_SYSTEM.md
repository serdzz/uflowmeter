# History System

## Overview

The history system stores and retrieves historical flow data over different time periods. Data is persisted in non-volatile memory (EEPROM) as ring buffers with fixed time intervals.

## History Types

```rust
pub enum HistoryType {
    Hour,   // Hourly history
    Day,    // Daily history
    Month,  // Monthly history
}
```

### Storage Parameters

| Type  | Interval              | Buffer size    | Retention  |
|-------|-----------------------|----------------|------------|
| Hour  | 3600 s (1 hour)       | 2160 records   | ~90 days   |
| Day   | 86400 s (1 day)       | 1116 records   | ~3 years   |
| Month | ~2592000 s (~30 days) | 120 records    | ~10 years  |

## AppRequest::SetHistory

### Purpose

`AppRequest::SetHistory(history_type, timestamp)` — requests historical flow data for a given time period.

### Parameters

```rust
AppRequest::SetHistory(
    history_type: HistoryType,  // History type (Hour/Day/Month)
    timestamp: u32              // Unix timestamp (seconds since 1970-01-01)
)
```

- **`history_type`** — selects which ring buffer to read from
- **`timestamp`** — the time to look up (automatically rounded to minutes)

### Processing Flow

```
1. UI Widget generates Actions::SetHistory
   └─> User changes date/time in the History widget

2. App::handle_event converts to AppRequest::SetHistory
   └─> Routes request to the RTIC task

3. app_request task handles the request
   ├─> Determines history type (Hour/Day/Month)
   ├─> Acquires locks on resources (app, history, storage)
   ├─> Calls history.find(storage, timestamp)
   └─> Updates app.history_state.flow

4. UI updates on the next cycle
   └─> Widget::update() reads app.history_state
```

## Implementation in main.rs

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
                    // Acquire locks for atomic resource access
                    (app, hour_history, storage).lock(|app, hour_history, storage| {
                        // Search for record by timestamp
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

## RingStorage Structure

### Definition

```rust
pub struct RingStorage<const OFFSET: usize, const SIZE: i32, const ELEMENT_SIZE: i32> {
    pub data: ServiceData,
}

#[bitfield]
pub struct ServiceData {
    pub size: u32,              // Number of records in the buffer
    pub offset_of_last: u32,    // Index of the last record
    pub time_of_last: u32,      // Timestamp of the last record
    crc: u16,                   // Checksum
}
```

### Type Parameters

- **`OFFSET`** — byte offset in EEPROM (from the start of the statistics page)
- **`SIZE`** — maximum number of records
- **`ELEMENT_SIZE`** — interval between records (seconds)

### Instantiation Examples

```rust
// Hourly history: 2160 records × 1 hour
type HourHistory = RingStorage<0, 2160, 3600>;

// Daily history: 1116 records × 1 day
type DayHistory = RingStorage<8640, 1116, 86400>;

// Monthly history: 120 records × ~30 days
type MonthHistory = RingStorage<13104, 120, 2592000>;
```

## The find() Method

### Signature

```rust
pub fn find(&mut self, storage: &mut MyStorage, time: u32) 
    -> Result<Option<i32>>
```

### Algorithm

1. **Normalize timestamp**
   ```rust
   let time = time - time % 60;  // Round down to minutes
   ```

2. **Check if data exists**
   ```rust
   if self.data.size() == 0 {
       return Ok(None);  // No records
   }
   ```

3. **Reverse scan of the ring buffer**
   ```rust
   let mut index = self.data.offset_of_last() as usize;
   for _ in 0..self.data.size() {
       // Compute the expected timestamp for the current index
       let expected_time = self.data.time_of_last()
           - (self.data.size() - 1 - index as u32) * ELEMENT_SIZE as u32;

       if expected_time == time {
           // Record found, read value from EEPROM
           let offset = self.offset(index);
           let mut buf = [0_u8; size_of::<i32>()];
           storage.read(offset, &mut buf)?;
           let value = i32::from_le_bytes(buf);
           return Ok(Some(value));
       }

       // Move to the previous record (with wraparound)
       if index == 0 {
           index = SIZE as usize - 1;
       } else {
           index -= 1;
       }
   }
   ```

4. **Result**
   - `Ok(Some(value))` — record found
   - `Ok(None)` — record not found (no data for this period)
   - `Err(...)` — EEPROM read error

### Time Complexity

- **O(n)** where n = number of records in the buffer
- In the worst case the entire buffer is scanned

## Usage in UI

### HistoryWidget

```rust
impl<K: HistoryKind> Widget<&App, Actions> for HistoryWidget<K> {
    fn event(&mut self, event: UiEvent) -> Option<Actions> {
        if self.editable {
            match event {
                UiEvent::Right => {
                    // User increments date/time
                    self.inc();
                    Some(Actions::SetHistory(K::history_type(), self.timestamp))
                }
                UiEvent::Left => {
                    // User decrements date/time
                    self.dec();
                    Some(Actions::SetHistory(K::history_type(), self.timestamp))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn update(&mut self, state: &App) {
        // Display the query result
        if let Some(flow) = state.history_state.flow {
            write!(self.value, "{flow}").ok();
        } else {
            write!(self.value, "None").ok();
        }
    }
}
```

## Usage Scenarios

### 1. View history for a specific hour

```
User:
1. Navigates to the Hour History screen
2. Presses Enter → enters edit mode
3. Adjusts date/time using Left/Right buttons

System:
1. Widget generates Actions::SetHistory(Hour, timestamp)
2. App converts to AppRequest::SetHistory(Hour, timestamp)
3. RTIC task calls hour_history.find(storage, timestamp)
4. Result is stored in app.history_state.flow
5. Widget displays the value on the LCD
```

### 2. Browse by day

```
User:
1. Navigates to the Day History screen
2. Enters edit mode
3. Scrolls days: Left (back) / Right (forward)

System:
- On each key press:
  - timestamp += 86400 (or -= 86400)
  - Request SetHistory(Day, new_timestamp)
  - Display updates
```

### 3. View monthly statistics

```
User:
1. Navigates to the Month History screen
2. Selects month/year

System:
- Interval ~30 days (2592000 seconds)
- Lower resolution, longer retention period
```

## Writing Data

Data is written automatically in `app_request(AppRequest::Process)`:

```rust
AppRequest::Process => {
    // ... measurements ...

    // Every minute (when second < 5)
    if datetime.time().second() < 5 {
        let timestamp = datetime.as_utc().unix_timestamp();

        // Every hour (minute == 0)
        if datetime.time().minute() == 0 {
            hour_history.add(storage, hour_flow as i32, timestamp as u32);

            // Every day (hour == 0)
            if datetime.time().hour() == 0 {
                day_history.add(storage, day_flow as i32, timestamp as u32);

                // Every month (day == 1)
                if datetime.date().day() == 1 {
                    month_history.add(storage, month_flow as i32, timestamp as u32);
                }
            }
        }
    }
}
```

## Error Handling

### Possible Errors

```rust
pub enum Error {
    NoRecords,       // Buffer is empty
    Unitialized,     // System not initialized
    Storage,         // EEPROM error
    WrongCrc,        // Bad checksum
    Spi(spi::Error), // SPI error
}
```

### Handling in UI

```rust
match hour_history.find(storage, timestamp) {
    Ok(Some(flow)) => {
        // Data found
        app.history_state.flow = Some(flow as f32);
    }
    Ok(None) => {
        // No data for this period
        app.history_state.flow = None;
        // UI will display "None"
    }
    Err(e) => {
        // Read error
        defmt::error!("History read error: {:?}", e);
        app.history_state.flow = None;
    }
}
```

## Memory

### EEPROM Size Calculation

```rust
const SIZE_ON_FLASH: usize =
    size_of::<u32>()           // Header
    + SIZE * size_of::<i32>()  // Data (SIZE records × 4 bytes)
    + size_of::<ServiceData>() // Metadata (16 bytes)
    + size_of::<u16>();        // CRC (2 bytes)
```

### Examples

- **Hour History**: 4 + 2160×4 + 16 + 2 = **8662 bytes**
- **Day History**: 4 + 1116×4 + 16 + 2 = **4486 bytes**
- **Month History**: 4 + 120×4 + 16 + 2 = **502 bytes**

**Total**: ~13.7 KB out of 128 KB available EEPROM

## Ring Buffer Layout

### Structure

```
┌───────────────────────────────────────────┐
│ ServiceData (metadata)                    │
│  - size: 100                              │
│  - offset_of_last: 75                     │
│  - time_of_last: 1700000000               │
│  - crc: 0xABCD                            │
├───────────────────────────────────────────┤
│ Data[0]: 1234   ← oldest record           │
│ Data[1]: 2345                             │
│ ...                                       │
│ Data[75]: 9876  ← latest record           │
│ Data[76]: (empty)                         │
│ ...                                       │
│ Data[SIZE-1]: (empty)                     │
└───────────────────────────────────────────┘
```

### Operations

- **Write new data**: overwrite the oldest record
- **Read**: reverse scan from the last record
- **Overflow**: automatic index wraparound

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_existing_record() {
        let mut storage = MockStorage::new();
        let mut history = RingStorage::<0, 100, 3600>::new(&mut storage).unwrap();

        // Add test data
        history.add(&mut storage, 1234, 1700000000).ok();

        // Search for record
        let result = history.find(&mut storage, 1700000000);
        assert_eq!(result, Ok(Some(1234)));
    }

    #[test]
    fn test_find_nonexistent_record() {
        let mut storage = MockStorage::new();
        let mut history = RingStorage::<0, 100, 3600>::new(&mut storage).unwrap();

        // Search for non-existent record
        let result = history.find(&mut storage, 1700000000);
        assert_eq!(result, Ok(None));
    }
}
```

## Performance

- **EEPROM read**: ~1–2 ms per record
- **Buffer scan**: O(n), where n ≤ SIZE
- **Worst case**: 2160 operations for Hour History
- **Typical case**: a few iterations (recent data)

## Possible Optimizations

1. **Binary search** (requires sorted buffer)
2. **Caching** recent queries
3. **Date-range indexing**
4. **Delta encoding** for data compression

### Current Optimizations

- Timestamp normalized to minutes (reduces variants)
- Ring buffer (O(1) write)
- CRC protection for metadata
- Direct EEPROM access (no buffering)

## Interaction Diagram

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
           │ RTIC task queue
                                 v
                          ┌──────────────┐
                          │ app_request  │
                          │    task      │
                          └──────────────┘
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

## See Also

- [UI_ARCHITECTURE.md](UI_ARCHITECTURE.md) — overall UI architecture
- `src/history.rs` — RingStorage implementation
- `src/apps.rs` — App state and Actions
- `src/main.rs` — RTIC task app_request
