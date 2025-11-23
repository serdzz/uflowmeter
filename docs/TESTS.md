# Tests for history.rs Module

## Overview

Comprehensive unit tests have been added to `src/history_lib.rs` to test the `RingStorage` and `ServiceData` data structures used for managing circular buffer history of measurements.

## Test Structure

Tests are located in the `tests` module within `src/history_lib.rs` and are conditionally compiled with `#[cfg(test)]`.

## Test Cases

### ServiceData Tests

1. **test_service_data_default** - Verifies that `ServiceData` initializes with default values (0)
2. **test_service_data_creation_with_values** - Tests setting and getting individual fields (size, offset, time)
3. **test_service_data_bytes_conversion** - Validates serialization and deserialization to/from bytes

### RingStorage Structure Tests

4. **test_advance_offset_wrapping** - Verifies circular buffer offset wrapping behavior:
   - Tests normal increment
   - Tests wraparound at SIZE boundary

5. **test_size_increment** - Validates size field incrementing

6. **test_timestamp_normalization** - Confirms timestamps are normalized to 60-second intervals

7. **test_first_stored_timestamp_empty** - Tests first timestamp calculation when empty
8. **test_first_stored_timestamp_with_data** - Tests first timestamp with data (last_time - size * ELEMENT_SIZE)

9. **test_last_stored_timestamp** - Verifies last stored timestamp retrieval

10. **test_multiple_advances** - Tests multiple sequential advances through buffer positions

11. **test_offset_calculation** - Verifies offset calculation between indices (4-byte steps for i32)

## Running Tests

Use the Makefile to run tests (it temporarily disables the embedded target):

```bash
make test
```

Or run manually:

```bash
sed -i.bak '/^target = /d' .cargo/config.toml && \
cargo test --lib --release && \
mv .cargo/config.toml.bak .cargo/config.toml
```

## Building the Embedded Binary

To build the embedded binary for STM32L151, use the Makefile:

```bash
make build
```

Or build directly with default target (thumbv7m-none-eabi):

```bash
cargo build --release
```

All 11 tests should pass:

```
running 11 tests
test history_lib_tests::test_advance_offset_wrapping ... ok
test history_lib_tests::test_first_stored_timestamp_empty ... ok
test history_lib_tests::test_first_stored_timestamp_with_data ... ok
test history_lib_tests::test_last_stored_timestamp ... ok
test history_lib_tests::test_multiple_advances ... ok
test history_lib_tests::test_offset_calculation ... ok
test history_lib_tests::test_service_data_bytes_conversion ... ok
test history_lib_tests::test_service_data_creation_with_values ... ok
test history_lib_tests::test_service_data_default ... ok
test history_lib_tests::test_size_increment ... ok
test history_lib_tests::test_timestamp_normalization ... ok

test result: ok. 11 passed; 0 failed; 0 ignored
```

## Test Coverage

The tests cover:
- ✓ ServiceData field manipulation
- ✓ Circular buffer index wrapping
- ✓ Timestamp calculations
- ✓ Offset calculations
- ✓ Ring size management

## Note on Implementation

Tests are written for the no_std library module `history_lib` which is a simplified, testable version of the embedded `history` module. The `history_lib` module provides the core data structures without dependencies on embedded HAL, making it suitable for testing on the host platform.
