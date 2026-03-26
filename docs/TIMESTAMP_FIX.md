# Timestamp Calculation Fix in HistoryWidget

## Problem

The `inc()` and `dec()` methods of `HistoryWidget` used incorrect timestamp calculation:

```rust
self.set_timestamp(self.get_datetime().assume_utc().unix_timestamp() as u32 % 60);
```

### Why this was wrong

The `% 60` operator (remainder after dividing by 60) produced values only in the range 0–59:

1. **For Hour History**: Only seconds (0–59) instead of the full hour timestamp ❌
2. **For Day History**: All day information was lost ❌
3. **For Month History**: All month information was lost ❌

## Fix

Removed `% 60` from the calculation:

```rust
self.timestamp = self.datetime.assume_utc().unix_timestamp() as u32;
```

`timestamp` now contains the full Unix timestamp (seconds since 1970-01-01), correct for all history types.

## Blink Mask Verification

The `blink_mask` values in `next_item()` were also verified:

### Time format: `"HH:MM:SS"` (8 characters)

| Component | Positions | Mask bits | Mask   | Status    |
|-----------|-----------|-----------|--------|-----------|
| Seconds   | 6–7       | 0–1       | `0x03` | ✓ Correct |
| Minutes   | 3–4       | 3–4       | `0x18` | ✓ Correct |
| Hours     | 0–1       | 6–7       | `0xc0` | ✓ Correct |

### Date format: `"DD/MM/YY"` (8 characters)

| Component | Positions | Mask bits | Mask   | Status    |
|-----------|-----------|-----------|--------|-----------|
| Day       | 0–1       | 6–7       | `0xc0` | ✓ Correct |
| Month     | 3–4       | 3–4       | `0x18` | ✓ Correct |
| Year      | 6–7       | 0–1       | `0x03` | ✓ Correct |

### Mask logic

In `src/gui/edit.rs`:
```rust
if self.blink_mask.get_bit(LEN - i - 1)
```

For `LEN=8`:
- Character at position 0 → bit 7
- Character at position 1 → bit 6
- Character at position 6 → bit 1
- Character at position 7 → bit 0

**Conclusion**: All `blink_mask` values are implemented correctly. ✓

## Results

- ✅ Code compiles without errors
- ✅ Clippy passes without warnings
- ✅ Binary size: 60996 bytes
- ✅ All blink masks are correct

## Files Changed

- `src/ui.rs` (lines 73 and 100): Removed `% 60` from timestamp calculation
- `src/ui.rs`: Added unit tests for verification

## Tests

Unit tests added in `src/ui.rs`:

- `test_blink_masks_correct()` — verifies blink mask values
- `test_timestamp_full_value()` — verifies full Unix timestamp
- `test_hour_increment_timestamp()` — hour increment should be +3600
- `test_day_increment_timestamp()` — day increment should be +86400
