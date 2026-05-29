# Zephyr architecture

System architecture of the uflowmeter firmware as it stands on the
`zephyr` branch. Read this **after** `docs/ZEPHYR_PORT.md` (the
porting story — what changed module-by-module from `rework/embassy`)
and **alongside** `docs/ARCHITECTURE.md` (the hardware overview from
the original RTIC era — pin map, sensor topology, mechanical layout
all still apply).

## 1. Hardware in one paragraph

STM32L151RCT6 (Cortex-M3, 256 KB flash, 32 KB SRAM, 8 MHz HSE crystal,
no LSE — only LSI ≈ 37 kHz on this board rev). Off-chip: TI TDC1000
ultrasonic AFE + TDC7200 time-to-digital converter sharing SPI2,
gated by a shared `EN` line; Microchip 25LC1024 EEPROM (128 KiB,
1024-byte pages) on SPI2 with its own CS + HOLD + WP; HD44780-class
2×16 character LCD on a private 4-bit GPIO bus + active-LOW power +
active-LOW backlight; 4-button keypad on PB6-PB9 with EXTI rising
edges; USART1 (PA9/PA10) carrying both Modbus RTU and an interactive
shell, with RS-485 DE/RE on a separate GPIO. PA8 outputs MCO (HSE/1
= 8 MHz) into the TDC reference inputs.

## 2. RTOS choices

| Concern | Decision | Why |
|---|---|---|
| Kernel | Zephyr 4.4 LTS | First-class L1 support: SoC dtsi, HAL, PM L1 power.c. |
| Language | C++20 | `CONFIG_STD_CPP20=y`. Lets us use `std::optional` / `std::atomic` / templates for `RingStorage`. |
| RTTI/exceptions | Off (Zephyr default) | Standard MCU constraint. |
| Heap | None in production code | `K_THREAD_STACK_DEFINE` / `K_MSGQ_DEFINE` at file scope only. Drivers hold static state. |
| Kernel-primitive wrapper | `lowlander/zpp` headers, vendored | Type-safe RAII for threads/mutexes/queues. Does NOT wrap GPIO/SPI/UART — those go via plain Zephyr C device APIs. |
| System clock | **Custom RTC sys_clock driver** (`src/timer/uflowmeter_rtc_timer.c`) | SysTick stops in STOP. We need a tick source that survives STOP so PM can program the next wake. RTC + LSI fits and survives VBAT. |
| Tickless | `CONFIG_TICKLESS_KERNEL=y` | Required to make STOP profitable — without it the kernel would wake on every fixed tick. |
| Power | `CONFIG_PM=y` with custom `pm_state_set` / `pm_state_exit_post_ops` strong-overrides | The upstream L1 power.c is marked `__weak`; we override to add LCD-power gating + clock restore tuned to our timing. |

## 3. Boot sequence

`src/main.cpp::main` runs once at the default priority of the **main
thread**. Every step is sequential:

1. **MCO out** (`mco::init`) — register-poke RCC and GPIOA for
   PA8 → MCO → HSE/1. Has to happen before the TDC chips see any
   driver init, since their state machine boots on the input clock.
2. **LCD init** (`drivers::hd44780::lcd().init()`) — 4-bit HD44780
   bring-up, ~50 ms blocking. Display shows "uflowmeter / press any
   key" so a watcher sees liveness immediately.
3. **EEPROM Options load** (`options::load`) — exits 25LC1024 deep
   power-down internally only for the read, then leaves the chip in
   standby. Dual-page CRC with primary/secondary fallback. Defaults
   on `BothCorrupt` or `IoError`.
4. **History rings load** (`history::init`) — same EEPROM,
   piggybacks on the awake window between options::load and the DP
   entry below.
5. **EEPROM → deep power-down** (`drivers::eeprom_enter_deep_power_
   down`). Chip stays here for the rest of normal operation; any
   future writer has to wake it under `eeprom_mutex`.
6. **UART transport up** (`drivers::uart::start`) — installs the
   USART1 IRQ handler, spawns the uart_worker thread, configures
   EXTI10 on PA10 as a STOP wake source.
7. **History tick up** (`history::start`) — spawns history_tick.
8. **Keypad init** (`drivers::keypad_init`) — configures PB6-PB9
   for `GPIO_INT_EDGE_BOTH`, arms one `k_work_delayable` per
   button for the 1 s/150 ms repeat scheme.
9. **Measurement up** (`measurement::start`) — spawns the
   measurement thread; first cycle fires after a 5 s sleep.
10. **PM arm** (`power::init`) — sets `pm_state_force` policy etc.
    From now on the idle thread will drop the chip into STOP when
    everyone is sleeping.
11. **Datetime init** (`datetime::init`) — reads RTC BKP0R magic;
    loads live calendar from TR/DR if valid, else applies 2024-01-01
    baseline. Must run after `power::init` because PM init touches
    RCC, but before any thread reads `datetime::now()` (which is why
    history_tick's first wake is 60 s out — gives us time).
12. **UI loop** — construct `MenuController`, draw the first frame
    under `lcd_mutex`, then enter the adaptive blocking loop.

After step 12 main never returns. The "boot" thread becomes the UI
thread.

## 4. Thread layout

Five application threads + three Zephyr-managed ones. All set up at
file scope (`K_THREAD_STACK_DEFINE` + `K_THREAD_DEFINE` /
`k_thread_create` from a start function called by main). No dynamic
spawning, no heap.

### Application threads

| # | Thread | Priority | Stack | Defined in | Blocks on |
|---|--------|---------:|------:|------------|-----------|
| 1 | **main / UI** | `K_PRIO_PREEMPT(0)` (default) | 2 KB (`CONFIG_MAIN_STACK_SIZE`) | `src/main.cpp` | `keypad_recv(timeout)` → `k_msgq_get(&key_msgq, …)`. Timeout: 150 ms during an open multi-field edit (blink animation); 2 s otherwise (live-value refresh). |
| 2 | **measurement** | `K_PRIO_PREEMPT(7)` | 1 KB | `src/measurement.cpp` | `k_sleep(K_SECONDS(5))` between cycles; `k_sem_take(&int_sem, K_MSEC(50))` per TDC7200 conversion. |
| 3 | **uart_worker** | `K_PRIO_PREEMPT(6)` | 2 KB | `src/drivers/uart.cpp` | `k_msgq_get(&rx_byte_msgq, &byte, K_USEC(1750))`. The timeout *is* the Modbus 1.75 ms inter-frame silence. |
| 4 | **history_tick** | `K_PRIO_PREEMPT(8)` | 1 KB | `src/history.cpp` | `k_sleep(K_SECONDS(60))`. |
| 5 | **keypad debounce/repeat** | runs on Zephyr's **system workqueue** | sysworkq's stack | `src/drivers/keypad.cpp` | One `k_work_delayable` per button — pressed ISR posts immediate event and arms +1 s; handler re-reads, re-posts, reschedules +150 ms if still held; release cancels. |

### Zephyr-managed threads

- **idle** — runs when everyone above is sleeping. Calls
  `pm_state_set(SUSPEND_TO_IDLE)` from `src/power.cpp`, which drops
  the chip into STOP (main regulator → low-power, PLL+HSE off,
  SLEEPDEEP+WFI). RTC + LSI keep counting.
- **system workqueue** — hosts the keypad work items above.
- **logging thread** — `CONFIG_LOG_MODE_DEFERRED=y`. `LOG_*` calls
  enqueue; this thread formats and emits via `printk` / `LOG_BACKEND_*`.

### Priority intent

- main (highest, 0) — owns the only display, must redraw promptly
  after input.
- uart_worker (6) — Modbus has a hard inter-frame timing budget;
  needs to preempt history_tick / measurement so a 1.75 ms silence
  window isn't missed mid-frame.
- measurement (7) — runs once every 5 s; tolerates being preempted
  by uart_worker but must finish a cycle quickly (the TDC INT
  semaphore times out at 50 ms).
- history_tick (8, lowest) — runs once per minute, no real-time
  constraint, can wait behind anything.

The ordering is also chosen so that an unexpected runaway in a
background subsystem cannot starve the UI.

## 5. ISR layer

| Source | Vector | What it does | Hand-off |
|---|---|---|---|
| USART1 RX (chained per-byte) | USART1 IRQ | Read each received byte, `k_msgq_put(&rx_byte_msgq, byte, K_NO_WAIT)`. | → uart_worker |
| PB6 / PB7 / PB8 / PB9 EXTI 6-9 | EXTI9_5 IRQ | Read pin, post `KeyEvent`, schedule debounce/repeat work item. | → main + sysworkq |
| PB0 EXTI 0 (TDC7200 INT) | EXTI0 IRQ | `k_sem_give(&int_sem)` — conversion done. | → measurement |
| RTC WUT (line 22) | RTC_WKUP IRQ | Custom sys_clock driver's `IRQ_DIRECT_CONNECT` handler — `sys_clock_announce(pending_ticks)`. | → kernel scheduler |
| PA10 EXTI 10 (UART wake) | EXTI15_10 IRQ | No handler logic. Its only job is to wake the chip from STOP so HSE+PLL restart before the next byte arrives; the wake byte itself is lost, peer retries. | — |

ISRs never touch the LCD or the EEPROM — all peripheral I/O that
might block is funnelled through threads.

## 6. Synchronization primitives

| Object | Type | Defined in | Producer | Consumer | Depth |
|---|---|---|---|---|---:|
| `key_msgq` | message queue | `drivers/keypad.cpp` | keypad EXTI ISRs | main | 8 × `KeyEvent` |
| `rx_byte_msgq` | message queue | `drivers/uart.cpp` | USART1 ISR | uart_worker | 512 × `uint8_t` |
| `int_sem` | binary semaphore | `drivers/tdc7200.cpp` | TDC7200 INT ISR | measurement | — |
| `lcd_mutex` | mutex | `drivers/hd44780.cpp` | main, measurement | (mutual exclusion) | — |
| `eeprom_mutex` | mutex | `drivers/eeprom_power.cpp` | main, uart_worker, history_tick | (mutual exclusion around DP-wake / op / DP-sleep) | — |
| `set_mutex` (datetime) | mutex | `datetime.cpp` | main (UI commit), uart_worker (shell SetDateUnix) | (mutual exclusion) | — |

## 7. Subsystem map

Who runs what, and which thread owns which kernel object.

```
                ┌────────────────┐
USART1 ISR ───► │ rx_byte_msgq   │ ───► uart_worker ─────► shell parse + dispatch
                │ (512 bytes)    │                        │                ▲
                └────────────────┘                        ▼                │
                                          modbus_handler ───────────────► options.cpp
                                                  │                     (load/save via
                                                  ▼                      eeprom_mutex)
                                            options::g_options
                                                  ▲
                          ┌───────────────────────┤
                          │                       │
keypad EXTI ─► key_msgq ──┴──► main / UI ────────►│
                                       │          │
                                       ▼          ▼
                                  MenuController  options.save_through_dp
                                       │
                                       ▼
                                  render() ─► lcd (lcd_mutex)
                                                ▲
                                                │
TDC1000+TDC7200 ◄── SPI2 ── measurement ────────┘
   (INT → int_sem)              │
                                ▼
                       latest_flow_m3h (std::atomic)
                                ▲
                                │
                       history_tick ── eeprom_mutex ── ring writes
                                ▲
                                │
                       datetime::now() ── set_mutex
```

Three pieces of cross-thread state, intentionally bounded:

- `options::g_options` — POD struct, single writer at a time under
  `eeprom_mutex`; readers (modbus_handler, render) accept torn reads
  because the consequences are cosmetic.
- `measurement::latest_flow_m3h` — `std::atomic<float>`, single
  writer (measurement), many readers (render, modbus_handler,
  history_tick). No mutex needed.
- `g_last_result` (history) — single-threaded, set + read from main
  only.

## 8. Power management

The interesting part of the port. The Cortex-M3's idle path takes us
into **STOP mode** (≈ 0.5 µA core) and back ≈ 5 s later in time for
the next measurement cycle.

### STOP entry (from idle thread)

`src/power.cpp::pm_state_set(SUSPEND_TO_IDLE, …)`:

1. Park the LCD if `drivers::lcd_user_wants_on == true` *and* it isn't
   already parked — cut VCC (PC0 HIGH) and backlight (PC5 HIGH).
2. PWR_CR: VOS to range 2, low-power regulator, PDDS=0 (STOP not
   STANDBY), CWUF.
3. SCB SCR.SLEEPDEEP = 1.
4. WFI.

### STOP exit (from same idle thread, post-WFI)

`pm_state_exit_post_ops`:

1. Restore clocks: HSE on, PLL on, switch SYSCLK back to PLL.
2. SCB SCR.SLEEPDEEP = 0.
3. If `drivers::lcd_user_wants_on`, re-run `lcd.init()` (~50 ms
   blocking — HD44780 loses state with VCC cycled). If the flag is
   false (15 s idle elapsed), skip re-init entirely → saves the wake
   latency and the backlight draw on every measurement cycle.
4. Return to the kernel; whichever thread's wake source fired runs
   next.

### Per-peripheral STOP behavior

| Peripheral | STOP behavior | Per-cycle action |
|---|---|---|
| LCD (HD44780) | VCC + backlight cut → loses state | Re-init on wake when user-on flag is set |
| EEPROM (25LC1024) | Held in deep-power-down all the time | None (already low-power) |
| TDC1000 / TDC7200 | `EN` line LOW between measurement cycles | None (already off) |
| USART1 | Clock dies in STOP, wakes via EXTI10 on PA10 | First byte after wake is lost; peer retries |
| RTC | Runs through STOP on LSI | Drives sys_clock + next-wake WUT |
| SPI2 | Clock dies; chip-selects already de-asserted | Brought up on first transfer post-wake |

### Wake sources

| EXTI line | Source | Effect |
|---|---|---|
| 6 | PB6 keypad Config | Wakes; ISR posts KeyEvent |
| 7 | PB7 keypad Enter  | same |
| 8 | PB8 keypad Down   | same |
| 9 | PB9 keypad Up     | same |
| 10 | PA10 USART1 RX    | Wakes only — byte lost |
| 22 | RTC WUT           | Wakes; sys_clock_announce drains pending_ticks |

### Accuracy caveat

LSI is uncalibrated and drifts ±10-15%. Both `k_uptime` and any
`k_sleep(N)` slip accordingly. Fine for a 5 s measurement cadence,
**not** fine for wall-clock — that's why the RTC TR/DR datetime
runs off the same LSI but is treated as approximate (the user can
adjust via shell `date set` or the UI's DateTime screen).

## 9. EEPROM concurrency contract

The 25LC1024 is the only persistently-stateful peripheral shared
across threads. Misuse breaks data; misuse during STOP-wake corrupts
silently. The discipline is:

> **Anything that calls `eeprom_read` / `eeprom_write` on the
> external 25LC1024 must, in order:**
>
> 1. `k_mutex_lock(&drivers::eeprom_mutex, K_FOREVER)`
> 2. `drivers::eeprom_exit_deep_power_down()`
> 3. perform reads / writes
> 4. `drivers::eeprom_enter_deep_power_down()`
> 5. `k_mutex_unlock(&drivers::eeprom_mutex)`

The chip silently ignores everything except 0xAB while in DP — so
step 2 is mandatory, not best-effort. The mutex covers the whole
sequence so a context switch can't strand the chip awake (~4 µA
continuous penalty) or split a multi-byte op across a DP cycle.

Three call sites obey this:
- `main` → `options::save_through_dp` on every committed edit
- `uart_worker` (modbus_handler.cpp) → `options::save_through_dp` on
  Modbus writes, and the shell `set_serial` path
- `history_tick` → ring writes via `with_eeprom_awake([&](dev){ … })`

The Options *load* at boot is exempt: it runs before the chip is
parked, so there's no DP to wake from. The history rings'
`init(eeprom)` rides the same pre-park window.

## 10. Clock topology

```
   HSE 8 MHz ─┬──────► RCC PLL  ──► SYSCLK (32 MHz) ──► AHB, APB1, APB2
              │
              └──► RCC MCO mux ──► PA8 (MCO out) ──► TDC1000 CLOCK
                                                      TDC7200 CLOCK

   LSI ≈ 37 kHz ──► RTC prescaler ──► ck_spre (1 Hz) ──► TR/DR
                                  └─► subseconds (~1024 Hz) ── sys_clock cycle source
                                  └─► WUT (RTC/16, ≈ 2312 Hz) ── kernel tick wake
```

- HSE is 8 MHz — the load-bearing fact for both the system PLL and
  the MCO-derived TDC reference. Commit `f20cffd` corrected an
  earlier assumption that the crystal was 24 MHz.
- LSI is *not* calibrated against HSE periodically — drift propagates
  to `k_uptime`. Adding RCC CIR calibration is on the wish list but
  not implemented.
- LSE crystal would replace LSI and make the calendar accurate; the
  current board lays out the pads but doesn't fit the crystal.

## 11. Memory map

### Flash (256 KB)

Application image. Current build: 61 052 B (23.29%). Linker script
is Zephyr's stock `arm/cortex_m` plus the SoC dtsi `flash0` node.

### SRAM (32 KB)

| Region | Size |
|---|---:|
| Application BSS + data | dominated by `options_scratch` (1 KB), driver static state, msgq + sem + mutex objects |
| Thread stacks | main 2 KB, uart 2 KB, measurement 1 KB, history 1 KB, sysworkq ~1 KB, logging ~768 B |
| Kernel state | ISR table, scheduler, PM state |
| **Total used** | 14 936 B (45.58%) |

### Backup registers (RTC BKP0R)

- `0x44544D45` ("DTME") magic — set by `datetime::init` after a
  successful TR/DR load + write. Cleared on cold VBAT, signals
  the calendar may be wrong.

### External EEPROM (128 KiB)

| Range | Owner | Contents |
|---|---:|---|
| `[0x0000 … 0x0400)` | options primary | 116 B Options + CRC, padded to one 1024 B page |
| `[0x0400 … 0x0800)` | options secondary | duplicate of primary, written second; survives mid-write power loss |
| `[0x1000 …)` (OFFSET_OF_STAT_PAGE) | history rings | Hour (8 656 B) → Day (4 480 B) → Month (496 B), chained via `SIZE_ON_FLASH` |
| rest | unused | reserved for future rings / per-meter logs |

The dual-page Options scheme tolerates power loss anywhere in the
1024 B write; the loader prefers a valid primary, falls back to
secondary, then to defaults.

## 12. Test surface

`tests/` builds standalone with the system C++ compiler. 62 cases
at HEAD:

| Module under test | Source | How |
|---|---|---|
| Modbus codec | `src/modbus.cpp` | pure logic, no Zephyr |
| Shell parser | `src/shell.cpp` | pure logic, `DateTimeProvider` injected as a function pointer |
| Calibration | `src/calibration.cpp` | pure logic |
| CRC primitives | `src/crc.cpp` | pure logic, plus a sanity check vs the Modbus CRC |
| History RingStorage | `src/history_lib.hpp` (header-only template) | RAM-backed `FakeEeprom` + `tests/shims/zephyr/{drivers/eeprom.h,logging/log.h}` so the header compiles without a real Zephyr tree |

Not yet host-tested (see `docs/ZEPHYR_PORT.md` for why):
- `src/options.cpp` dual-page protocol — would need the same fake-
  eeprom shim threaded through the file's Zephyr includes.
- `src/datetime.cpp` — splits cleanly into pure-logic helpers
  (to_timestamp / inc_*/dec_*) and an RTC TR/DR I/O half; the
  former is testable, the split hasn't happened.
- `src/history.cpp` thread loop, `src/measurement.cpp`,
  `src/power.cpp`, `src/timer/uflowmeter_rtc_timer.c`, anything
  under `src/drivers/` — hardware- or kernel-bound.

When the suite outgrows the standalone runner the natural upgrade
is `west twister` against `native_sim`.

## 13. Where to look next

- **For a feature port story** — `docs/ZEPHYR_PORT.md` (subsystem-
  by-subsystem RTIC → embassy → Zephyr deltas)
- **For the embassy attempt's lessons** — `docs/EMBASSY_PORT.md`
- **For the hardware itself** — `docs/ARCHITECTURE.md` (pin map,
  sensor topology, mechanical), `docs/HARDWARE_INTEGRATION.md`
- **For the measurement physics** — `docs/ULTRASONIC_FLOW.md`
- **For the wire formats** — `docs/MODBUS_MAP.md`,
  `docs/TDC1000_REGISTER_MAP.md`, `docs/TDC7200_REGISTER_MAP.md`
- **For the EEPROM layout in detail** — `docs/HISTORY_SYSTEM.md`
  (RTIC era — formulas now match the embassy-bug-fix `SIZE_ON_FLASH
  = 16 + 4*SIZE`)
- **For test wiring** — `tests/README.md`
- **For day-to-day commands** — `CLAUDE.md`
