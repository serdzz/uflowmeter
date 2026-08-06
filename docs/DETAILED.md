# Detailed change log — `rework/embassy`, 2026-08-01 … 2026-08-02

Everything done in this stretch of work, in enough detail to re-derive
the reasoning without re-reading the diffs. Twenty-two commits,
`d4105bf..07f79ab`, all on `rework/embassy`, none pushed.

The through-line: the embassy port booted and drove the LCD, but it did
not **measure** anything and its outputs were dead. Every register read
from both TDC chips came back `0x00`, every cycle logged
`tof: down=false up=false`, no M-Bus datagram ever went out, and the
4-20 mA loop was silent. By the end both chips configure and read back
correctly, the full C++ measurement arithmetic is ported and tested, all
three output paths are live, configuration can be uploaded over XMODEM,
and the button UI works.

The reference throughout is the C++ firmware that runs this same board,
at `/Users/sergejlepin/work/sandbox/UFlowMeter_c++`. Where a decision
here looks arbitrary, it almost always came from reading that source.

---

## 1. Build and CI hygiene

**`d4105bf` — `chore(build): clear warnings, apply rustfmt, lint the binary in CI`**

Starting point for everything else: the branch had accumulated warnings
that made real diagnostics hard to see.

- Cleared all compiler warnings across `drivers/*`, `lib.rs`, `main.rs`.
- Ran `cargo fmt` over the branch.
- **Closed a CI blind spot.** `make clippy` only ever linted the *host*
  library. The embedded binary — `main.rs`, every driver, all the
  hardware-facing code, which is most of what is actually shipped —
  was never linted at all. Added:

  ```make
  clippy: clippy-host clippy-embedded
  clippy-embedded:
  	cargo clippy --release --bin uflowmeter -- -D warnings
  ```

  plus a matching `clippy-embedded` job in `.github/workflows/ci.yml`.
- Added `rework/embassy` to the CI trigger branches, so the branch is
  actually built by CI rather than only locally.

Everything after this point had to pass `-D warnings` on *both* targets,
which caught several things early: `needless_range_loop`,
`manual_is_multiple_of`, `len_without_is_empty`, `drop_non_drop`.

---

## 2. The measurement path

This is the bulk of the work and the part that was actually broken.

### 2.1 STOP mode was killing the measurement mid-flight

**`778b273` — `fix(power): keep STOP out of the TDC measurement window`**

Both TDC chips are clocked from an 8 MHz reference the MCU puts out on
MCO/PA8, derived from HSE. STOP gates HSE. So if the low-power executor
dropped into STOP while a measurement was in flight, the reference
under both chips simply stopped — mid-conversion.

`Config::min_stop_pause` was 10 ms while `TDC_INT_TIMEOUT` is 50 ms, so
any cycle that waited on the INT line was a candidate. Raised
`min_stop_pause` to 100 ms with the invariant written down where
someone will see it:

> `min_stop_pause` must stay strictly greater than `TDC_INT_TIMEOUT`,
> otherwise the executor is free to enter STOP while parked on that
> timeout.

That relationship is now stated in the doc comment on both constants.

### 2.2 Five bugs between the drivers and the silicon

**`080f10e` — `fix(tdc): get real ToF readings — align both drivers with the C++`**

The decisive commit. Register reads returned `0x00` from both chips
*even though the EEPROM on the same SPI2 bus read back fine* — which
ruled out the bus and pointed at the chips or their control lines.

1. **TDC1000 RESET polarity — the one that mattered most.**
   The C++ `reset()` pulses RESET high and leaves it **low**; low is the
   *running* level. This port created PC6 at `Level::High`, commented
   "out of reset", and so held the chip in reset for its entire life.
   That is precisely why it answered `0x00` while the TDC7200 — which
   has no reset line — answered normally. The asymmetry was the clue.

   ```rust
   pub fn reset(&mut self) {
       self.res.set_high();
       self.res.set_low();   // running level is LOW
   }
   ```

2. **Command byte encoding, both chips.** Bit 6 = write, bit 7 =
   auto-increment, a read sends the bare address (`TDC1000_WRITE_BIT
   0x40` in TI's header; both C++ drivers frame it the same way).
   TDC1000 writes had bit 7 set instead. Multi-byte reads never set
   auto-increment, so `read_time1` read the same register three times
   rather than walking TIME1.

   > Worth knowing: the legacy Rust driver at `src/hardware/tdc7200.rs`
   > has read and write **inverted** relative to TI and to the C++.
   > Trusting it cost one full debug cycle here — the port was already
   > correct and got "fixed" into being wrong. It is not a reliable
   > reference.

3. **Configuration values were never on this board.**
   `Options::tdc1000_regs` / `tdc7200_regs` turned out to be a sound
   design — the C++ keeps one 20-byte `def_regs` blob split 10/10,
   which is exactly that layout. The problem was the data: the TDC1000
   half of this board's EEPROM is all zeros, and the TDC7200 half holds
   the ASCII `"1234567890"` stub that the legacy RTIC firmware wrote on
   every boot. Adopted the C++ defaults and added `regs_or_default()`,
   which falls back when the stored blob is all-zero or is that stub:

   ```rust
   const TDC1000_DEFAULT_REGS = [0x48,0x45,0x01,0x01,0x07,0xA0,0x1E,0x00,0x6A,0x03];
   const TDC7200_DEFAULT_REGS = [0x02,0x44,0x06,0x07,0xFF,0xFF,0xFF,0xFF,0x00,0x00];
   ```

4. **Channel select used the wrong bit.** The C++ toggles bit 2 (`0x04`)
   of CONFIG_2; this port used bit 0. Both "channels" were measuring the
   same acoustic path.

5. **INT wait was edge-triggered.** The C++ polls the *level*
   (`while (INT::IsSet())`). `wait_for_falling_edge()` misses a line
   that is already low by the time you get there, then burns the full
   50 ms timeout and reports a failure that never happened. Now
   `wait_for_low()`.

Also adopted the C++ ordering: reset-then-EN once at start-up rather
than power-cycling the chips per measurement, and per channel, clear
both chips' latched flags and settle ~2 ms before triggering.

**Result on hardware:** both chips read back what was written
(`0x48/0x45/0x01/0x01` and `0x02/0x44/0x07`) and produce stable ToF
pairs.

### 2.3 Reproducible builds

**`8f1d71c` — `build: track Cargo.lock`** · **`f5ab53b` — `build: pin the embassy fork by rev instead of branch`**

`Cargo.lock` was gitignored and the embassy dependencies were pinned to
a *branch* of the `serdzz/embassy` fork — so any push to that branch
silently changed what this firmware was built from. Committed the
lockfile, dropped the `.gitignore` entry, and pinned every embassy crate
to `rev = "abb8bf6"`. Bisecting a hardware regression is only meaningful
once this is true.

### 2.4 Full parity with the C++ measurement

**`01e5b29` — `feat(measure): match the C++ measurement path end to end`**

With the chips talking, the numbers coming out were still meaningless
(`down=2055 up=2056` — raw counter ticks). Three gaps, all in the same
loop:

**Calibrated time of flight.** The port read TIME1 alone and handed that
24-bit counter straight to the flow `Calculator`. It ignored
CLOCK_COUNT, both calibration counters, and the four other stops the
chip is configured to report. The C++ reads all 39 bytes from `0x10` and
reconstructs picoseconds:

```
dn   = CALIBRATION2 - CALIBRATION1
tof  = (TIME1 - TIMEn+1) * 140625 / dn   (+ fractional refinement)
     + CLOCK_COUNTn * 125000
```

Those constants belong to *this meter's* configuration: `CONFIG2 = 0x44`
sets CALIBRATION2_PERIODS = 10, so the calibration count spans nine
periods of the 8 MHz reference (125 000 ps each), and 140 625 is
1 125 000 / 8, with the `* 8` steps recovering the bits integer division
would otherwise drop. Averaged over NUM_STOP + 1 = 5 stops.

Ported into **`src/tdc_lib.rs`** as pure no-HAL code, per the `*_lib.rs`
convention — `decode_tof`, `average_stops`, `stop_numbers`. Eight host
tests cover the reference arithmetic, per-stop field offsets, remainder
rounding, and the guards against a short block, a bad stop count, and an
uncalibrated block where `CALIBRATION1 == CALIBRATION2` would divide by
zero.

**Second transducer pair.** `Options` has always carried two calibration
tables (`zero1/v1x/k1x` and `zero2/v2x/k2x`) and the C++ measures both
pairs, scoring each against its own table. Added
**`src/drivers/sensor_mux.rs`** — PB3 enable, PB4/PB5 address, with
`Off=0 / One=1 / Two=3` written as one 3-bit field exactly as in
`Hardware::SensorMux` — and made the loop cover both sensors per cycle.
Those three pins were previously just parked low.
`options_to_calib_table()` now takes a sensor index.

**Units at the Calculator boundary.** `decode_tof` yields picoseconds;
`Calculator` wants nanoseconds. The C++ divides by 1000.0f right there
(`Src/measure.cpp:188`); we were passing the value through unscaled — a
1000× error that would have looked like a calibration problem forever.

**Watchdog.** Started the IWDG at ~14 s (matching the C++ prescaler 128 /
reload 4095) and pet it from the measurement loop rather than a task of
its own. A dedicated pet task keeps the dog quiet with everything else
wedged; *this* task waking on schedule and completing an SPI cycle is
the liveness signal actually worth guarding.

### 2.5 Diagnostics and log noise

**`a1cdccc` — `fix(tdc): show the fields a failed decode turned on`**

A failed decode used to say nothing about why. It now dumps TIME1,
CLOCK_COUNT1 and both CALIBRATION registers. That distinguishes two very
different failures: an all-zero *CALIBRATION* means the chip never
completed a calibration cycle (expected with no echo returning), while
an all-zero *block* points at the auto-increment bulk read itself.

**`eb6b7cf` — `fix(measure): stop flooding the log when there is no signal`**

With no transducers attached every 5 s cycle fails identically, and the
RTT log became unusable for anything else. An unchanged failure now
repeats once per `QUIET_CYCLES = 12` cycles — once a minute — while any
*change* in the failure reports immediately.

---

## 3. Reporting outputs

Three paths that the C++ has and the port had lost.

**`61c2126` — `feat(mbus): broadcast the M-Bus datagram again`**

Re-enabled `pub mod mbus;` and wired `send_mbus_datagram()` into the
history tick. The C++ reschedules its communication process with
`Parameters::MBUS_PROCESSING_TIMEOUT = 5*60` seconds after each
datagram, so `MBUS_PERIOD_TICKS = 5` against the 60 s tick matches it
exactly. Broadcast-only — nothing replies — so a full TX queue just
means the line is busy and this round is dropped rather than stalling
the main loop.

**`d6baf75` — `feat(measure): report a window mean instead of the latest cycle`**

The meter was reporting whatever the most recent 5 s cycle happened to
produce. The C++ keeps a window mean (`average_m3ph_`). Added
**`src/average_lib.rs`** — a 10-slot `AverageBuffer` that divides by the
*populated* count, so the reading is sane before the window fills — plus
`Calculator::near_zero_filter()`. The C++ applies that dead-band filter
a second time to the window average (`Src/measure.cpp:202`), because
averaging several small non-zero readings can drift back above zero;
that second application is reproduced here.

Deliberately **not** ported: the C++ seeds this buffer from RTC backup
registers across a reset. Left out as an open question rather than
copied blind.

**`d9df442` — `feat(analog): emit the 4-20 mA loop frame`**

Ported `Calculator::to_analog()` and the `AA 55 <u16 LE>` serial frame.
The conversion is reproduced verbatim including its oddities — the C++
reads an `AnalogMaxValue` parameter and falls back to `Vmax`, but
`Options` has no such field, so this is always the fallback path; the
`* 0.3125` scale and the `+ 800` offset are kept as-is. Faithful-port
first; if the arithmetic is wrong it is wrong the same way the shipping
firmware is wrong, which is the debuggable state.

---

## 4. Configuration upload over XMODEM

The board's EEPROM has no valid TDC registers and no calibration — §2.2.
`regs_or_default()` papers over that at boot, but there has to be a way
to actually *write* the values. The C++ does it over XMODEM from the
shell. This is that path.

**`c0f6334` — `feat(xmodem): add the XMODEM-CRC receive state machine`**

**`src/xmodem_lib.rs`** — pure, no transport, no HAL. `XModemReceiver`
fed one byte at a time returning a `Response`; SOH/STX/EOT/CAN handling,
`'C'` poll for CRC mode, CRC-16/XMODEM (poly `0x1021`, init 0).

One deliberate divergence from the C++: it **splits the write cursor
from the packet position**. The C++ conflates them and has an
off-by-two on multi-packet transfers as a result. 11 tests, including
two packets arriving back to back.

**`dc6f432` — `feat(xmodem): wire the upload path through to Options and EEPROM`**

- **`src/upload_lib.rs`** — decodes both blobs. `CALIBRATION_BLOCK = 56`
  (14 little-endian f32), `TDC_BLOCK = 20` (10 TDC1000 + 10 TDC7200
  registers). The important detail: the C++ `tCallibrationTable` is
  `{ dTOF0, data[3] }` where each entry is `{ V, K }` — **V and K
  alternate per point** rather than being grouped. `Options` stores them
  as separate fields, which is exactly where a `memcpy`-shaped port
  silently transposes them. A test pins the interleaving. 7 tests.
- **`src/shell.rs`** — `set_calibration` and `set_configuration`
  commands, `ShellAction::Upload(kind)` and
  `ShellAction::ApplyUpload(kind, data)`. Tested that `process_line`
  recognises them — otherwise the line falls through to the Modbus
  framer.
- **`src/drivers/uart.rs`** — the transfer runs **inline in
  `run_session`**, not via `shell_task`. The prompt and the first `'C'`
  poll have to reach the line with no other writes in between, or the
  shell's own output races the poll. So `handle_read` returns the
  request upward instead of forwarding the line. Once the sender starts,
  the `'C'` poll is masked — otherwise it injects stray bytes into the
  packet stream. 500 ms poll interval, 15 s deadline.
- Result goes to the main loop as `ShellAction::ApplyUpload`, is applied
  to `Options`, and persisted to EEPROM.

---

## 5. LCD, keypad and UI

Two independent defects that both presented to the user as "the buttons
do not work". In each case the obvious-looking change is the one that
*re*introduces the bug, which is why they are also recorded in
`CLAUDE.md`.

**`b831358` — `fix(lcd): busy-wait the microsecond delays, 5x faster render`**

The HD44780 driver awaited `Timer::after` for its sub-millisecond
timing. Under the RTC-backed low-power time driver an await costs far
more than the delay it asks for — programming an alarm means dropping
RTC write protection and waiting on synchronisation flags — and the
driver needs ~170 of them per frame. A full 2×16 render measured
**96-100 ms with awaits against 17-31 ms without**. The main loop
therefore sat ~100 ms behind every key event, which felt exactly like
dropped presses. Microsecond delays now busy-wait via
`cortex_m::asm::delay`; millisecond ones (power-up, clear) still yield.
The trade-off — a render blocks the executor for a few ms instead of
yielding between nibbles — is the better end of the deal, since nothing
else needs to run inside a render.

**`70d9be8` — `test(examples): add a minimal keypad + LCD example`**

`examples/buttons.rs` drives nothing but the four button pins and the
LCD — no executor integration, no idle timeout, no measurement task, no
menu. It exists to answer "is it the driver or the firmware around it"
in a single flash, and it is what produced the measurement below.

**`9bed4ad` — `fix(keypad): detect presses by sampling, not by awaiting EXTI edges`**

An earlier commit had moved the keypad from polling to pure event-driven
for idle power saving: `select4` over four `wait_for_falling_edge()`
futures, one press per edge. On hardware that **loses roughly nineteen
presses in twenty**. Measured with `examples/buttons.rs`: the
edge-driven build caught ~1 press in 20; sampling the same pins every
20 ms caught **286 of 286**, with the level trace showing clean
transitions in both directions throughout. The pins and pull-ups are
fine — awaiting the edges is what drops them. (The C++ `Keyboard::read`
has always polled.)

The driver now uses edges **only to wake from STOP** and detects presses
by sampling, at two rates so that costs no power:

- *idle* — race the four edges against a 150 ms backstop timer; nothing
  is read from the winner, both just mean "sample now". 150 ms is
  deliberately **above `min_stop_pause` (100 ms)** so STOP still happens
  between samples.
- *active* — a key is down or one was released less than 400 ms ago:
  sample every 20 ms, so presses, releases and repeat timing are all
  caught and a burst stays in this mode start to finish.

**`c80c41b` — `fix(lcd): park the data lines before cutting panel power`**

Dropping the supply on PC0 while RS/RW/E/D4-D7 are still driven feeds
current through the panel's protection diodes and leaves the controller
in a state it does not recover from on the next power-up. The display
then stays frozen no matter what is written to it — which, after the
first 15 s idle timeout, looks exactly like dead buttons. Added
`Hd44780::park()`, driving every line low before the gate opens. The C++
does the equivalent in `Lcd::shutdown()` by reconfiguring the pin list
to inputs with pull-downs (`hardware/lcd.cpp:96-99`).

**`bce6667` — `fix(ui): label the reset button for what it does`**

**`9e14ffe` — `feat(ui): edit mode with a blinking value for the ВКЛ/ВЫКЛ screens`**

The value pickers (ВКЛ/ВЫКЛ, тип связи, тип датчика) had no visible edit
state — nothing distinguished "showing a value" from "changing a value".
Added `editable` per picker, `is_editing()`, and `value_blanked()`,
which blanks the value line during the hidden half of the blink cycle.
The C++ gets the same effect by handing the whole label to
`set_blinked_label` (`ui/editbox.cpp:15`).

**`a1e3e2a` — `feat(ui): refresh a visible screen on a timer, so the clock ticks`**

The blink animation advances one frame per `render()` call, so key
events alone left it frozen mid-cycle — and the DateTime seconds never
moved. The main `select6` now carries a timer arm whose deadline depends
on state: `BLINK_FRAME` (100 ms, six frames ≈ 600 ms cycle) while
editing, `LIVE_REFRESH` (500 ms) for a visible non-editing screen, and
`pending()` — never resolves — when nothing is on screen, so an idle
meter is not woken to redraw a dark panel.

**`80bbc26`, `07f79ab` — documentation**

Recorded in `CLAUDE.md`: flashing must use `--speed 500` (at the default
SWD rate this board fails to connect reproducibly, in four different
ways depending on what the firmware is doing); the hold-RESET fallback
for when even that fails, and why `--connect-under-reset` is not the
answer here (NRST is not wired to the probe); and both UI defects above.

---

## 6. Tests

**312 host tests pass**, up from 264 at the start of this work — 48
added. All of them are pure, no-HAL, and run on the host per the
`*_lib.rs` convention:

| Module | Tests | Covers |
|---|---|---|
| `tdc_lib_tests` | 8 | ToF reference arithmetic, per-stop field offsets, remainder rounding, short block / bad stop count / divide-by-zero guards |
| `xmodem_lib_tests` | 11 | CRC mode negotiation, packet assembly, back-to-back packets, EOT/CAN, retry accounting |
| `upload_lib_tests` | 7 | Both blob layouts, in particular the V/K interleaving |
| `average_lib_tests` | 6 | Partial-window mean, wraparound, pair pushes |
| `tests` (calibration) | +4 | `to_analog`, `near_zero_filter` |
| `shell` | +4 | The two new upload commands through `process_line` |

Green across the board: `cargo build --release`, `make clippy` and
`make clippy-embedded` (both `-D warnings`), `cargo fmt --check` clean,
312/312 tests.

---

## 7. Verified on hardware vs. not

**Verified.** Both TDC chips configure and read back correctly. TIME1 is
live. The measurement loop runs on schedule and pets the watchdog. The
UI renders, the buttons work, the clock ticks, edit mode blinks. The
firmware boots and the measurement cycle is unchanged after the UART
rework, so the XModem wiring did not disturb the Modbus/shell path.

**Not verified, and cannot be on this bench.**

- **Time of flight.** `CLOCK_COUNT` and `CALIBRATION` stay zero and the
  TDC1000 reports `ERR_NO_SIG (0x02)` — because there are **no
  transducers attached at all**. The decode arithmetic is covered by
  tests against the C++ reference; that it produces correct *physical*
  readings is unverifiable until sensors exist.
- **Flow output** reads 0.0: `const_val` is unset on this uncalibrated
  board.
- **XMODEM end to end.** The protocol logic has 11 tests, but an actual
  transfer needs a serial peer running an XMODEM sender, which this
  bench does not have. No byte has crossed the wire.

---

## 8. Load-bearing — do not "fix" in isolation

- `min_stop_pause` (100 ms) **must** stay greater than `TDC_INT_TIMEOUT`
  (50 ms). Lowering it puts STOP back inside the measurement window and
  kills the 8 MHz reference mid-conversion.
- `IDLE_POLL` (150 ms) in the keypad **must** stay above
  `min_stop_pause`. A shorter tick pins the meter awake.
- The keypad must not go back to treating EXTI edges as presses. It
  loses ~95 % of them; `examples/buttons.rs` will show this in one flash.
- `Hd44780::park()` must be called before the panel supply is cut.
- The HD44780 microsecond delays must not be converted to
  `Timer::after`. That is a 5× render regression and it reads as dropped
  key presses.
- `src/hardware/tdc7200.rs` (legacy, RTIC-era) has read/write inverted
  and is not a reference for anything.

---

## 9. Open items

- **`diag_cycles`** in `main.rs` still dumps three cycles of register
  readbacks on every boot. It was bring-up scaffolding; with no
  transducers coming, one cycle would be enough as a chip-comms check.
  Not removed pending a decision.
- **`AverageBuffer` RTC backup seeding** (C++ DR6/DR7) deliberately not
  ported — see §3.
- **XMODEM** needs one end-to-end run against a real sender.
- Text is handled as **UTF-8** throughout; the C++ CP1251 conversion is
  intentionally not ported. `DeferredDisplay` decodes UTF-8 natively
  into the CGRAM Cyrillic glyphs.

---

## 10. Commit index

| Commit | Subject |
|---|---|
| `d4105bf` | chore(build): clear warnings, apply rustfmt, lint the binary in CI |
| `778b273` | fix(power): keep STOP out of the TDC measurement window |
| `080f10e` | fix(tdc): get real ToF readings — align both drivers with the C++ |
| `8f1d71c` | build: track Cargo.lock |
| `f5ab53b` | build: pin the embassy fork by rev instead of branch |
| `01e5b29` | feat(measure): match the C++ measurement path end to end |
| `61c2126` | feat(mbus): broadcast the M-Bus datagram again |
| `d6baf75` | feat(measure): report a window mean instead of the latest cycle |
| `a1cdccc` | fix(tdc): show the fields a failed decode turned on |
| `eb6b7cf` | fix(measure): stop flooding the log when there is no signal |
| `d9df442` | feat(analog): emit the 4-20 mA loop frame |
| `c0f6334` | feat(xmodem): add the XMODEM-CRC receive state machine |
| `dc6f432` | feat(xmodem): wire the upload path through to Options and EEPROM |
| `b831358` | fix(lcd): busy-wait the microsecond delays, 5x faster render |
| `bce6667` | fix(ui): label the reset button for what it does |
| `70d9be8` | test(examples): add a minimal keypad + LCD example |
| `9bed4ad` | fix(keypad): detect presses by sampling, not by awaiting EXTI edges |
| `c80c41b` | fix(lcd): park the data lines before cutting panel power |
| `9e14ffe` | feat(ui): edit mode with a blinking value for the ВКЛ/ВЫКЛ screens |
| `80bbc26` | docs: record the SWD speed and the two UI defects |
| `a1e3e2a` | feat(ui): refresh a visible screen on a timer, so the clock ticks |
| `07f79ab` | docs: note the hold-RESET fallback for flashing |

New files: `src/tdc_lib.rs`, `src/average_lib.rs`, `src/xmodem_lib.rs`,
`src/upload_lib.rs`, `src/drivers/sensor_mux.rs`, `examples/buttons.rs`,
plus the four matching `*_tests.rs`.

Net: **30 files, +3839 / −296**.
