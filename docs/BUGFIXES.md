# Critical Business Logic Bug Fixes

## Date: 2025-11-23

### 1. ❏ CRITICAL: Reset Flow Accumulators After Saving

**File:** `src/main.rs:454-503`

**Problem:**
After saving hourly/daily/monthly data, the counters `hour_flow`, `day_flow`, `month_flow` were not reset. This caused values to accumulate and duplicate data to be stored in history.

**Example:**
- Hour 1: flow = 10 L → hour_flow = 10, saved 10
- Hour 2: flow = 15 L → hour_flow = 25 (10+15), saved 25 ❌
- Hour 3: flow = 20 L → hour_flow = 45 (10+15+20), saved 45 ❌

**Fix:**
Added accumulator reset after successful save:
```rust
if let Err(_e) = (hour_history, &mut storage).lock(|hour_history, storage| {
    hour_history.add(storage, hour_flow as i32, timestamp as u32)
}) {
    defmt::error!("Failed to log hour flow:");
} else {
    defmt::info!("Hour flow logged: {} at {}", hour_flow, timestamp);
    // ✅ Reset hour accumulator
    app.lock(|app| app.hour_flow = 0.0);
}
```

Same fix applied to `day_flow` and `month_flow`.

---

### 2. ❏ Incorrect Handling of Negative Time Delta

**File:** `src/history.rs:181-204`

**Problem:**
When writing data with a timestamp earlier than the last entry (clock rollback), the logic added `ELEMENT_SIZE` to the absolute value of delta:
```rust
delta = delta.abs() + ELEMENT_SIZE;  // ❌ Wrong
while delta != 0 {
    // ...
    delta -= ELEMENT_SIZE;
}
```

This could lead to:
- Extra zero values being written
- Incorrect buffer size reduction
- Data loss

**Fix:**
```rust
delta = delta.abs();  // ✅ Removed + ELEMENT_SIZE
while delta >= ELEMENT_SIZE {  // ✅ Changed condition
    // ...
}
```

---

### 3. ❏ u32 Underflow When Decrementing Offset

**File:** `src/history.rs:194-198`

**Problem:**
When offset = 0, the operation `offset - 1` caused u32 underflow:
```rust
let tmp = self.data.offset_of_last() - 1;  // ❌ Underflow when offset=0
self.data.set_offset_of_last(tmp);
if self.data.offset_of_last() > SIZE as u32 {
    self.data.set_offset_of_last(SIZE as u32);
}
```

The `> SIZE` check did not help because u32::MAX > SIZE.

**Fix:**
```rust
// ✅ Explicit zero check
if self.data.offset_of_last() == 0 {
    self.data.set_offset_of_last(SIZE as u32 - 1);
} else {
    let tmp = self.data.offset_of_last() - 1;
    self.data.set_offset_of_last(tmp);
}
```

---

### 4. ❏ Incorrect Gap Filling in History

**File:** `src/history.rs:164-175`

**Problem:**
When time intervals were skipped, the gap slots were filled with zeros using timestamp = 0:
```rust
while delta > ELEMENT_SIZE {
    self.write_value(storage, 0, 0)?;  // ❌ timestamp = 0!
    delta -= ELEMENT_SIZE;
    self.advance_offset_by_one();
}
```

This led to:
- Incorrect timestamps in storage
- Inability to determine the exact time of a gap
- Errors in time-based searches

**Fix:**
```rust
while delta > ELEMENT_SIZE {
    // ✅ Compute correct timestamp for the gap slot
    let gap_time = self.data.time_of_last() + ELEMENT_SIZE as u32;
    self.write_value(storage, 0, gap_time)?;
    self.write_service_data(storage)?;
    delta -= ELEMENT_SIZE;
}
self.write_value(storage, val, time)?;
self.write_service_data(storage)?;
return Ok(());
```

---

### 5. ❏ Incorrect Offset Comparison Condition

**File:** `src/history.rs:189`

**Problem:**
Comparing offset with size was logically incorrect:
```rust
if self.data.offset_of_last() == self.data.size() {  // ❌
```

- `offset_of_last()` — position in the ring buffer (0..SIZE-1)
- `size()` — number of elements (0..SIZE)

These have different ranges.

**Fix:**
```rust
if self.data.offset_of_last() == self.data.size() - 1 {  // ✅
```

---

## Testing

After applying fixes:
- ✅ Project compiles without errors
- ✅ Clippy has no warnings
- ✅ Binary size: 60612 bytes (Flash), 6852 bytes (RAM)

## Recommendations for Further Development

1. **Add unit tests** for the `history.rs` module:
   - Gap filling test
   - Negative time delta test
   - Ring buffer overflow test

2. **Add "already logged" flags** in the main loop:
   ```rust
   let mut last_hour_logged = 0;
   if datetime.time().minute() == 0 && last_hour_logged != datetime.time().hour() {
       // Log to history
       last_hour_logged = datetime.time().hour();
   }
   ```

3. **Add timestamp validation** before writing:
   - Check for reasonable range
   - Guard against large time jumps

4. **Debugging logs:**
   - Add debug logs when resetting accumulators
   - Log skipped time intervals
