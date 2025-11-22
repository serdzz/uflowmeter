# UI Module Examples

This document provides examples demonstrating the UI system components.

## Overview

The UI module (`src/ui.rs`) implements:
- **History Widgets**: Hour/Day/Month history navigation
- **DateTime Editing**: Interactive date/time editing with visual feedback
- **Blink Masks**: Control which characters blink during editing
- **Timestamp Calculation**: Unix timestamp for history queries

## Key Concepts

### HistoryWidgetTrait

The trait that implements datetime navigation and editing:

```rust
pub trait HistoryWidgetTrait {
    fn get_datetime(&self) -> PrimitiveDateTime;
    fn set_datetime(&mut self, datetime: PrimitiveDateTime);
    fn get_items(&self) -> DateTimeItems;
    fn set_items(&mut self, items: DateTimeItems);
    fn get_timestamp(&self) -> u32;
    fn set_timestamp(&mut self, timestamp: u32);
    
    fn inc(&mut self);  // Increment current field
    fn dec(&mut self);  // Decrement current field
    fn next_item(&mut self) -> bool;  // Navigate to next field
}
```

### Blink Masks

Blink masks control which characters flash during editing.

#### Time Format: `"HH:MM:SS"` (8 characters)

```
Position:  0  1  2  3  4  5  6  7
Display:   H  H  :  M  M  :  S  S
Bit:       7  6  5  4  3  2  1  0
```

| Field   | Positions | Bits | Mask | Value  |
|---------|-----------|------|------|--------|
| Seconds | 6-7       | 0-1  | SS   | `0x03` |
| Minutes | 3-4       | 3-4  | MM   | `0x18` |
| Hours   | 0-1       | 6-7  | HH   | `0xc0` |

#### Date Format: `"DD/MM/YY"` (8 characters)

```
Position:  0  1  2  3  4  5  6  7
Display:   D  D  /  M  M  /  Y  Y
Bit:       7  6  5  4  3  2  1  0
```

| Field | Positions | Bits | Mask | Value  |
|-------|-----------|------|------|--------|
| Day   | 0-1       | 6-7  | DD   | `0xc0` |
| Month | 3-4       | 3-4  | MM   | `0x18` |
| Year  | 6-7       | 0-1  | YY   | `0x03` |

## Example 1: Timestamp Calculation

Timestamps are calculated as full Unix timestamps (seconds since 1970-01-01).

```rust
use time::{macros::datetime, Duration};

// Starting point
let dt = datetime!(2024-01-15 10:30:00 UTC);
let ts1 = dt.unix_timestamp() as u32;
// ts1 = 1705318200

// Hour history: add 1 hour
let dt_hour = dt.saturating_add(Duration::HOUR);
let ts2 = dt_hour.unix_timestamp() as u32;
assert_eq!(ts2 - ts1, 3600);  // 1 hour = 3600 seconds

// Day history: add 1 day
let dt_day = dt.saturating_add(Duration::DAY);
let ts3 = dt_day.unix_timestamp() as u32;
assert_eq!(ts3 - ts1, 86400);  // 1 day = 86400 seconds
```

**Important:** The timestamp uses the full Unix value, NOT `% 60` or any other modulo operation.

## Example 2: Navigation Flow

The editing flow through different fields:

```
State Machine:
    
    None ──────> Seconds ──────> Minutes ──────> Hours
     ↑            (0x03)           (0x18)         (0xc0)
     │               │                │              │
     │               │                │              v
     │               │                │            Day ──────> Month ──────> Year
     │               │                │           (0xc0)        (0x18)       (0x03)
     │               │                │              │            │            │
     │               │                │              v            v            v
     └───────────────┴────────────────┴──────────────┴────────────┴────────────┘
                                    (Press Enter on Year)
```

### User Interaction Example

```
Initial state: 2024-01-15 10:30:45
Display: "15/01/24    10:30:45"
Items: None (not editing)

[User presses Enter]
→ Items: Seconds
→ Time edit enabled
→ Blink mask: 0x03 (SS blinks)
→ Display: "15/01/24    10:30:  "  (45 blinks)

[User presses Right]
→ DateTime: 2024-01-15 10:30:46  (incremented)
→ Timestamp: 1705318246

[User presses Enter]
→ Items: Minutes
→ Blink mask: 0x18 (MM blinks)
→ Display: "15/01/24    10:  :46"  (30 blinks)

[User presses Right]
→ DateTime: 2024-01-15 10:31:46  (incremented)
→ Timestamp: 1705318306

[Continue through Hours → Day → Month → Year → None]
```

## Example 3: History Widget Implementation

Simplified pseudo-code showing widget usage:

```rust
// Create history widget
let mut widget = HistoryWidget::new();
widget.set_datetime(datetime!(2024-01-15 10:00:00 UTC));

// User starts editing
widget.next_item();  // None → Seconds
// Internally:
// - get_time_edit().blink_mask(0x03)
// - get_time_edit().set_editable(true)
// - set_items(DateTimeItems::Seconds)

// User increments
widget.inc();  // 10:00:00 → 10:00:01
let ts = widget.get_timestamp();
// ts = Unix timestamp for 2024-01-15 10:00:01 UTC

// Query history with this timestamp
// Actions::SetHistory(HistoryType::Hour, ts)
```

## Example 4: Month Navigation

Month navigation uses `Month::next()` and `Month::previous()` with wrapping:

```rust
use time::Month;

let mut month = Month::January;
month = month.next();  // → February

month = Month::December;
month = month.next();  // → January (wraps)

month = Month::January;
month = month.previous();  // → December (wraps)
```

## Example 5: History Types

Different history types use different time intervals:

### Hour History
- Interval: 3600 seconds (1 hour)
- Navigation: Hours
- Storage: Last 2160 hours (~90 days)

```rust
let base = datetime!(2024-01-15 12:00:00 UTC);
for i in 0..24 {
    let dt = base.saturating_add(Duration::hours(i));
    let ts = dt.unix_timestamp() as u32;
    // Query: get_history(HistoryType::Hour, ts)
}
```

### Day History
- Interval: 86400 seconds (1 day)
- Navigation: Days
- Storage: Last 1116 days (~3 years)

```rust
let base = datetime!(2024-01-15 00:00:00 UTC);
for i in 0..30 {
    let dt = base.saturating_add(Duration::days(i));
    let ts = dt.unix_timestamp() as u32;
    // Query: get_history(HistoryType::Day, ts)
}
```

### Month History
- Interval: ~30 days (1 month)
- Navigation: Months
- Storage: Last 120 months (10 years)

```rust
let mut dt = datetime!(2024-01-15 00:00:00 UTC);
for i in 0..12 {
    // Use month arithmetic, not fixed intervals
    dt = dt.replace_month(dt.month().next()).unwrap();
    let ts = dt.unix_timestamp() as u32;
    // Query: get_history(HistoryType::Month, ts)
}
```

## Example 6: Blink Mask Verification

How to verify blink masks are correct:

```rust
// Time masks
const SECONDS: u32 = 0x03;  // 0b00000011
const MINUTES: u32 = 0x18;  // 0b00011000
const HOURS: u32 = 0xc0;    // 0b11000000

// Verify no overlap
assert_eq!(SECONDS & MINUTES, 0);
assert_eq!(SECONDS & HOURS, 0);
assert_eq!(MINUTES & HOURS, 0);

// Verify all digits covered
let all = SECONDS | MINUTES | HOURS;
assert_eq!(all, 0xdb);  // 0b11011011 (bits 0,1,3,4,6,7)
// Bits 2 and 5 are colons (:) - not digits
```

## Complete Flow Diagram

```
User Action         Widget State           Display
───────────────────────────────────────────────────────
[View History]      editable=false         15/01/24  10:30:45
                    items=None             From: 123.4L
                    
[Press Enter]       editable=true          15/01/24  10:30:__
                    items=Seconds          From: 123.4L
                    blink_mask=0x03
                    
[Press Right]       datetime += 1s         15/01/24  10:30:46
                    timestamp updated
                    → Query new data
                    
[Press Enter]       items=Minutes          15/01/24  10:__:46
                    blink_mask=0x18
                    
[Press Right]       datetime += 1min       15/01/24  10:31:46
                    
[Press Enter]       items=Hours            15/01/24  __:31:46
                    blink_mask=0xc0
                    
[Press Right]       datetime += 1hour      15/01/24  11:31:46
                    
[Press Enter]       items=Day              __/01/24  11:31:46
                    time_edit disabled
                    date_edit enabled
                    blink_mask=0xc0
                    
[Press Right]       datetime += 1day       16/01/24  11:31:46
                    
[Press Enter]       items=Month            16/__/24  11:31:46
                    blink_mask=0x18
                    
[Press Right]       month = month.next()   16/02/24  11:31:46
                    
[Press Enter]       items=Year             16/02/__  11:31:46
                    blink_mask=0x03
                    
[Press Right]       year += 1              16/02/25  11:31:46
                    
[Press Enter]       items=None             16/02/25  11:31:46
                    editable=false         From: 789.0L
                    all edits disabled
```

## Testing

See `src/ui_logic_tests.rs` for runnable tests that verify:
- Blink mask values
- Timestamp calculations
- Navigation state machine
- Time increment/decrement logic

Run tests with:
```bash
make test
```

## See Also

- `src/ui.rs` - Full implementation
- `src/main.rs` - Integration with application
- `src/ui_logic_tests.rs` - Unit tests
- `docs/TIMESTAMP_FIX.md` - Timestamp calculation details
