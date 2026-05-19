# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & test commands

This repo has a **dual-target setup**: the embedded binary builds for `thumbv7m-none-eabi`, but library tests run on the host. `.cargo/config.toml` pins the embedded target by default, so anything host-side requires temporarily stripping that line.

| Task | Command |
|------|---------|
| Build embedded binary (release) | `cargo build --release` |
| Run all host tests | `make test` |
| Run a single test | `bash run_host.sh test <test_name>` |
| Modbus tests only (single-threaded) | `make test-modbus` |
| Clippy (host, `-D warnings`) | `make clippy` |
| Run UI examples on host | `make ui-examples` |
| Format check | `cargo fmt -- --check` |
| Flash via probe-rs | `cargo embed --release` (chip configured in `.embed.toml`) |

**Do not run `cargo test` directly** — without removing the embedded target it will try to link `std` against `thumbv7m-none-eabi` and fail. Use `make test` or `bash run_host.sh test ...`; the script removes the `target = ...` line and restores it via a `trap` even on failure.

## Dual-target architecture (load-bearing)

- `src/main.rs` — `no_main`/`no_std` RTIC binary; full HAL and hardware access.
- `src/lib.rs` — testable surface, gated by `#![cfg_attr(not(test), no_std)]`.
- Embedded-only modules (`hardware`, `history`, `mbus`, `modbus`, `modbus_handler`, `options`, `shell`) sit inside a `#[cfg(not(test))]` block in `lib.rs` so they vanish during host builds.
- **Pattern for new testable logic**: split into `*_lib.rs` (no-std core, no HAL) and a thin embedded wrapper. Compare `history_lib.rs` (testable) vs `history.rs` (HAL-bound).
- Tests live next to source as `src/*_tests.rs`, gated `#[cfg(test)]` in `lib.rs`.

## Runtime architecture

- **RTIC app** (`src/main.rs`, `#[app(device = hal::stm32, ...)]`): tasks include `rtc_timer`, `timer`, `ui_timer`, `app_request`, `usart1_irq`, `shell_cmd`, `modbus_poll`, `tdc7200_irq`, `tdc7200_result`, `idle`. `Shared` carries the LCD, three history ring buffers, EEPROM `storage`, `app`, `ui`, both TDC drivers, modbus state, and `options`.
- **UI** (`src/ui.rs` + `src/gui/`): a `Screen` enum + `MenuController` holding 4 `MenuList` ring buffers. No `dyn` or heap dispatch — modeled after the C++ UsFlowMeter `UI::List`. Widgets implement `Widget<S, A>`; compose them with the `widget_group!` macro.
- **History** (`history_lib.rs`): three `RingStorage<OFFSET, COUNT, INTERVAL>` const-generic ring buffers (Hour / Day / Month) backed by 25LC1024 EEPROM. Layout offsets are chained at compile time via `HourHistory::SIZE_ON_FLASH` etc.
- **SharedBus**: SPI2 is shared between TDC1000, TDC7200, and EEPROM via `shared_bus_rtic::SharedBus`.

### SharedBus pitfall (read before touching TDC code)

TDC driver methods carry an `E: From<PinError>` bound that is **not** satisfied when SPI is wrapped as a `BusProxy`. Call concrete TDC methods only from `main.rs` where types are monomorphized — never inside a generic helper function (the bound will fail). Use `Tdc7200::new_simple` to construct without those bounds. See `docs/ARCHITECTURE.md` → "SharedBus Patterns" for the full explanation.

## Project conventions

- **Patched HAL**: `stm32l1xx-hal` is overridden via `[patch.crates-io]` in `Cargo.toml` to point at the `serdzz/stm32l1xx-hal` fork. Do not assume upstream API matches.
- **Logging**: `defmt` + `defmt-rtt`. `DEFMT_LOG=trace` is set in `.cargo/config.toml`.
- **Allocator**: `emballoc::Allocator<4096>` is declared in *both* `lib.rs` (under `#[cfg(not(test))]`) and `main.rs`. Stack is 8 KB (`memory.x`, `STACK_SIZE = 8192`).
- **Strict lints**: `main.rs` uses `#![deny(unsafe_code)]` and `#![deny(warnings)]`; CI clippy runs with `-D warnings`. New code must pass both.
- **Cargo features**: `low_power`, `noswd` (both empty by default).
- **CI** (`.github/workflows/ci.yml`): four parallel jobs — embedded build, host tests, clippy, `cargo fmt --check`.

## Where to look for deeper context

- `docs/ARCHITECTURE.md` — system overview, dual-target rationale, SharedBus pitfalls
- `docs/UI_ARCHITECTURE.md` — `Widget` trait, `Screen`/`MenuController`, `widget_group!`
- `docs/HISTORY_SYSTEM.md` — `RingStorage` layout and retention table
- `docs/MODBUS_MAP.md` — Modbus register map
- `docs/TDC1000_REGISTER_MAP.md`, `docs/TDC7200_REGISTER_MAP.md` — chip register details
- `STM32L151.svd` — peripheral definitions for the MCU
