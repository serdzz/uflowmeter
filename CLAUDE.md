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

## Known load-bearing bugs in STOP / wake / UI path

Three coupled bugs that **together** keep the device from crashing but break the button UI path. Touching any one in isolation surfaces the others — verified on hardware 2026-05-19.

1. **`exti9_5` ordering** (`src/main.rs:467-470`). The button-wake handler calls `power.active()` before `power.exit_sleep()`. `active()` clears `self.sleep`, so `exit_sleep()` sees `self.sleep == false` and **skips `rcc.reconfigure_after_stop()`** — clocks stay on the MSI fallback after wake instead of returning to PLL. SPI to the LCD then runs at the wrong rate and the UI silently fails to render new state ("buttons don't react"). RTC wake (`rtc_timer`, line 438) calls only `exit_sleep()` so it's unaffected.

2. **`rcc.cr.write()` clobber** (`src/hardware/power.rs:138`). Uses `.write()` instead of `.modify()` to enable HSI. `.write()` resets every unspecified field to its reset default, which turns **PLLON and HSEON off** right after `reconfigure_after_stop()` just turned them on. Hardware then auto-falls-back SYSCLK to MSI. Net effect today: PLL never actually stays restored after any wake — the system runs on MSI most of the time even when `get_sysclk_source()` reports PLL.

3. **RTIC timer queue corrupts across STOP / SysTick wake** (`cortex-m-rtic-1.1.4` + `systick-monotonic` + STM32L1 STOP). After several STOP cycles with a fully-clocked SysTick, the `SysTick` handler's `tq::dequeue` call panics with a BusFault — peek() returns a `Some(&NotReady)` whose backing pointer is into unmapped memory (`0x0012xxxx`). This is the bug `src/main.rs:635-639` documents and works around for Path A (rtc_timer → Process → inline `prepare_sleep`, no `spawn_after`). Path B (button → TIM2 → `spawn_after(IDLE_TIMEOUT, DeepSleep)`) still leaves an entry in the queue, plus `modbus_poll` self-reschedules via `spawn_after`. The crash reproduces even with both queues empty if SysTick is fast enough.

**Why nothing crashes today**: bugs #1 and #2 together keep SYSCLK on MSI (~2 MHz) after any wake, so SysTick fires ~12× slower than configured. That's slow enough that the queue corruption in #3 never accumulates to a crash within a typical session. Fix #1 alone or #2 alone exposes #3 → first or second wake HardFaults.

**To actually fix the UI**, #3 must be addressed first. Options (none cheap):
- Switch the monotonic backend off SysTick (e.g. RTC-based monotonic that keeps ticking through STOP).
- Quiesce SysTick + drain `RTIC_TQ` before STOP entry, reinit after wake.
- Update RTIC past 1.1.4 if newer releases fix this.

Until then, the file `src/main.rs:467` ordering and the `write()` at `src/hardware/power.rs:138` are **load-bearing** — do not "fix" them in isolation. The `// gpio_power.up() skipped` style stale comments around `exit_sleep` already led to one bad commit (`4c4776b`, reverted by `e70d707`).

### Embassy port (branch `rework/embassy`, 2026-05-21)

Sidestepping all three bugs by replacing the runtime entirely. Branch
state at commit `1c07268`:

- Cargo.toml: embassy-{executor 0.10, stm32 0.6, time 0.5, sync 0.8,
  futures, embedded-hal} pinned to `serdzz/embassy` via
  `[patch.crates-io]`. cortex-m-rtic, stm32l1xx-hal, shared-bus-rtic
  dropped.
- `src/main.rs.rtic-backup`: verbatim copy of the prior RTIC main.
- `src/drivers/hd44780.rs`: async 4-bit parallel HD44780 driver using
  `embassy_stm32::gpio::Output` + `embassy_time::Timer`. Verified on
  hardware — text renders on the LCD.
- `src/drivers/keypad.rs`: 20 Hz async polling task for PB6..PB9,
  emits `KeyEvent::Pressed` into a static channel with the legacy
  1 s / 150 ms repeat timing. Verified — all four buttons detected
  and reflected on the LCD.
- `src/lib.rs`: trimmed to the pure-Rust modules (apps, calibration,
  gui, history_lib, ui). The legacy `hardware/*`, `measurement/*`,
  `history.rs`, `mbus.rs`, `modbus*.rs`, `options.rs`, `shell.rs` are
  gated off — they still use embedded-hal 0.2 + the old HAL.

**Still TODO on the embassy branch:**

- RTC datetime + backup registers (embassy-stm32 exposes this — wire
  up `Rtc::new`, port the `app.last_uptime_rtc` backup-register dance).
- STOP mode. Embassy's `low-power` feature is gated on L4/L5/U5/U3/
  WB/WL/U0 — **not L1**. Two real options:
    1. Add `stm32l1` to the cfg gates in `embassy-stm32/src/low_power.rs`
       + write an L1 RTC-based LPTimeDriver. Hours of work, uncertain.
    2. Skip the embassy executor integration and call `pwr.stop_mode()`
       manually via the `embassy_stm32::pac::pwr` crate inside an idle
       watchdog task. embassy_time stops ticking during STOP (TIM3 is
       gated), which matches the prior SysTick behavior — readers
       already handle it. Cheap.
- Port EEPROM (25LC1024) — shared SPI2 between EEPROM / TDC1000 /
  TDC7200. Use `embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice`.
- TDC1000 + TDC7200 drivers. Will need to be rewritten on
  embedded-hal 1.0 (the old ones used `blocking::spi::Transfer/Write`
  + `digital::v2::OutputPin`).
- USART1 for Modbus RTU + shell.
- Wire the existing `ui.rs` event loop on top of the new `KEYS` channel.

### Failed fix attempt (2026-05-19, do not retry as-is)

Tried this combination expecting it to close #3 without the deeper refactor:

- Fix #1: swap `power.exit_sleep()` before `power.active()` in `exti9_5`.
- Fix #2: replace `self.rcc.cr.write(...)` with `self.rcc.cr.modify(...)`.
- Mask SysTick `CTRL.TICKINT` in `enter_stop_mode`, clear `SCB.ICSR.PENDSTCLR` and re-enable `TICKINT` at the end of `exit_sleep` (after `reconfigure_after_stop`). Wrapped the unsafe MMIO in two helpers `mask_systick()` / `unmask_systick_clear_pending()` in `src/hardware/nvic.rs` to stay inside `#![deny(unsafe_code)]`.
- Removed the `app_request::spawn_after(IDLE_TIMEOUT, DeepSleep)` / `handle.cancel()` churn from the TIM2 timer task; replaced with `app_request::spawn(AppRequest::DeepSleep)` on the `is_active() → false` transition. Removed the `handle: Option<...>` local field too.

Result on hardware: clean boot, RTC wake works, but **first or second button press still HardFaults** with the same `Escalated BusFault → tq::dequeue → SysTick` stack trace at `0x00124xxx`. SysTick masking only narrows the immediate-wake window — corruption is already in the queue's RAM by the time `TICKINT` is re-enabled. Removing TIM2's `spawn_after` doesn't help either, because the only remaining producer (`modbus_poll`) wasn't even firing during the test — the queue should have been empty, yet `peek()` returned `Some(&garbage)`.

So masking + queue hygiene around `Power` is **not** the right layer. The corruption either happens (a) inside `monotonic::now()` returning a value that makes `tq.set_compare()` confuse internal state, or (b) inside `cortex-m-rtic 1.1.4`'s timer-queue implementation itself across STOP. Fixing it requires touching RTIC internals or replacing the monotonic.

All four changes above were reverted; tree returned to `e70d707` parity.
