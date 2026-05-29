# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Firmware for an ultrasonic flow meter built around an **STM32L151RC** (Cortex-M3, 256 KB flash, 32 KB SRAM). The product has lived three lives:

1. RTIC (Rust) — see `main` branch.
2. Embassy (Rust async) — see `rework/embassy` branch.
3. **Zephyr RTOS in C++** — this branch (`zephyr`).

The Zephyr port started 2026-05-29 and is intentionally **green-field**: the Rust tree was wiped on first commit so this is now a Zephyr T2-star application with `west.yml` at the root. The Rust source for every subsystem still lives in git history on `rework/embassy` — use it as the spec when porting a feature:

```sh
git show rework/embassy:src/<file>.rs        # read source
git diff rework/embassy main -- src/<file>   # see what changed RTIC → embassy
```

## Build & flash commands

This repo IS the Zephyr application. The first build clones Zephyr + HALs into sibling directories via `west`.

| Task | Command |
|------|---------|
| Bootstrap workspace (one-time) | `west init -l . && west update` |
| Build for the custom board | `west build -b uflowmeter_v1 .` |
| Clean build | `west build -b uflowmeter_v1 -p always .` |
| Flash (probe-rs) | `west flash` |
| Open serial console | `picocom /dev/tty.usbmodem* -b 115200` |
| Report binary size | `arm-zephyr-eabi-size build/zephyr/zephyr.elf` |
| Read DT bindings consumed | `west build -t dts_report` |
| Open menuconfig | `west build -t menuconfig` |

There's no host-side test target yet. The Rust tree had host-runnable unit tests (`make test`); reintroducing them on Zephyr means adding a `tests/` sibling that builds via Zephyr's twister or a separate GoogleTest target.

## Repo layout

```
/                         # T2-star Zephyr application
├── west.yml              # manifest: pins Zephyr v3.7 LTS + HAL_STM32
├── CMakeLists.txt        # top-level: find_package(Zephyr) + app sources
├── prj.conf              # Kconfig overrides (C++20, LOG, GPIO)
├── Kconfig               # app-specific symbols (empty for now)
├── sample.yaml           # twister metadata
├── boards/
│   └── uflowmeter/uflowmeter_v1/   # HWMv2 custom board (STM32L151RC)
├── dts/bindings/         # custom DT bindings (HD44780)
├── include/zpp/          # vendored lowlander/zpp headers (Apache-2.0)
├── src/
│   ├── main.cpp
│   └── drivers/
│       ├── hd44780.{cpp,hpp}
│       └── keypad.{cpp,hpp}
├── docs/                 # ARCHITECTURE, UI_ARCHITECTURE, HISTORY_SYSTEM,
│                         # MODBUS_MAP, TDC*_REGISTER_MAP — reference material
└── STM32L151.svd         # register definitions (still useful as reference)
```

The single source of truth for the pin map is **`boards/uflowmeter/uflowmeter_v1/uflowmeter_v1.dts`**. Driver code consumes it via `DT_CHOSEN(uflowmeter_lcd)` / `DT_CHOSEN(uflowmeter_keypad)`.

## Conventions

- **C++20**, enforced by `CONFIG_STD_CPP20=y`. zpp headers require it.
- **No exceptions, no RTTI** (Zephyr default).
- **No heap in production code** — drivers use static state, message queues are `K_MSGQ_DEFINE`'d at file scope.
- **Pure C++ namespaces** everywhere (`uflow::drivers`, `uflow::ui`, …). One namespace per top-level subsystem.
- **zpp** wraps kernel primitives (threads, mutexes, queues, timers). It does NOT wrap GPIO/SPI/UART — those go through Zephyr's plain C device API directly. See `include/zpp/NOTICES.md` for the vendoring detail.
- **Logging**: `LOG_MODULE_REGISTER(name, CONFIG_LOG_DEFAULT_LEVEL)`. No `printk` in new code.

## Pending ports (from rework/embassy)

| Subsystem | Rust source on rework/embassy | Status |
|-----------|-------------------------------|--------|
| Options / EEPROM (25LC1024) | `src/options.rs`, `src/drivers/eeprom.rs` | **Done.** Zephyr `atmel,at25` driver via DT (no custom driver). Options is a packed POD in `src/options.{hpp,cpp}` — 116 bytes, byte-exact with the Rust `modular_bitfield`. Dual-page CRC layout preserved. Chip parked in deep power-down after boot via `src/drivers/eeprom_power.{hpp,cpp}` (saves ~4 µA continuous over the at25 driver alone). |
| Calibration | `src/calibration.rs` | **Done.** Pure-logic port in `src/calibration.{hpp,cpp}` — Calculator + CalibTable + MeterConfig + apply_ratio (piecewise-linear, 4 zones). No Zephyr deps. |
| TDC1000 + TDC7200 | `src/drivers/tdc1000.rs`, `src/drivers/tdc7200.rs`, `src/main.rs` `measurement_task` | **Done.** `src/drivers/tdc{1000,7200}.{hpp,cpp}` use sidecar `spi_dt_spec` against `&spi2`. `src/measurement.{hpp,cpp}` runs a dedicated thread (priority 7, 1 KB stack) on a 5 s cycle: power up → load configs → downstream tof → upstream tof → power down → calc → publish via atomic + LCD row 1. TDC7200 INT (PB0 EXTI) → k_sem → measurement thread. Live calibration refresh deferred (captured each cycle from `options::g_options`; works as soon as Modbus/UI mutates the global). |
| History rings | `src/history_lib.rs` | Three const-generic ring buffers, EEPROM-backed |
| Modbus RTU | `src/modbus.rs`, `src/modbus_handler.rs`, `src/drivers/uart.rs` | **All 3 commits done.** Codec + register handler + USART1 IRQ-driven transport. Worker thread (K_PRIO_PREEMPT(6), 2 KB stack) drains per-byte RX msgq with 1.75 ms inter-frame silence timeout, hands frames to the handler, writes responses via uart_poll_out. **EEPROM concurrency mutex** (`drivers::eeprom_mutex`) added — main UI save + modbus_handler save + history ring writes all acquire it around their wake/op/sleep sequences. **Outstanding for SCADA reliability**: UART STOP-wake (per-session PA10 EXTI + USART CR1.UESM) — bytes during STOP are currently lost; peer must retry. Build with `CONFIG_PM=n` for always-reliable Modbus, or accept Modbus-retry semantics with PM on. |
| UI state machine | `src/ui.rs`, `src/gui/*` | **All 6 commits done.** Navigation across all 19 screens + edit modes (EEPROM-persisted Set\*) + DateTime field-stepping (RAM-backed) + HistoryWidget date picker (Hour/Day/Month) + Cyrillic glyph rendering (FONT table + ROM lookalikes + per-frame CGRAM allocator) + **blink animation on DateTime/History active field** + **Version easter-egg pattern** (Enter×3, Up×2, Down×2 → EnterCalibration) + **AppRequest handlers** (SystemReset → `sys_reboot`, EnterCalibration → `select(MenuId::Calibration)`, DeepSleep no-op as PM handles STOP). Main-loop tick adapts: 150 ms while a multi-field edit is open (blink visible), 2 s idle (minimizes wakes). **Outstanding**: RTC TR/DR datetime persistence, host-side test harness (intentionally separate scope). |
| History rings | `src/history_lib.rs`, `src/history.rs`, `src/main.rs` `handle_history_tick` | **Done.** `src/history_lib.hpp` is a header-only `RingStorage<OFFSET, SIZE, ELEMENT_SIZE>` template. `src/history.cpp` instantiates Hour (2160×3600s), Day (1116×86400s), Month (120×2678400s) and runs a 60 s tick thread that accumulates `measurement::latest_flow_m3h` into per-ring buckets, writing on minute=0 / hour=0 / day=1 boundaries. Each ring op brackets eeprom_power DP wake/sleep. **Fixed embassy bug**: `SIZE_ON_FLASH = 16 + 4*SIZE` (embassy had `SIZE + 20` which treats SIZE as bytes and would have overlapped chained rings 4x). |
| Shell | `src/shell.rs` | **Done.** `src/shell.{hpp,cpp}` is the pure-logic parser (Result + Action types, host-testable). UART worker now accumulates BOTH a line buffer (terminated on \r/\n) and a Modbus frame buffer (terminated on 1.75 ms silence) per the embassy dual-discrimination heuristic. Shell line dispatch is inline in the worker: SetDateUnix calls `datetime::set` (Unix→2000 epoch shift), SetSerial writes Options + saves via the shared `eeprom_mutex`, SetVerbose is log-only. Commands: help / date get|set / zero / calibrate / set_serial / set_verbose / get_settings / get_calibration — verbatim from `rework/embassy:src/shell.rs`. |
| STOP-mode low-power | `src/main.rs` idle handling | Zephyr `pm_system_suspend()` — see `CONFIG_PM` |

The roadmap for the next few commits sits in `/Users/sergejlepin/.claude/plans/zephyr-linked-snowflake.md`.

## Power management

This branch implements full STOP-mode low-power via Zephyr's PM
framework + a custom STM32L1 RTC system clock driver:

- **CPU**: `pm_state_set(SUSPEND_TO_IDLE)` (defined in `src/power.cpp`)
  drops the chip into STOP — main regulator → low-power, PLL+HSE off,
  SLEEPDEEP + WFI. Wakes on RTC WUT (the kernel timer) or any keypad
  EXTI (PB6-PB9).
- **System clock**: `src/timer/uflowmeter_rtc_timer.c` replaces the
  default SysTick driver. Backed by the STM32L1 RTC running off LSI
  (~37 kHz, ±10-15% accuracy); subseconds counter at ~1024 Hz is the
  kernel cycle source; WUT on RTC/16 (~2312 Hz, ~28 s max sleep)
  programs the next wake. `CONFIG_CORTEX_M_SYSTICK=n`,
  `CONFIG_TICKLESS_KERNEL=y`, `CONFIG_PM=y`.
- **LCD**: VCC (PC0 active-LOW) + backlight (PC5 active-LOW) both
  cut on STOP entry. HD44780 logic loses state — `pm_state_exit_post_ops`
  re-runs `lcd().init()` (~50 ms blocking) on every wake. Tradeoff
  worth ~1 mA standby for the 1% wake-time hit at 5-second cycles.
- **EEPROM**: already in deep power-down since boot — no per-STOP
  action needed.
- **TDC1000/TDC7200**: already EN=LOW between measurement cycles —
  no per-STOP action needed.

### Accuracy caveat

LSI is uncalibrated and drifts ±10-15%. `k_uptime` and any
`k_sleep(N)` duration will slip accordingly. Acceptable for a
5-second measurement cadence; not acceptable for time-of-day. When
calendar accuracy matters (history rings, RTC datetime display),
either add an LSE crystal on the next board rev or periodically
calibrate LSI against HSE via RCC CIR (not implemented).

### Wake sources

| EXTI line | Source | Behavior |
|---|---|---|
| 6  | PB6 (keypad Config) | Wakes; keypad ISR pushes KeyEvent → main thread |
| 7  | PB7 (keypad Enter)  | same |
| 8  | PB8 (keypad Down)   | same |
| 9  | PB9 (keypad Up)     | same |
| 22 | RTC WUT             | Wakes; sys_clock_announce drains pending_ticks |

When a keypad wake interrupts a long sleep, the kernel returns to
`measurement` thread early — measurement runs its cycle, returns to
`k_sleep(K_SECONDS(5))`, the timer driver programs WUT for a fresh
5 s. Net effect: the 5 s cadence resets on every keypress, which is
acceptable but not strictly periodic. Fix: track absolute target time
in measurement thread, re-sleep for the remainder. Deferred.

## EEPROM access invariants

The 25LC1024 sits in **deep power-down (~1 µA)** after the boot-time
Options load and stays there. Any code that calls `eeprom_read` /
`eeprom_write` (directly or via `uflow::options::load`/`save`) **must**
first call `uflow::drivers::eeprom_exit_deep_power_down()` and pair it
with `eeprom_enter_deep_power_down()` afterwards — the chip silently
ignores all commands except RDP (0xAB) while in DP. See
`src/drivers/eeprom_power.hpp` for the full contract (single-threaded,
not ISR-safe, ~100 µs tRDP wake latency).

When STOP-mode integration lands, the idle hook should leave the EEPROM
in DP — there's no need to wake it before suspend.

## Background context worth reading once

- `docs/ARCHITECTURE.md` — system overview (still accurate for the hardware)
- `docs/UI_ARCHITECTURE.md` — Widget / Screen / MenuController design (port target)
- `docs/HISTORY_SYSTEM.md` — ring buffer layout, EEPROM offsets
- `docs/MODBUS_MAP.md` — register map (must stay byte-stable across the port)
- `docs/TDC1000_REGISTER_MAP.md`, `docs/TDC7200_REGISTER_MAP.md` — chip register details
- `docs/EMBASSY_PORT.md` — what we learned during the embassy attempt; explains why we left Rust
