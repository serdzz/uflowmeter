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

| Subsystem | Rust source on rework/embassy | Notes |
|-----------|-------------------------------|-------|
| Options / EEPROM (25LC1024) | `src/options.rs`, `src/drivers/eeprom.rs` | Shared SPI2 bus, dual-page CRC layout |
| Calibration | `src/calibration.rs` | Pure-logic piecewise-linear |
| TDC1000 + TDC7200 | `src/drivers/tdc1000.rs`, `src/drivers/tdc7200.rs`, `src/main.rs` `measurement_task` | Shared SPI2, EXTI0 wake on PB0 |
| History rings | `src/history_lib.rs` | Three const-generic ring buffers, EEPROM-backed |
| Modbus RTU | `src/modbus.rs`, `src/modbus_handler.rs` | On-demand USART1 session |
| UI state machine | `src/ui.rs`, `src/gui/*` | `MenuController` over 4 ring-buffer menus |
| Shell | `src/shell.rs` | set_serial, set_address, date, verbose |
| STOP-mode low-power | `src/main.rs` idle handling | Zephyr `pm_system_suspend()` — see `CONFIG_PM` |

The roadmap for the next few commits sits in `/Users/sergejlepin/.claude/plans/zephyr-linked-snowflake.md`.

## Background context worth reading once

- `docs/ARCHITECTURE.md` — system overview (still accurate for the hardware)
- `docs/UI_ARCHITECTURE.md` — Widget / Screen / MenuController design (port target)
- `docs/HISTORY_SYSTEM.md` — ring buffer layout, EEPROM offsets
- `docs/MODBUS_MAP.md` — register map (must stay byte-stable across the port)
- `docs/TDC1000_REGISTER_MAP.md`, `docs/TDC7200_REGISTER_MAP.md` — chip register details
- `docs/EMBASSY_PORT.md` — what we learned during the embassy attempt; explains why we left Rust
