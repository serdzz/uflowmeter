# Embassy port

This document covers the port from the original RTIC 1.1.4 build to
the [embassy](https://github.com/embassy-rs/embassy) async runtime.
The port is merged; this is kept as the record of what changed and
why.
Only the differences against the RTIC-era architecture documents
(`ARCHITECTURE.md`, `HISTORY_SYSTEM.md`, etc.) are listed here — for
everything that hasn't changed (Modbus register map, history ring
layout, TDC register details) those documents still apply.

## Why the port

The RTIC build hit three coupled bugs in the STOP / wake / UI path
that together kept buttons from working reliably after a sleep cycle
(see `CLAUDE.md` → "Known load-bearing bugs in STOP / wake / UI
path"). Rather than fix three layers of timer-queue interaction with
`cortex-m-rtic 1.1.4`, `systick-monotonic`, and the STM32L1 STOP
mode, the runtime was replaced wholesale. Embassy's `low-power`
feature handles transparent STOP entry on its own.

Patched `embassy-stm32` fork: <https://github.com/serdzz/embassy>,
branch `stm32l1_low_power`. The patch adds STM32L1 to the cfg gates
in `low_power.rs` and `rtc/low_power.rs` so embassy's transparent
STOP-mode executor works on our chip (upstream embassy only supports
L4/L5/U5/U3/WB/WL/U0). Pinned via `[patch.crates-io]` in `Cargo.toml`.

## Task layout

A single `embassy_executor::Executor` (the `executor-thread` flavour
that integrates with the `low-power` feature) drives the following
tasks. Everything that isn't running is `await`ing on a channel,
signal, or EXTI line; the executor enters STOP when all tasks are
suspended and at least one alarm is more than `min_stop_pause` =
10 ms away.

| Task | Spawn site | Wake source(s) |
|------|------------|----------------|
| `main` loop | implicit | `KEYS`, `MODBUS_FRAMES`, `HISTORY_TICK`, `FLOW_RESULT`, `SHELL_ACTIONS`, idle-deadline `Timer` |
| `measurement_task` | `main.rs` | 5 s `Timer::after_secs(5)` |
| `history_tick_task` | `main.rs` | 60 s `Timer::after_secs(60)` |
| `keypad_task` | `main.rs` | EXTI on PB6..PB9 while idle; 50 ms poll while a key is held |
| `uart_session_task` | `main.rs` | EXTI on PA10 falling edge OR `UART_TX` channel push |
| `shell_task` | `main.rs` | `SHELL_LINES` channel receive |

### Cross-task channels / signals

| Static | Type | Purpose |
|--------|------|---------|
| `KEYS` | `Channel<…, KeyEvent, 8>` | Keypad → main UI dispatcher |
| `MODBUS_FRAMES` | `Channel<…, ModbusFrame, 4>` | uart_session → main Modbus handler |
| `SHELL_LINES` | `Channel<…, ShellLine, 4>` | uart_session → shell_task |
| `SHELL_ACTIONS` | `Channel<…, ShellAction, 4>` | shell_task → main (side effects) |
| `UART_TX` | `Channel<…, ModbusFrame, 4>` | any task → uart_session (outbound bytes) |
| `HISTORY_TICK` | `Channel<…, (), 1>` | history_tick_task → main (1-slot coalescing) |
| `FLOW_RESULT` | `Signal<…, f32>` | measurement_task → main (latest m³/h, latest-wins) |
| `OPTIONS_WATCH` | `Watch<…, Options, 2>` | main → measurement_task (live calibration) |

## Power management

### Background

embassy's transparent STOP integration counts a per-peripheral
`REFCOUNT_STOP1` / `REFCOUNT_STOP2`. `get_stop_mode` returns `None`
(silent skip) whenever any peripheral is bumping the refcount,
which keeps the MCU out of STOP. The biggest pinners on this
firmware are USART1 (DMA-async) and the active SPI bus — both bump
the refcount for the duration of any pending read/write.

The measured bench-unit baseline was ~10 mA average pre-port. The
combination of fixes below brings that down to ~4 mA in idle
sessions and ~2-3 mA in deep idle (LCD power-gated).

### Wake architecture

```
            ┌──────────────────────────┐
            │   main: select6 over     │
            │   KEYS / MODBUS_FRAMES / │
            │   HISTORY_TICK / FLOW /  │
            │   SHELL_ACTIONS / idle   │
            └───────────┬──────────────┘
                        │
        ┌───────────────┼───────────────────────┐
        ▼               ▼                       ▼
  EXTI PB6..PB9   EXTI PA10 + UART_TX     Timer 60 s
  (keypad)        (uart_session_task)     (history_tick)
        │               │                       │
        └───── all branches await with no       │
               sub-10 ms alarm armed ──────────┘
                        │
                        ▼
                ┌───────────────┐
                │ embassy STOP  │
                └───────────────┘
```

### Optimisations applied (in order)

1. **EXTI-wake UART session** (`drivers/uart.rs`).
   `USART1` is *not* initialised at boot. While idle, `PA10` is an
   `ExtiInput` with `Pull::Up`. The falling edge of the first
   incoming start bit wakes the MCU. Only then does
   `uart_session_task` instantiate `Uart` from the reborrowed
   `Peri` handles, run the framer for ~800 ms after the last byte,
   and drop the `Uart` again. **Trade-off**: the first byte of every
   new transmission is lost (USART is off when the start bit
   arrives); Modbus masters retry on CRC mismatch, so this is fine
   on the wire. Shell users will see the first character of their
   first post-idle command swallowed.

   Empirical impact: bench-unit `enter stop` count over 30 s went
   from 1 to ~1000 with this single change.

2. **Event-driven keypad** (`drivers/keypad.rs`). Two modes:
   - *Idle* — `select4` over the four `ExtiInput::wait_for_falling_edge`
     futures. No timer is armed.
   - *Pressed* — falls back to a 20 Hz poll loop to drive
     `REPEAT_DELAY` / `REPEAT_INTERVAL`. Exits back to *Idle* the
     moment every button is released.

   Removed the previous 20 Hz idle poll, which was responsible for
   most of the wake-up cycles seen on the PPK trace.

3. **TDC1000/TDC7200 power gating** (`measurement_task`). EN starts
   `Level::Low` at boot. Each 5 s cycle: `power_on()` both →
   `Timer::after_millis(1)` regulator settle → reload TDC config
   from `Options` → measure down + up → `power_off()` both →
   process result. Saves ~0.5 mA continuous; the chips are on for
   ~2 % of each cycle.

4. **LCD power-gate + idle backlight** (`main.rs`). Backlight starts
   off at boot; first key press turns it on, an idle deadline (15 s
   after the last key) turns it back off *and* drops PC0 (LCD VCC).
   On the next key press the deadline is re-armed, PC0 is asserted,
   a 50 ms regulator settle elapses, `lcd.init().await` runs (~60 ms),
   then the backlight comes back on. Wake-time hit is ~110 ms for
   the first press after idle.

5. **Per-event `OPTIONS_WATCH` republish**. The main loop pushes the
   current `Options` snapshot to `OPTIONS_WATCH.sender()` after every
   UI dispatch and Modbus write. `measurement_task` calls
   `try_changed()` at the top of each cycle to pick up new
   calibration without a reboot.

6. **Persistent uptime via RTC backup registers** (`handle_history_tick`).
   `BKP0` = `uptime_seconds`, `BKP1` = `last_uptime_rtc`. Each
   60-s tick: `delta = current_ts − last_uptime_rtc`, clamped to
   24 h (sanity-guard for `date set` jumps), `saturating_add`-ed
   into `uptime_seconds`, both fields written back. Loaded into the
   `App` struct once at boot via `load_uptime_from_backup`.

### Things that were *tried and reverted*

- **MSI 4 MHz → 2 MHz**: avg current went *up* from 4.5 mA to
  7.1 mA. Slower core stretches every wake task → more wall-clock
  time above the `min_stop_pause` floor → embassy ends up skipping
  STOP entry more often. Net active duty cycle climbed faster than
  per-tick savings. Reverted to default `RANGE4M`; comment in
  `main.rs` records the finding.

### Things *not* attempted

- **GPIO save/restore around STOP** (the legacy `gpio_power.rs`
  pattern): estimated < 200 µA win, well below current measurement
  noise. Skipped in favour of the keypad-EXTI rewrite, which buys
  ms-per-second of avoided CPU time.
- **RS-485 transceiver power-gating** (PC9): can't be turned off
  while idle, since the wake EXTI on PA10 needs the transceiver
  alive to present the line.

## Modbus register map changes

`OPTIONS_END` raised from `0x001F` (32 registers / 64 bytes) to
`0x0039` (58 registers / 116 bytes) so the per-installation
calibration table (`zero1`, `v**`, `k**`, `const_val`) is reachable
via Modbus reads and writes, not just the first 64 bytes. Added
`reserved: B8` trailer to round the serialized `Options` bitfield
to an even byte count so register 57 reads no longer trip the
"end_byte > options_bytes.len()" OOB check.

A new `const_val: B32` field (f32 bits) was appended to the
bitfield to back `MeterConfig::const_val` (speed-of-sound geometry
constant L²/(2·cos α)). It sits at byte offset 111-114 — straddles
registers 55-57 because `sensor_type: B8` shifts the alignment of
all subsequent f32 fields. Existing on-disk EEPROM data is
preserved: old firmware's saved Options page CRC-matches under the
new layout because the new field is initialised to 0 from prior
zero padding.

## Persistence layout

EEPROM (25LC1024, 128 KB) layout is unchanged from the legacy build:

| Offset | Size | Contents |
|--------|------|----------|
| 0 | 1024 | Options primary copy |
| 1024 | 1024 | Options secondary copy (CRC fallback) |
| 4096 | varies | History rings (Hour → Day → Month, chained via `SIZE_ON_FLASH`) |

Both Options copies are read by `Options::load_with_buf`; primary
is tried first, secondary on CRC mismatch.

## Dual-target build

Same as before but with one Cargo.toml change worth highlighting:
embedded-only deps (cortex-m, cortex-m-rt, defmt-rtt, panic-probe,
embassy-*) are now under
`[target.'cfg(target_arch = "arm")'.dependencies]`. Pure-Rust deps
(defmt, embedded-hal, heapless, time, modular-bitfield, crc16, …)
remain in `[dependencies]` so `cargo test --lib` works on host
without stripping the `.cargo/config.toml` target. `make test` runs
The host suite has grown since; see the README for the current count. At the time of the port, 264 tests across `calibration`, `modbus`, `modbus_handler`,
`options`, `shell`, `history`, `history_lib`, `ui_history`,
`ui_logic`, and the `tests` module.

## Files of interest in this port

| File | Role |
|------|------|
| `src/main.rs` | Executor entry, peripheral init, main `select6` loop, `handle_app_request` / `handle_modbus_frame` / `handle_history_tick` / `handle_shell_action`, RTC backup uptime |
| `src/drivers/uart.rs` | EXTI-wake `uart_session_task` + `shell_task` |
| `src/drivers/keypad.rs` | Event-driven keypad (EXTI idle / poll while held) |
| `src/drivers/tdc1000.rs`, `src/drivers/tdc7200.rs` | Minimal SPI drivers with `power_on()` / `power_off()` |
| `src/drivers/eeprom.rs` | 25LC1024 driver implementing `embedded_storage::Storage` |
| `src/drivers/hd44780.rs` | Async HD44780 4-bit parallel LCD driver |
| `src/drivers/deferred_display.rs` | Sync-fill / async-flush adapter so `Widget::render` stays synchronous |
| `src/drivers/cyrillic.rs` | 5×8 CGRAM bitmaps + Latin lookalike substitutions |
| `src/shell.rs` | Pure-Rust command parser + `parse_action` side-effect extractor |
| `src/options.rs` | `Options` bitfield (extended with `const_val` + `reserved`) |
| `src/modbus_handler.rs` | Modbus RTU register dispatcher (Options range raised to 0x0039) |

## See also

- `CLAUDE.md` for the original RTIC bug stack that motivated the port.
- `ARCHITECTURE.md` for system overview that still applies.
- `HISTORY_SYSTEM.md` for the ring layout (unchanged).
- `MODBUS_MAP.md` for the legacy register map (extended end-address noted above).
