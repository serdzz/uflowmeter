# uFlowmeter Project Testing

## Project Structure

The project consists of two crates:

1. **Library crate** (`src/lib.rs`):
   - `history_lib` — ring buffer history library
   - `hardware` — hardware drivers (embedded only)
   - Tests: `history_lib_tests.rs`, `history_tests.rs`, `tests.rs`, `ui_history_tests.rs`, `ui_logic_tests.rs`

2. **Binary crate** (`src/main.rs`):
   - `ui` — user interface (Viewport, LabelScreen, LabelsWidget)
   - `apps` — application logic (App, Actions, AppRequest)
   - `gui` — GUI components
   - `hardware`, `history`, `options` — embedded modules
   - Requires embedded dependencies (`hal`, RTIC, etc.)

## Running Tests

### All Tests (library crate)

```bash
make test
```

or manually:

```bash
sed -i.bak '/^target = /d' .cargo/config.toml && \
cargo test --lib --release && \
mv .cargo/config.toml.bak .cargo/config.toml
```

The Makefile temporarily removes the embedded target from `.cargo/config.toml`, runs tests on the host platform, then restores the config.

### Specific Test Modules

```bash
cargo test --lib history_lib_tests
cargo test --lib history_tests
cargo test --lib tests
cargo test --lib ui_history_tests
cargo test --lib ui_logic_tests
```

## Test Suite Overview (192 tests)

Runs tests from `src/history_lib_tests.rs`, `src/history_tests.rs`, `src/tests.rs`, `src/ui_history_tests.rs`, and `src/ui_logic_tests.rs`:

**History lib tests (11):**
- ✅ `test_advance_offset_wrapping`
- ✅ `test_first_stored_timestamp_empty`
- ✅ `test_first_stored_timestamp_with_data`
- ✅ `test_multiple_advances`
- ✅ `test_offset_calculation`
- ✅ `test_last_stored_timestamp`
- ✅ `test_service_data_bytes_conversion`
- ✅ `test_service_data_creation_with_values`
- ✅ `test_service_data_default`
- ✅ `test_size_increment`
- ✅ `test_timestamp_normalization`

**UI logic tests (11):**
- ✅ `test_blink_masks_correct`
- ✅ `test_timestamp_full_value`
- ✅ `test_hour_increment_timestamp`
- ✅ `test_day_increment_timestamp`
- ✅ `test_different_dates_different_timestamps`
- ✅ `test_minute_increment_timestamp`
- ✅ `test_second_increment_timestamp`
- ✅ `test_timestamp_monotonic`
- ✅ `test_bitmask_positioning`
- ✅ `test_time_masks_complete`
- ✅ `test_date_decrement_timestamp`

## Why Tests Run on the Host Platform

The Makefile removes the embedded target from `.cargo/config.toml` before running tests. Without the target specification, Cargo compiles for the host (x86_64 / aarch64). This works because:

1. `no_std` is gated by `#![cfg_attr(not(test), no_std)]` — tests compile with `std`
2. Embedded-specific modules (`hardware`, `stm32l1xx_hal`) are guarded by `#[cfg(not(test))]`
3. Core logic (GUI, history, UI widgets) has no HAL dependencies

## Build and Code Checks

### Compilation Check
```bash
cargo check --release
```

### Release Build
```bash
cargo build --release
```

### Clippy
```bash
make clippy
```

### Binary Size
```bash
arm-none-eabi-size target/thumbv7m-none-eabi/release/uflowmeter
```

## Summary

- ✅ **192 tests** — all passing
- ✅ **Embedded build** — compiles without errors
- ✅ **Clippy** — no warnings
- ✅ **No hardware required** — all tests use mocks
