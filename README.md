# uFlowmeter

Transit-time ultrasonic liquid flow meter, in Rust, on an STM32L151RC.

Firmware updates are delivered over the meter's own serial line as
AES-256-GCM encrypted images, installed by a bootloader that
authenticates them before touching the running application.

## Features

- **Transit-time measurement** with a TDC1000 analog front end and a
  TDC7200 time-to-digital converter, two transducer pairs, both flow
  directions.
- **History** in external EEPROM — hourly, daily and monthly ring
  buffers sized at compile time.
- **Local UI** — 2x16 character LCD with a four-button keypad and a
  Cyrillic menu.
- **Outputs** — Modbus RTU, M-Bus datagrams, and a 4-20 mA loop frame.
- **Encrypted field updates** over XMODEM; see
  [`docs/BOOTLOADER.md`](docs/BOOTLOADER.md).
- **Low power** — the async runtime drops the part into STOP between
  measurement cycles.
- **Host-testable core** — 345 tests run on the development machine,
  no hardware required.

## Hardware

| | |
|---|---|
| MCU | STM32L151RC — Cortex-M3, 256 KiB flash, 32 KiB RAM |
| AFE | TDC1000 — ultrasonic transducer drive and receive |
| TDC | TDC7200 — precision time-to-digital converter |
| Display | HD44780-compatible 2x16 character LCD |
| Storage | Microchip 25LC1024 — 128 KiB SPI EEPROM |
| Buses | SPI2 shared by both TDCs and the EEPROM; USART1 for Modbus, the shell and updates |

The TDC pair is clocked from an 8 MHz reference the MCU drives out on
MCO/PA8.

## Flash layout

**The application does not boot on its own.** It links at `0x08004000`,
not at the reset vector, so a unit flashed with only `uflowmeter` will
sit dead until the bootloader is flashed alongside it.

```
0x08000000  ┌──────────────────┐
            │ bootloader  16K  │  authenticate, install, jump
0x08004000  ├──────────────────┤
            │ slot A     120K  │  the application, runs from here
0x08022000  ├──────────────────┤
            │ slot B     120K  │  staged update image
0x08040000  └──────────────────┘
```

These numbers live in `fwimage::layout`, which is shared by everything
that depends on them, with a test that checks the regions tile the part
exactly.

## Repository layout

```
uflowmeter/
├── src/                     application firmware
│   ├── main.rs              embassy tasks, wiring, measurement loop
│   ├── lib.rs               host-testable surface
│   ├── drivers/             hd44780, keypad, uart, eeprom, tdc1000,
│   │                        tdc7200, sensor_mux, slot_b
│   ├── ui.rs, gui/          screens, menus and widgets
│   ├── *_lib.rs             pure logic, tested on the host:
│   │                        history, tdc, average, xmodem, upload
│   ├── modbus*.rs, mbus.rs  protocol implementations
│   └── shell.rs             serial command interface
├── bootloader/              separate crate, own linker script
├── fwimage/                 update image format + streaming AES-256-GCM
├── tools/imgtool/           host tool: pack, encrypt and inspect images
├── examples/                standalone bring-up programs
├── docs/                    architecture and hardware references
├── memory-app.x             linker memory map for slot A
├── rust-toolchain.toml      pinned compiler, components and targets
└── Makefile                 build, flash, test and lint entry points
```

`src/hardware/` and `src/measurement/` are **legacy** — they belong to
the RTIC firmware this replaced, are not referenced by `lib.rs`, and do
not compile against the current HAL. `src/main.rs.rtic-backup` is kept
for the same archival reason. Do not treat any of them as live code.

## Building

The toolchain is pinned in `rust-toolchain.toml`, so rustup installs
the right compiler, components and target automatically. Nothing needs
to be set up by hand.

```sh
make build        # application  → slot A
make bootloader   # bootloader   → reset vector (needs UFW_AES_KEY)
```

Release builds of the bootloader require a signing key:

```sh
make imgtool
target/<host>/release/imgtool keygen --out ufw.key
UFW_AES_KEY=$(cat ufw.key) make bootloader
```

A build with no key **fails by design** — a default key that shipped by
accident would be worse than a build error, since nothing about the
resulting firmware would look wrong. `--features dev-key` compiles in a
well-known key for bench work only.

## Flashing

Two binaries, two downloads:

```sh
make flash              # bootloader + application, from scratch
make flash-bootloader   # bootloader only
make flash-app          # application only
```

**Always flash at `--speed 500`** — the Makefile targets do. At the
default SWD rate this board fails to connect, reproducibly and in
several different ways depending on what the firmware is doing at the
time. `CLAUDE.md` documents the failure modes and the hold-RESET
fallback.

`cargo run --release` is no longer the way to flash: it would write
only the application and leave the reset vector as it found it.

## Field updates

```sh
UFW_AES_KEY=$(cat ufw.key) make image IMG_VERSION=7
```

builds the application, encrypts it and writes `app.ufw`. Send it to a
meter over the serial line:

```
> firmware_update
Erasing staging slot...
Send the .ufw image with XMODEM-CRC now.
<send app.ufw with any XMODEM-CRC sender>
Staged. Resetting...
```

The meter reboots, and the bootloader authenticates the image before
any of it reaches the running slot.

Mind the line rate. An image is about 86 KiB, which is 884 000 bits on
an 8N1 line, so the transfer costs roughly 8 s at 115200, 1.5 min at
the 9600 Modbus uses, and 12 min at the 1200 of M-Bus. Protocol
overhead adds little on top — XMODEM spends five bytes per 1 KiB
packet. M-Bus is still a poor mode to be in when an update is due.

An interrupted transfer or a power cut at any point leaves the old
application bootable — see
[`docs/BOOTLOADER.md`](docs/BOOTLOADER.md) for the ordering that
guarantees it.

## Testing

The pure logic is split out of the hardware-facing code specifically so
it can be tested without a device.

```sh
make test           # application library — 319 tests
make test-fwimage   # image format and AES-GCM — 26 tests
make clippy         # all four crates, both targets, -D warnings
make ui-examples    # run the UI on the host
```

The AES-GCM vectors come from OpenSSL rather than from this
implementation: checking our own output against itself would only prove
self-consistency.

## Interfaces

One USART serves all of these, so **the configured communication type
decides the line rate** and everything else on the wire follows it:

| Communication type | Line rate |
|---|---|
| Modbus | 9600 baud |
| M-Bus | 1200 baud |
| off / analog | 115200 baud |

The rate is read when a session starts, so a change made in the menu or
over Modbus takes effect within a second without a reset. The practical
consequence is that the shell is reachable at 9600 while Modbus is
selected, not at its own rate — there is only one pair of wires. The
Modbus inter-frame silence follows from the rate as the spec requires,
which is why it is not a constant: at 9600 the boundary is about 4 ms,
not the 1750 µs that applies above 19200 baud.

**Modbus RTU** — 8N1, configurable address, functions 0x03, 0x06 and
0x10. Register map in [`docs/MODBUS_MAP.md`](docs/MODBUS_MAP.md).

**Shell** — shares USART1 with Modbus; a line is treated as a command
if it starts with a known keyword, otherwise as a Modbus frame. `help`
lists the commands.

**M-Bus** — RSP_UD datagram broadcast every five minutes.

**4-20 mA** — a serial frame carrying the scaled reading.

## Debugging

`defmt` over RTT:

```sh
probe-rs run --chip STM32L151RC --speed 500 \
  target/thumbv7m-none-eabi/release/uflowmeter
```

With a bootloader present, the application starts a few milliseconds
after reset; RTT attaches to whichever program is running.

## Documentation

| | |
|---|---|
| [BOOTLOADER.md](docs/BOOTLOADER.md) | flash layout, image format, keys, update procedure |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | system overview and dual-target rationale |
| [UI_ARCHITECTURE.md](docs/UI_ARCHITECTURE.md) | screens, widgets, menu controller |
| [HISTORY_SYSTEM.md](docs/HISTORY_SYSTEM.md) | ring buffer layout and retention |
| [MODBUS_MAP.md](docs/MODBUS_MAP.md) | register map |
| [TDC1000_REGISTER_MAP.md](docs/TDC1000_REGISTER_MAP.md), [TDC7200_REGISTER_MAP.md](docs/TDC7200_REGISTER_MAP.md) | chip registers |
| [DETAILED.md](docs/DETAILED.md) | detailed record of the measurement and UI work |
| `CLAUDE.md` | build commands, flashing quirks, load-bearing bugs |

## Status

Verified on hardware: boot, LCD, keypad, menu and edit mode, EEPROM,
RTC, the measurement cycle, both TDC chips configuring and reading
back, Modbus, the shell, and the bootloader handing over to slot A.

Not yet verified:

- **The update path has never run end to end on a device.** Only the
  bootloader's no-image boot path has executed; it has not
  authenticated, decrypted or installed a real image on hardware.
- **No XMODEM transfer has crossed the wire.** The protocol logic is
  tested, but the bench has no RS-485 adapter.
- **Time of flight is unproven.** With no transducers attached the
  TDC1000 reports `ERR_NO_SIG`. The decode arithmetic is covered by
  tests against the reference implementation; that it yields correct
  physical readings is not established.

## License

MIT OR Apache-2.0

## Author

Sergej Lepin <sergej.lepin@gmail.com>
