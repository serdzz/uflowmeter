# Test Architecture for uflowmeter

## Overview

This project is an embedded STM32L1 application that needs to work both as:
1. **Binary (no_std embedded)**: Runs on embedded hardware with `thumbv7m-none-eabi` target
2. **Library (std)**: Provides testable modules for host platform

## Module Structure

### `src/main.rs`
- Binary entry point
- Uses embedded features (RTIC, HAL, etc.)
- Compiled only for `thumbv7m-none-eabi` target

### `src/lib.rs`
- Library root
- Re-exports testable modules
- Conditional compilation: `#![cfg_attr(not(test), no_std)]`
- Tests use host target, library code uses `no_std` when embedded

### `src/history.rs`
- Original embedded module with full HAL dependencies
- Not directly testable due to HAL requirements

### `src/history_lib.rs`
- Standalone testable version of history module
- No HAL dependencies
- Contains core logic for `RingStorage` and `ServiceData`
- Used by both library and embedded builds

### `src/history_lib_tests.rs`
- Unit tests for `history_lib` module
- Compiled only for host target (`#[cfg(test)]`)
- 11 comprehensive test cases

## Build Configuration

### `.cargo/config.toml`
```toml
[build]
target = "thumbv7m-none-eabi"  # Default for binary builds

[target.thumbv7m-none-eabi]
rustflags = ["-C", "link-arg=-Tlink.x", "-C", "link-arg=-Tdefmt.x"]
```

This default target is used for binary builds but **not** for library tests.

## Building and Testing

### Build Release Binary
```bash
cargo build --release
# Uses thumbv7m-none-eabi target, creates optimized embedded binary
```

### Run Tests
```bash
# Using Makefile (recommended)
make test

# Or manually temporarily disable embedded target
sed -i.bak '/^target = /d' .cargo/config.toml && \
cargo test --lib --release && \
mv .cargo/config.toml.bak .cargo/config.toml
```

The Makefile handles target switching automatically. When running tests, Cargo uses the host platform target instead of the embedded target, allowing tests to link against std library.

## Why This Approach?

1. **Embedded-first**: Binary uses no_std with minimal overhead
2. **Testable**: Core logic extracted to a no-std-compatible module that can be tested on host
3. **No duplication**: Tests reuse the same `history_lib` code that embedded build uses
4. **Flexible**: Easy to add more testable modules following same pattern

## Adding New Tests

1. Add test functions to `src/history_lib_tests.rs`
2. Keep `src/history_lib.rs` public methods that tests need
3. Run tests with the command above

## Limitations

- Tests cannot directly test embedded-specific code (RTIC, HAL)
- Only core logic can be tested this way
- For full integration tests, physical hardware or simulator needed
