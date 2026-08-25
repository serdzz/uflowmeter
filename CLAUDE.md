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
| Clippy (all four crates, `-D warnings`) | `make clippy` |
| Run UI examples on host | `make ui-examples` |
| Format check | `cargo fmt --all -- --check` |
| Build the bootloader | `UFW_AES_KEY=$(cat ufw.key) make bootloader` |
| Flash both binaries | `make flash` |
| Flash the application only | `make flash-app` |
| Pack an update image | `UFW_AES_KEY=$(cat ufw.key) make image` |

**Two binaries now.** The application links at `0x08004000`, so a board
flashed with only `uflowmeter` will sit dead until the bootloader is
flashed too — `make flash` does both, at `--speed 500`. `cargo run
--release` is no longer a way to flash: it writes the application and
leaves the reset vector as it found it. See `docs/BOOTLOADER.md`.

**The toolchain is pinned** in `rust-toolchain.toml` (currently 1.98.0),
along with the components and targets the build needs, so a fresh clone
builds and lints without extra setup and CI compiles with the same rustc
you do. That pin exists because it was learned the hard way: a lint
present in 1.98 but not in 1.94 passed every local check and then failed
four CI jobs at once. Bumping it is deliberate — change the channel, run
`make clippy` and `make test`, and expect new lints.

**Flash at `--speed 500`.** At the default SWD rate this board fails to
connect, reproducibly and in several different ways depending on what the
firmware is doing at the time: `SwdDpWait`, `SwdDpError`,
`JtagGetIdcodeError`, or a connect that succeeds and then dies partway
through the write with "An error with the flashing procedure has
occurred". At 500 kHz it works first time. `cargo run --release` uses the
runner in `.cargo/config.toml`, which does **not** pass the speed — so
for anything beyond a quick attempt, invoke `probe-rs run` directly.

If it still will not connect, hold the board's RESET button down, start
the flash, and release RESET a second or two later. That has been the
reliable fallback: on the occasions where a power-cycle left two
successive attempts failing with `SwdDpWait`, holding RESET worked
first time (though the write itself then takes ~26 s rather than the
usual 6-15). The likely reason is that the firmware reaches STOP before
the probe can attach — `min_stop_pause` is 100 ms and the executor
sleeps between the 5 s measurement cycles, so the window after a reset
is only milliseconds wide.

`--connect-under-reset` is not the answer here: NRST is not wired
through to the probe, which is why that path returns
`JtagGetIdcodeError` instead of working.

**Do not run `cargo test` directly** — without removing the embedded target it will try to link `std` against `thumbv7m-none-eabi` and fail. Use `make test` or `bash run_host.sh test ...`; the script removes the `target = ...` line and restores it via a `trap` even on failure.

## Dual-target architecture (load-bearing)

- `src/main.rs` — `no_main`/`no_std` RTIC binary; full HAL and hardware access.
- `src/lib.rs` — testable surface, gated by `#![cfg_attr(not(test), no_std)]`.
- `lib.rs` declares only what builds for both targets. The RTIC-era glue (`src/hardware/`, `src/measurement/`, `src/main.rs.rtic-backup`) is excluded simply by not being declared there — it still imports old `stm32l1xx-hal` types and does not compile. Treat those directories as an archive, not as live code.
- **Pattern for new testable logic**: split into `*_lib.rs` (no-std core, no HAL) and a thin embedded wrapper. Compare `history_lib.rs` (testable) vs `history.rs` (HAL-bound).
- Tests live next to source as `src/*_tests.rs`, gated `#[cfg(test)]` in `lib.rs`.

## Runtime architecture

- **embassy app** (`src/main.rs`): five spawned tasks — `keypad_task`, `measurement_task`, `uart_session_task`, `shell_task`, `history_tick_task`. Everything else runs in `main` itself, which owns the LCD, the three history ring buffers, the EEPROM, `app`, `ui` and `options` outright: the executor is single-threaded, so there are no shared-resource locks anywhere.
- **Main loop**: a `select6` over the keypad channel, framed Modbus frames, the 60 s history tick, the measurement result signal, shell actions, and an idle timer whose deadline depends on UI state (blink frame while editing, slower refresh when a screen is visible, `pending()` when the panel is dark).
- **UI** (`src/ui.rs` + `src/gui/`): a `Screen` enum + `MenuController` holding 4 `MenuList` ring buffers. No `dyn` or heap dispatch — modeled after the C++ UsFlowMeter `UI::List`. The widget framework in `src/gui/` is separate and **not used by the shipped UI**: `ui.rs` takes only `CharacterDisplay`, `HistoryType` and `UiEvent` from it, and `widget_group!` / `widget_mux!` are invoked nowhere.
- **History** (`history_lib.rs`): three `RingStorage<OFFSET, COUNT, INTERVAL>` const-generic ring buffers (Hour / Day / Month) backed by 25LC1024 EEPROM. Layout offsets are chained at compile time via `HourHistory::SIZE_ON_FLASH` etc.
- **SPI2** is shared between TDC1000, TDC7200 and the EEPROM. `shared_bus_rtic` is gone with RTIC; the drivers take the bus directly from `main`.

### TDC bring-up (read before touching TDC code)

The five bugs that stood between the embassy port and a working TDC pair
are described in `docs/DETAILED.md` §2.2. The one worth carrying in your
head: **TDC1000 RESET is active-high and idles LOW.** Leaving it high
holds the chip in reset for its whole life, and the symptom is that it
answers `0x00` to every register read while the TDC7200 — which has no
reset line — answers normally.

Also: the legacy driver in `src/hardware/tdc7200.rs` has read and write
inverted relative to both the datasheet and the C++ firmware. It is not
a reference for anything.

## Project conventions

- **Patched HAL**: `stm32l1xx-hal` is overridden via `[patch.crates-io]` in `Cargo.toml` to point at the `serdzz/stm32l1xx-hal` fork. Do not assume upstream API matches.
- **Logging**: `defmt` + `defmt-rtt`. `DEFMT_LOG=trace` is set in `.cargo/config.toml`.
- **No allocator.** `emballoc` went with RTIC; nothing heap-allocates. The stack starts at the end of RAM (`_stack_start` = `0x20008000`) and grows down into what `.data` and `.bss` leave — roughly 19 KiB. `STACK_SIZE` in a linker script does nothing; `cortex-m-rt` reads `_stack_start`.
- **Lints**: `make clippy` runs four passes — host lib, embedded binary, bootloader, and the host tools — all with `-D warnings`. `main.rs` carries only `#![no_std]` / `#![no_main]`; the crate-wide `deny` attributes went with the RTIC firmware.
- **Cargo features**: `low_power`, `noswd` (both empty by default).
- **CI** (`.github/workflows/ci.yml`): seven jobs — embedded build, host tests, host clippy, embedded clippy, bootloader (build + clippy + size), image format (tests + pack/verify/tamper round trip), and `cargo fmt --all --check`. `workflow_dispatch` allows a manual run.

## Where to look for deeper context

- `docs/ARCHITECTURE.md` — system overview, dual-target rationale, SharedBus pitfalls
- `docs/UI_ARCHITECTURE.md` — `Screen`/`MenuController` (shipped) vs the unused `Widget` framework in `src/gui/`
- `docs/HISTORY_SYSTEM.md` — `RingStorage` layout and retention table
- `docs/MODBUS_MAP.md` — Modbus register map
- `docs/TDC1000_REGISTER_MAP.md`, `docs/TDC7200_REGISTER_MAP.md` — chip register details
- `STM32L151.svd` — peripheral definitions for the MCU

## Why the runtime was replaced (history)

The RTIC firmware carried three coupled bugs. They are gone with it —
none of the code below exists any more — but the shape is worth knowing,
because it is the reason the runtime was replaced rather than patched.

1. **`exti9_5` ordering.** The button-wake handler called
   `power.active()` before `power.exit_sleep()`. `active()` cleared the
   sleep flag, so `exit_sleep()` skipped `reconfigure_after_stop()` and
   the clocks stayed on the MSI fallback. SPI to the LCD then ran at the
   wrong rate and the UI silently stopped rendering — "buttons don't
   react".
2. **`rcc.cr.write()` clobber.** Used `.write()` instead of `.modify()`
   to enable HSI, which reset every unspecified field and turned PLLON
   and HSEON off right after they had been turned on.
3. **RTIC 1.1.4's timer queue corrupted across STOP/SysTick wake.**
   `tq::dequeue` faulted on a `peek()` returning a pointer into unmapped
   memory.

The trap: the first two **masked** the third. Holding the system on MSI
at about 2 MHz slowed SysTick roughly twelvefold, which was slow enough
that the queue corruption never accumulated to a crash in a typical
session. Fixing either of the first two alone exposed the third and the
device HardFaulted on the first or second button press.

Masking SysTick and draining the queue around STOP was tried and did not
help — the corruption is in RAM before the mask lifts. See the failed
attempt recorded at the end of this file.

### Embassy port

The port is merged and on `main`. What was open at the time it was
written — RTC and backup registers, STOP mode, EEPROM, both TDC drivers,
USART1 for Modbus and the shell, and wiring the UI onto the key channel
— is done. `docs/EMBASSY_PORT.md` keeps the record of what changed;
`docs/DETAILED.md` covers the measurement and UI work that followed.

### UI findings verified on hardware (2026-08-02)

Two independent defects both presented as "the buttons do not work".
Both are fixed; the notes are here because the obvious-looking change
in each case is the one that reintroduces the bug.

1. **Do not treat EXTI edges as key presses** (`src/drivers/keypad.rs`).
   An earlier commit moved this driver from polling to pure event-driven
   for the idle power saving, awaiting `wait_for_falling_edge()` on the
   four pins and emitting a press per edge. On hardware that loses
   roughly nineteen presses in twenty. Measured with
   `examples/buttons.rs`, which drives nothing but these four pins:
   awaiting edges caught ~1 press in 20, sampling the same pins every
   20 ms caught 286 of 286, with the level trace showing clean
   transitions in both directions throughout. The pins and pull-ups are
   fine; awaiting the edges is what drops them. The driver now uses
   edges only to wake from STOP and detects presses by sampling — the
   same arrangement the C++ `Keyboard::read` has always used. The idle
   sample interval (150 ms) is deliberately above `min_stop_pause`
   (100 ms) so STOP still happens between samples.

2. **Park the LCD lines before cutting panel power** (`Hd44780::park`).
   Dropping the supply on PC0 while RS/RW/E/D4-D7 are still driven feeds
   current through the panel's protection diodes and leaves the
   controller in a state it does not recover from on the next power-up.
   The display then stays frozen no matter what is written to it — which
   after the first 15 s idle timeout looks exactly like dead buttons.
   The C++ does the equivalent in `Lcd::shutdown()` by reconfiguring the
   pin list to inputs with pull-downs (`hardware/lcd.cpp:96-99`).

Related: the LCD's microsecond delays busy-wait via `cortex_m::asm::delay`
rather than `Timer::after`. Under the RTC-backed low-power time driver an
await costs far more than the delay it asks for, and the driver needs ~170
of them per frame — a full 2x16 render measured 96-100 ms with awaits
against 17-31 ms without. See the comment on `delay_us`.

`examples/buttons.rs` is kept for exactly this kind of question: it
isolates keypad + LCD from the executor, the idle timeout, the
measurement task and the menu, so "is it the driver or the firmware
around it" can be answered in one flash.

### Failed fix attempt on the RTIC firmware (2026-05-19)

Historical, like the three bugs above — none of this code is in the tree
any more. Recorded because the approach looks reasonable and does not
work, so it is worth not rediscovering. Tried this combination expecting
it to close bug #3 without the deeper refactor:

- Fix #1: swap `power.exit_sleep()` before `power.active()` in `exti9_5`.
- Fix #2: replace `self.rcc.cr.write(...)` with `self.rcc.cr.modify(...)`.
- Mask SysTick `CTRL.TICKINT` in `enter_stop_mode`, clear `SCB.ICSR.PENDSTCLR` and re-enable `TICKINT` at the end of `exit_sleep` (after `reconfigure_after_stop`). Wrapped the unsafe MMIO in two helpers `mask_systick()` / `unmask_systick_clear_pending()` in `src/hardware/nvic.rs` to stay inside `#![deny(unsafe_code)]`.
- Removed the `app_request::spawn_after(IDLE_TIMEOUT, DeepSleep)` / `handle.cancel()` churn from the TIM2 timer task; replaced with `app_request::spawn(AppRequest::DeepSleep)` on the `is_active() → false` transition. Removed the `handle: Option<...>` local field too.

Result on hardware: clean boot, RTC wake works, but **first or second button press still HardFaults** with the same `Escalated BusFault → tq::dequeue → SysTick` stack trace at `0x00124xxx`. SysTick masking only narrows the immediate-wake window — corruption is already in the queue's RAM by the time `TICKINT` is re-enabled. Removing TIM2's `spawn_after` doesn't help either, because the only remaining producer (`modbus_poll`) wasn't even firing during the test — the queue should have been empty, yet `peek()` returned `Some(&garbage)`.

So masking + queue hygiene around `Power` is **not** the right layer. The corruption either happens (a) inside `monotonic::now()` returning a value that makes `tq.set_compare()` confuse internal state, or (b) inside `cortex-m-rtic 1.1.4`'s timer-queue implementation itself across STOP. Fixing it requires touching RTIC internals or replacing the monotonic.

All four changes above were reverted; tree returned to `e70d707` parity.
