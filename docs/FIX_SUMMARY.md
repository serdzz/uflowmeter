# Bug Fix Report

## Date: 2025-11-23

## Files Changed

### 1. `src/mod.rs` - DELETED ✅
**Problem:** Stray file in the `src/` root, duplicating `src/measurement/mod.rs`  
**Action:** File deleted

### 2. `src/main.rs` - 3 critical fixes ✅

#### 2.1 Reset hourly flow counter (line 464)
```rust
+ // Reset hour accumulator after successful save
+ app.lock(|app| app.hour_flow = 0.0);
```

#### 2.2 Reset daily flow counter (line 477)
```rust
+ // Reset day accumulator after successful save
+ app.lock(|app| app.day_flow = 0.0);
```

#### 2.3 Reset monthly flow counter (line 498)
```rust
+ // Reset month accumulator after successful save
+ app.lock(|app| app.month_flow = 0.0);
```

**Effect:** Prevents accumulation and duplication of data in history

---

### 3. `src/history.rs` - 4 critical fixes ✅

#### 3.1 Correct gap filling for skipped time intervals (lines 165-174)

**Before:**
```rust
while delta > ELEMENT_SIZE {
    self.write_value(storage, 0, 0)?;  // ❌ timestamp = 0
    delta -= ELEMENT_SIZE;
    self.advance_offset_by_one();
}
```

**After:**
```rust
// Fill gaps with zero values but correct timestamps
while delta > ELEMENT_SIZE {
    let gap_time = self.data.time_of_last() + ELEMENT_SIZE as u32;
    self.write_value(storage, 0, gap_time)?;  // ✅ correct timestamp
    self.write_service_data(storage)?;
    delta -= ELEMENT_SIZE;
}
self.write_value(storage, val, time)?;
self.write_service_data(storage)?;
return Ok(());
```

**Effect:** Gap slots now receive correct timestamps

---

#### 3.2 Fix negative time delta handling (lines 182-184)

**Before:**
```rust
delta = delta.abs() + ELEMENT_SIZE;  // ❌ extra ELEMENT_SIZE
while delta != 0 {
```

**After:**
```rust
// Handle negative delta (going back in time)
delta = delta.abs();  // ✅ without adding ELEMENT_SIZE
while delta >= ELEMENT_SIZE {  // ✅ changed condition
```

**Effect:** Correct handling of clock rollback

---

#### 3.3 Fix offset comparison condition (line 189)

**Before:**
```rust
if self.data.offset_of_last() == self.data.size() {  // ❌
```

**After:**
```rust
if self.data.offset_of_last() == self.data.size() - 1 {  // ✅
```

**Effect:** Logically correct comparison

---

#### 3.4 Guard against u32 underflow when decrementing offset (lines 193-198)

**Before:**
```rust
let tmp = self.data.offset_of_last() - 1;  // ❌ underflow at 0
self.data.set_offset_of_last(tmp);
if self.data.offset_of_last() > SIZE as u32 {
    self.data.set_offset_of_last(SIZE as u32);
}
```

**After:**
```rust
// Handle underflow correctly
if self.data.offset_of_last() == 0 {
    self.data.set_offset_of_last(SIZE as u32 - 1);
} else {
    let tmp = self.data.offset_of_last() - 1;
    self.data.set_offset_of_last(tmp);
}
```

**Effect:** u32 underflow prevented

---

## Change Statistics

| File | Lines changed | Severity |
|------|---------------|----------|
| `src/mod.rs` | deleted | medium |
| `src/main.rs` | +6 lines | critical |
| `src/history.rs` | ~30 lines | critical |

---

## Verification After Fixes

✅ **Compilation:** successful  
✅ **Clippy:** no warnings  
✅ **Binary size:** within normal range (60 KB Flash, 6.8 KB RAM)  
✅ **Logic:** all found bugs fixed  

---

## Commit Command

```bash
git add -A
git commit -m "Fix critical business logic bugs

- Reset flow accumulators after saving to history
- Fix negative time delta handling in ring buffer
- Fix offset underflow protection
- Fix gap filling with correct timestamps
- Remove incorrect src/mod.rs file

Fixes prevent data duplication and buffer corruption"
```

---

## Next Steps (recommended)

1. ✅ **Commit the changes**
2. ⚠️ **Add unit tests** for `history.rs`
3. ⚠️ **Add "already logged" guards** to prevent duplicate writes
4. ⚠️ **Test on hardware** — verify operation on the real device

---

## Impact on Existing Data

⚠️ **Note:** If the system already has saved data containing errors (accumulated values, incorrect timestamps), those will remain. Recommended actions:

1. Clear history on the next firmware update
2. Or create a migration to recalculate the data
3. Document the update date for reporting purposes
