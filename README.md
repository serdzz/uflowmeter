# uflowmeter

Firmware for a battery-powered ultrasonic flow meter built around an
**STM32L151RC** (Cortex-M3, 256 KB flash, 32 KB SRAM, 8 MHz HSE
crystal). Drives:

- TDC1000 ultrasonic analog frontend (RX channel switching, TX pulses)
- TDC7200 time-to-digital converter (sub-nanosecond ToF capture)
- HD44780 16×2 character LCD with Cyrillic glyph support
- 4-button keypad + idle-state EXTI wake
- 25LC1024 SPI EEPROM (128 KB, for Options + 3 history rings)
- Modbus RTU slave + interactive shell on USART1
- Internal RTC (TR/DR + backup register persistence)

The flow-rate algorithm is piecewise-linear with a 4-zone K-correction
applied on top of a `(dTOF - dTOF0) × const_val / (sum_TOF)²` raw
calculation — matches the legacy C++ production calibration so existing
field units stay compatible.

## Status

This branch (`zephyr`) is a green-field rewrite of the firmware on
**Zephyr RTOS in C++20**. Three prior implementations existed:

| Branch | Runtime | Notes |
|---|---|---|
| `main` | Rust + RTIC 1.1.4 | Original; blocked by SysTick/STOP-mode bugs |
| `rework/embassy` | Rust + embassy async | Blocked by L1 lacking embassy `low-power` cfg |
| `zephyr` (here) | C++20 on Zephyr 4.4 | **All subsystems ported. PM disabled pending unblock.** |

Build: green against Zephyr 4.4.99 + SDK 1.0.1 (also tested 4.1.99 +
SDK 0.17.1 earlier). Binary size: **52 KB flash (20%) / 12 KB SRAM
(38%)**.

Host tests: **46/46 pass** (modbus codec, shell parser, calibration
math).

On-device: build verified; flash needs a connected ST-Link + cold-boot
window. See [Flash recovery](docs/ZEPHYR_PORT.md#flash-recovery--swdapwait--swddpwait)
if SWD locks up.

## Quick start

Prereqs:
- Zephyr SDK 1.0+ for Zephyr 4.4 (SDK 0.17 for Zephyr 4.1)
- west + Python venv with the standard Zephyr dependencies
- `openocd` or `probe-rs` for flashing
- An ST-Link V2 + the uflowmeter v1 hardware (or compatible STM32L151RC board)

```sh
# Build:
source ~/zephyrproject/.venv/bin/activate
export ZEPHYR_BASE=~/uflowmeter/zephyr
export ZEPHYR_SDK_INSTALL_DIR=~/zephyr-sdk-1.0.1
west build -b uflowmeter_v1 . -p always

# Flash via openocd:
west flash --runner openocd

# Or via probe-rs (the runner the Rust era used):
probe-rs download --chip STM32L151RC --binary-format hex build/zephyr/zephyr.hex

# Serial console (PA9 TX / PA10 RX, 115200 baud, shares with Modbus):
picocom /dev/tty.usbmodem* -b 115200
```

Host tests (no Zephyr SDK needed):

```sh
cmake -S tests -B build-tests
cmake --build build-tests
./build-tests/uflowmeter_tests  # → "46 passed, 0 failed (of 46)"
```

## Custom board

Board definition (HWMv2 layout):
[`boards/uflowmeter/uflowmeter_v1/`](boards/uflowmeter/uflowmeter_v1/).
Pin map source of truth is
[`uflowmeter_v1.dts`](boards/uflowmeter/uflowmeter_v1/uflowmeter_v1.dts).

Summary:
- HSE 8 MHz crystal → PLL → 16 MHz SYSCLK (per Zephyr issue #22078)
- LSI ~37 kHz → RTC subseconds (1024 Hz) + WUT (~2312 Hz) — drives the custom sys_clock
- USART1 (PA9/PA10) → Modbus + shell, RS-485 transceiver enable on PC9
- SPI2 (PB13/14/15) → 25LC1024 EEPROM (CS=PC10), TDC1000 (CS=PB11), TDC7200 (CS=PB12)
- HD44780 LCD on PC1/PC2/PC3 + PA4-7; VCC = PC0, backlight = PC5
- Keypad on PB6/PB7/PB8/PB9 (EXTI)
- TDC EN/RST/INT pins: PB10/PC6, PB1/PB0
- MCO output on PA8 → TDC clock (configuration deferred — no L1 DT binding)

## Documentation

- **[`docs/ZEPHYR_PORT.md`](docs/ZEPHYR_PORT.md)** — canonical port doc:
  what was migrated, why, what wasn't, known gaps, recovery procedures.
  **Read this for the deep dive.**
- [`CLAUDE.md`](CLAUDE.md) — day-to-day reference (build/flash commands,
  subsystem status table, repo layout, EEPROM access invariants)
- [`docs/EMBASSY_PORT.md`](docs/EMBASSY_PORT.md) — predecessor port's
  notes; many comments still apply
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — original system
  overview; hardware sections still apply
- [`docs/MODBUS_MAP.md`](docs/MODBUS_MAP.md) — Modbus register map
  (byte-exact with the embassy port)
- [`docs/HISTORY_SYSTEM.md`](docs/HISTORY_SYSTEM.md) — ring layout
  + retention table
- [`docs/TDC1000_REGISTER_MAP.md`](docs/TDC1000_REGISTER_MAP.md) +
  [`docs/TDC7200_REGISTER_MAP.md`](docs/TDC7200_REGISTER_MAP.md) —
  chip register details
- [`tests/README.md`](tests/README.md) — host test harness

## Licensing

Application code: Apache-2.0 (matches Zephyr).
Vendored `lowlander/zpp` headers: Apache-2.0, see
[`include/zpp/LICENSE`](include/zpp/LICENSE) and
[`include/zpp/NOTICES.md`](include/zpp/NOTICES.md).
