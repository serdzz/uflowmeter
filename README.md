# uflowmeter

Firmware for an ultrasonic flow meter built around an **STM32L151RC**
(Cortex-M3, 256 KB flash, 32 KB SRAM, 8 MHz HSE crystal). Drives two
TDC chips (TDC1000 analog frontend + TDC7200 time-to-digital), an
HD44780 character LCD, a 4-button keypad, an external 25LC1024 EEPROM
for calibration + history, and a Modbus RTU slave over RS-485.

This branch (`zephyr`) is a green-field port to Zephyr RTOS in C++.
Earlier Rust implementations live on `main` (RTIC) and `rework/embassy`
(Embassy async). See [`CLAUDE.md`](CLAUDE.md) for the migration roadmap
and pin-map reference.

## Quick start

Prereqs:
- [Zephyr toolchain](https://docs.zephyrproject.org/latest/develop/getting_started/) (SDK 0.16+, west, arm-zephyr-eabi)
- [`probe-rs`](https://probe.rs/) for flashing (the prior Rust build used it; the board.cmake keeps it as the default runner)

```sh
# One-time: pull Zephyr + HALs into sibling directories.
west init -l .
west update

# Build for the custom board.
west build -b uflowmeter_v1 .

# Flash + open the serial console (PA9 TX / PA10 RX, 115200 baud).
west flash
picocom /dev/tty.usbmodem* -b 115200
```

## Status

First commit on this branch matches the embassy port's last
confirmed-working state on hardware: boot → LCD shows
"uflowmeter / zephyr boot" → pressing any of the four keypad buttons
echoes the label.

The roadmap for the rest of the port (TDC measurement, EEPROM, Modbus,
history rings, UI state machine, low-power STOP mode) is tracked in
`/Users/sergejlepin/.claude/plans/zephyr-linked-snowflake.md` and
reflected in commit history on this branch.

## Board

Custom board definition (HWMv2):
[`boards/uflowmeter/uflowmeter_v1/`](boards/uflowmeter/uflowmeter_v1/).
Pin map lives in
[`uflowmeter_v1.dts`](boards/uflowmeter/uflowmeter_v1/uflowmeter_v1.dts) —
that's the source of truth, everything else (driver code, docs) refers
back to it.

## Licensing

Application code: Apache-2.0 (matches Zephyr).
Vendored `lowlander/zpp` headers: Apache-2.0, see
[`include/zpp/LICENSE`](include/zpp/LICENSE) and
[`include/zpp/NOTICES.md`](include/zpp/NOTICES.md).
