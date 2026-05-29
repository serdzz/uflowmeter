# Zephyr port — `zephyr` branch

This document covers the port from `rework/embassy` (Rust + Embassy)
to **Zephyr RTOS in C++20**. Only the differences against the prior
Rust ports are listed here — anything still accurate (Modbus register
map, history ring layout, TDC register details) carries over from
`docs/ARCHITECTURE.md`, `docs/HISTORY_SYSTEM.md`,
`docs/MODBUS_MAP.md`, `docs/TDC*_REGISTER_MAP.md`, and
`docs/EMBASSY_PORT.md`.

## Why the port

The embassy port hit a single hard blocker that ended development:
**STM32L1 has no upstream Zephyr (or embassy) power-management
support**. embassy's `low-power` feature is gated on L4/L5/U5/U3/WB/
WL/U0 only; Zephyr's `CONFIG_PM` requires `HAS_PM`, which is also
not declared for stm32l1x. Both runtimes need either a fork patch
or a custom bypass to get STOP mode working on this chip.

The Zephyr port chose to accept the gap (build with `CONFIG_PM=n`,
ship as a working RUN-mode firmware first) and re-attack STOP as a
separate concern. The full subsystem set is otherwise reimplemented
end-to-end with all the user-visible behavior of the embassy version
preserved.

## Build & flash

```sh
source ~/zephyrproject/.venv/bin/activate
export ZEPHYR_BASE=~/uflowmeter/zephyr
export ZEPHYR_SDK_INSTALL_DIR=~/zephyr-sdk-1.0.1
west build -b uflowmeter_v1 . -p always
```

Tested against **Zephyr 4.4.99 + SDK 1.0.1** on macOS arm64. The
4.1.99 LTS build also worked with SDK 0.17.1 (commit `133cc93`); the
4.4 jump shrank the binary from 81 KB to 52 KB flash thanks to
GCC 14.3 + newer libstdc++ dead-code elimination.

Final size: **52 KB / 256 KB flash (20%)**, **12 KB / 32 KB SRAM (38%)**.

Flash:

```sh
west flash --runner openocd

# or via system openocd directly (matches what worked in this session):
/opt/homebrew/bin/openocd \
  -f boards/uflowmeter/uflowmeter_v1/support/openocd.cfg \
  -c init -c 'reset init' \
  -c 'flash write_image erase build/zephyr/zephyr.hex' \
  -c 'reset run' -c shutdown

# or probe-rs (the runner the Rust era used):
probe-rs download --chip STM32L151RC --binary-format hex build/zephyr/zephyr.hex
```

Host-side tests (no Zephyr SDK needed):

```sh
cmake -S tests -B build-tests
cmake --build build-tests
./build-tests/uflowmeter_tests   # → "46 passed, 0 failed (of 46)"
```

## Repo layout

```
.
├── CMakeLists.txt            # find_package(Zephyr) + app sources
├── prj.conf                  # Kconfig overrides
├── Kconfig                   # app-specific symbols (empty)
├── west.yml                  # manifest (pins Zephyr 3.7 LTS — workspace
│                             # already at ~/uflowmeter/zephyr 4.4.99)
├── sample.yaml               # twister metadata
├── boards/uflowmeter/uflowmeter_v1/
│   ├── board.yml             # HWMv2 board descriptor
│   ├── uflowmeter_v1.dts     # device tree — source of truth for pinmap
│   ├── uflowmeter_v1_defconfig
│   ├── Kconfig.uflowmeter_v1
│   ├── board.cmake           # runners (stm32cubeprogrammer / openocd)
│   └── support/openocd.cfg   # ST-Link + stm32l1 target (single-bank)
├── dts/bindings/
│   ├── display/uflowmeter,hd44780.yaml
│   └── sensor/uflowmeter,tdc{1000,7200}.yaml
├── include/zpp/              # vendored lowlander/zpp headers (Apache-2.0)
├── src/
│   ├── main.cpp              # boot + main thread (UI dispatcher)
│   ├── calibration.{hpp,cpp} # pure-logic Calculator / 4-zone apply_ratio
│   ├── datetime.{hpp,cpp}    # RTC TR/DR + BCD + per-field arithmetic
│   ├── history.{hpp,cpp}     # 3 ring buffers + 60 s tick thread
│   ├── history_lib.hpp       # template RingStorage
│   ├── measurement.{hpp,cpp} # 5 s measurement thread
│   ├── modbus.{hpp,cpp}      # codec (parse/build/CRC16-Modbus)
│   ├── modbus_handler.{hpp,cpp} # register R/W (Options + flow)
│   ├── options.{hpp,cpp}     # 116-byte Options struct + dual-page CRC
│   ├── power.{hpp,cpp}       # pm_state_set hooks (currently dead — see below)
│   ├── shell.{hpp,cpp}       # text command parser
│   ├── drivers/
│   │   ├── cyrillic.{hpp,cpp}    # 70-entry FONT + Latin lookalike table
│   │   ├── eeprom_power.{hpp,cpp}# DP wake/sleep + eeprom_mutex
│   │   ├── hd44780.{hpp,cpp}     # LCD 4-bit driver
│   │   ├── keypad.{hpp,cpp}      # EXTI-driven 4-button input
│   │   ├── tdc1000.{hpp,cpp}     # AFE driver
│   │   ├── tdc7200.{hpp,cpp}     # TDC driver + INT semaphore
│   │   └── uart.{hpp,cpp}        # USART1 IRQ + Modbus/shell dispatch
│   ├── timer/
│   │   └── uflowmeter_rtc_timer.c  # custom sys_clock (RTC subseconds)
│   └── ui/
│       ├── screen.hpp              # ScreenId / MenuId enums
│       ├── events.{hpp,cpp}        # UiEvent + INPUT_KEY_* mapping
│       ├── app_request.hpp         # AppRequest enum
│       ├── menu_list.{hpp,cpp}     # ring buffer
│       ├── menu_controller.{hpp,cpp} # state machine + edit modes
│       └── render.{hpp,cpp}        # 2-row painter + UTF-8 + CGRAM
├── tests/                    # host-side test harness (~80-line fwk)
│   ├── CMakeLists.txt
│   ├── framework.{hpp,cpp}
│   ├── test_modbus.cpp
│   ├── test_shell.cpp
│   └── test_calibration.cpp
└── docs/                     # this directory
```

## Thread layout

Five threads + the main thread; all preemptible. Cross-thread channels
are `std::atomic<float>` (one-writer flow publication), `k_msgq`
(keypad bytes, UART RX), and `k_mutex` (LCD + EEPROM serialization).

| Thread | Priority | Stack | Wake source |
|---|---|---|---|
| `main` (UI) | `K_PRIO_PREEMPT(0)` | 2 KB (default) | `keypad::keypad_recv` (2 s timeout idle, 150 ms while editing) |
| `measurement` | `K_PRIO_PREEMPT(7)` | 1 KB | `k_sleep(K_SECONDS(5))` |
| `history_tick` | `K_PRIO_PREEMPT(8)` | 1 KB | `k_sleep(K_SECONDS(60))` |
| `uart_modbus` | `K_PRIO_PREEMPT(6)` | 2 KB | `k_msgq_get(rx_byte_msgq, K_FOREVER)` idle, `K_USEC(1750)` mid-frame |
| TDC7200 INT ISR | (ISR ctx) | — | EXTI on PB0 falling edge → `k_sem_give(int_sem)` |
| Keypad ISRs (×4) | (ISR ctx) | — | EXTI on PB6/7/8/9 falling edge → debounce → `k_msgq_put` |

## Shared resources

| Resource | Owner / mutex | Notes |
|---|---|---|
| HD44780 LCD | `drivers::lcd_mutex` | Main + measurement both write (main row 0, measurement publishes via atomic that main renders) |
| EEPROM (25LC1024) | `drivers::eeprom_mutex` | UI Options save + Modbus Options save + history ring writes all acquire it around the wake/op/sleep cycle |
| `options::g_options` | none (single-writer) | UI writes from main; Modbus writes from uart_modbus. Two-writer race window microseconds wide; symptom would be a torn `options.serial_number` u32 — not observed |
| `measurement::latest_flow_m3h` | `std::atomic<float>` | naturally lock-free on ARMv7-M for aligned 32-bit access |
| RTC TR/DR + BKP0R | `datetime::set_mutex` (writes only) | reads are lock-free (SSR→TR→DR shadow coherency from hardware) |

## Subsystem status

| Subsystem | Status | Source ref |
|---|---|---|
| HD44780 4-bit LCD | ✓ Driver done | `src/drivers/hd44780.{hpp,cpp}` |
| Cyrillic CGRAM | ✓ FONT table + Latin-lookalike substitution; per-frame slot allocator | `src/drivers/cyrillic.{hpp,cpp}` + `src/ui/render.cpp` |
| 4-button keypad | ✓ EXTI-driven with 20 ms debounce, single-press (repeat not ported) | `src/drivers/keypad.{hpp,cpp}` |
| 25LC1024 EEPROM | ✓ Via Zephyr `atmel,at25` driver + custom deep-power-down | `src/drivers/eeprom_power.{hpp,cpp}` |
| Options (116-byte) | ✓ Byte-exact with embassy `modular_bitfield` layout; dual-page CRC | `src/options.{hpp,cpp}` |
| TDC1000 + TDC7200 | ✓ Drivers + 5 s measurement thread; INT via `k_sem` | `src/drivers/tdc{1000,7200}.cpp` + `src/measurement.cpp` |
| Calibration | ✓ 4-zone piecewise-linear (12 host tests pass) | `src/calibration.{hpp,cpp}` |
| History rings | ✓ Three EEPROM rings (Hour/Day/Month) + 60 s tick. **Fixed embassy SIZE_ON_FLASH bug.** | `src/history_lib.hpp` + `src/history.cpp` |
| UI state machine | ✓ All 19 screens, edit modes, DateTime + History pickers, Version easter-egg, blink | `src/ui/*` |
| Modbus RTU | ✓ Codec (12 tests) + handler + USART1 IRQ transport | `src/modbus*.cpp` + `src/drivers/uart.cpp` |
| Shell on USART1 | ✓ Dual-buffer w/ Modbus on same wire | `src/shell.{hpp,cpp}` + `src/drivers/uart.cpp` |
| RTC datetime persistence | ✓ TR/DR + BKP0 magic; survives reset with VBAT | `src/datetime.{hpp,cpp}` |
| Custom RTC sys_clock | ✓ Replaces SysTick (subseconds at 1024 Hz, WUT for tickless) | `src/timer/uflowmeter_rtc_timer.c` |
| STOP-mode PM | ✓ Enabled. Zephyr 4.4 has upstream L1 power.c; our `src/power.cpp` strong symbols override the upstream `__weak` versions to add LCD power-gating around STOP entry. | `src/power.{hpp,cpp}` + `boards/.../Kconfig.uflowmeter_v1` (`select TICKLESS_CAPABLE`) |
| UART STOP-wake | ✓ Done — PA10 EXTI line 10 armed as STOP wake source via Zephyr's `stm32_gpio_intc_*` API. USART driver retains pinctrl ownership; EXTI samples the pad regardless of GPIO mode. First byte after wake is still lost (USART clock dead while chip rides through STOP→clock-restore window ~50 ms); Modbus retry handles it. | `src/drivers/uart.cpp` `pa10_wake_cb` + the arming block in `start()` |
| MCO output on PA8 | ✓ Done — direct CMSIS register access in `src/mco.{hpp,cpp}` (PA8 AF0 + RCC->CFGR.MCOSEL=HSE + MCOPRE=/1 = 8 MHz). Called from main early, before measurement::start. | `src/mco.{hpp,cpp}` + `main.cpp` |
| Host test harness | ✓ Custom 80-line framework; 46/46 pass for modbus + shell + calibration | `tests/` |

## Power management

**STOP mode is enabled.** `prj.conf` has `CONFIG_PM=y` +
`CONFIG_TICKLESS_KERNEL=y`. Zephyr's PM policy chooses
`PM_STATE_SUSPEND_TO_IDLE` (mapped to STOP via the
`stop_mode` node in our DTS) whenever the next k_sleep deadline
is ≥ `min-residency-us = 2000`. Our custom RTC sys_clock driver
programs WUT for the wake.

### How it works (Zephyr 4.4)

Zephyr 4.4 has upstream STM32L1 PM support
(`soc/st/stm32/stm32l1x/power.c`, by Oleh Kravchenko 2025). Provides
`__weak pm_state_set` + `pm_state_exit_post_ops` that:

- enter STOP via `LL_PWR_SetPowerMode(LL_PWR_MODE_STOP)` +
  `LL_PWR_SetRegulModeLP(LL_PWR_REGU_LPMODES_LOW_POWER)` +
  `LL_LPM_EnableDeepSleep()` + `k_cpu_idle()`
- restore the system clock on wake via `stm32_clock_control_init(NULL)`

`src/power.cpp` provides **strong-symbol overrides** for both
`pm_state_set` and `pm_state_exit_post_ops`. Strong wins over weak at
link time — verified via `arm-zephyr-eabi-addr2line` on the linked
ELF that both symbols resolve to our source file.

Our overrides keep the same CMSIS sequence as upstream's L1 power.c
PLUS:

- cut LCD backlight + VCC before WFI (under lcd_mutex)
- restore LCD VCC + re-run `lcd().init()` + restore backlight after
  clock restoration

LCD goes from "shows last frame in standby" to "completely powered
down (HD44780 idle current ≈ 0.5-1 mA saved)" at the cost of a ~50 ms
re-init on every STOP exit. The `min-residency-us = 2000` threshold
in the DT power-states keeps Zephyr from entering STOP for k_sleeps
shorter than 2 ms (where the LCD re-init dwarfs the savings).

### Earlier blocker (historical)

Zephyr 4.1.99 did NOT declare `HAS_PM` for stm32l1x. `CONFIG_PM=y`
failed the Kconfig dependency check. We shipped commit `133cc93`
with `CONFIG_PM=n` as a 4.1-targeted workaround. The 4.4 jump
(commit `9aac675`) inherited that workaround silently — this commit
unwinds it now that the upstream gap is closed.

### Earlier blocker (different angle: embassy)

The embassy port's `low-power` feature cfg-gates only L4/L5/U5/U3/
WB/WL/U0 — no L1. They patched their `embassy-stm32` fork to add
L1 to the gates. Zephyr 4.4 doing the equivalent upstream is the
clean win here.

### Custom sys_clock driver

`src/timer/uflowmeter_rtc_timer.c` (the custom RTC-based system
clock) is what makes tickless+PM work on this chip. SysTick stops in
STOP mode; the RTC keeps running on LSI. The driver:

- counts at 1024 Hz via the RTC subseconds counter (HW config in
  init: PREDIV_A=35, PREDIV_S=1023)
- programs RTC WUT (clocked from RTC/16 ≈ 2312 Hz) for the next
  kernel deadline via `sys_clock_set_timeout`
- announces elapsed ticks on WUT IRQ via `sys_clock_announce`
- selects `TICKLESS_CAPABLE` from `boards/.../Kconfig.uflowmeter_v1`,
  which lets `CONFIG_TICKLESS_KERNEL=y` enable + `CONFIG_PM=y` work

### What's already low-power-friendly

Even with `CONFIG_PM=n` the system aggressively gates everything else:

- **EEPROM** sits in deep power-down (~1 µA) after boot. Wakes
  briefly on each Options save / history ring write, re-parks.
- **TDC1000 + TDC7200** are EN=LOW between measurement cycles.
  Powered up only for ~5 ms per 5 s cycle (0.1% duty).
- **LCD backlight** is in the DT as a `gpio-leds` node — easy to
  toggle off on idle (UI commit 6/6 has the helpers; not wired to
  an idle timer yet).
- **MCO output on PA8** is currently not configured (deferred). When
  enabled, drives the TDC reference at 8 MHz; can be gated when TDCs
  are off.

What we're spending now (`CONFIG_PM=n`, MCU always active):
~2 mA continuous on the bench unit. Vs ~5-10 µA the moment STOP
lands. The gap is real and worth closing.

## EEPROM concurrency

Three call sites reach the chip through `drivers::eeprom_power`'s
`in_dp_state` bool:

1. `main.cpp::save_options_through_dp` — UI edit commits
2. `modbus_handler.cpp::save_options_through_dp` — Modbus 0x06 / 0x10
3. `history.cpp::with_eeprom_awake` — history_tick boundary writes

A fourth caller (`shell::dispatch_shell_action` in `uart.cpp`) also
saves Options on `set_serial`. **All four go through the shared
`options::save_through_dp(eeprom, scratch)` helper** in `options.cpp`,
which acquires `drivers::eeprom_mutex` (`K_MUTEX_DEFINE` in
eeprom_power.cpp), wakes the chip from DP, calls `save()`, re-parks
in DP, and releases the mutex. The mutex prevents `in_dp_state` from
going out of sync — the Zephyr SPI bus mutex already serializes
wire-level access.

## Flash recovery — SwdApWait / SwdDpWait

When you see this on first flash:

```
Info : STLINK V2 ... VID:PID 0483:3748
Info : Target voltage: 3.14
Error: init mode failed (unable to connect to the target)
# or from probe-rs: SwdApWait / SwdDpWait
```

The chip is in STOP mode (from prior firmware) without
`DBGMCU.CR.DBG_STOP=1`, which masks SWD. Recovery requires physical
intervention:

1. **Power-cycle the board** (unplug + replug USB / power). On the
   cold-boot window before STOP entry, SWD responds and
   `west flash --runner openocd` succeeds.
2. **Hold the NRST button** (if exposed) during flash; release ~1 s
   after the openocd "init mode" line appears.

`connect_assert_srst` is already enabled in `support/openocd.cfg`
but only helps if NRST is routed to the ST-Link on the board — on
some bench setups it isn't.

Once flashed with **this** firmware (`CONFIG_PM=n` → no STOP entry),
the chip stays continuously SWD-reachable for subsequent flashes
without ritual.

## Differences from the embassy port

| Concern | Embassy | Zephyr (this port) |
|---|---|---|
| Runtime | `embassy_executor::Executor` async tasks | Zephyr threads + `k_mutex` / `k_sem` / `k_msgq` |
| Sys clock | `embassy-time` + SysTick (broken across STOP) | Custom RTC subseconds sys_clock driver |
| Per-device PM | Embassy refcount (STOP1/STOP2) | Manual (no Zephyr PM today) |
| `Options` | `modular_bitfield` struct | C++ packed POD (byte-exact) |
| History rings | `RingStorage<OFFSET, SIZE, ELEMENT_SIZE>` const-generic Rust template | C++ template w/ the same shape — **fixed SIZE_ON_FLASH bug** |
| Modbus codec | `heapless::Vec` | Fixed-size buffers, allocation-free |
| Shell ↔ Modbus discrimination | Single byte stream into BOTH buffers, \r\n flushes line, 1.75 ms silence flushes frame | Identical heuristic |
| Cyrillic | `cyrillic.rs` FONT + lookalike | Direct C++ port — **bytes identical** |
| Datetime | `time` crate `PrimitiveDateTime` | RTC TR/DR + BCD ↔ binary helpers; survives reset with VBAT |
| Flash runner | `probe-rs` via `.embed.toml` | `west flash --runner openocd` (probe-rs as fallback) |

## Known gaps + planned follow-ups

| # | Gap | Plan |
|---|---|---|
| ~~1~~ | ~~`CONFIG_PM=n` — no STOP mode~~ | **Closed.** Zephyr 4.4 has upstream L1 PM. Re-enabled `CONFIG_PM=y`; our power.cpp overrides keep LCD gating. |
| ~~2~~ | ~~UART STOP-wake~~ | **Closed.** STM32L1 has no UESM (L0/L4+ only), so we wake the chip via EXTI line 10 on PA10's falling edge. Wake-byte lost (USART clock dead ≈ 50 ms while clocks restore); Modbus retries handle it. |
| ~~3~~ | ~~MCO on PA8~~ | **Closed.** Direct CMSIS register poke in `src/mco.{hpp,cpp}` (no L1 DT binding). HSE/1 = 8 MHz. |
| ~~4~~ | ~~`options::save_through_dp` cleanup~~ | **Closed.** Single helper in `options.cpp`; four callers (main, modbus_handler, uart shell, history) consume it. |
| ~~5~~ | ~~Set-verbose log filter~~ | **Closed.** `shell> set_verbose 1` now raises runtime log level to DBG across all sources via `log_filter_set(NULL, …)`; `set_verbose 0` drops to INF. Requires `CONFIG_LOG_RUNTIME_FILTERING=y` (added). |
| ~~6~~ | ~~Formatted `date get` shell reply~~ | **Closed (`cebf509`).** `shell::set_datetime_provider(fn)` lets uart.cpp install a callback that formats `datetime::now()` as `YYYY-MM-DD HH:MM:SS`. Parser stays Zephyr-free (host-testable). |
| ~~7~~ | ~~Keypad repeat (1 s delay, 150 ms interval)~~ | **Closed.** `src/drivers/keypad.cpp` now uses GPIO_INT_EDGE_BOTH + per-button `k_work_delayable`. Press posts first event + arms 1 s delayed work; work handler re-reads pin, posts another event + reschedules 150 ms if still held; release cancels the work. Matches embassy `REPEAT_DELAY` / `REPEAT_INTERVAL`. |
| 8 | LCD power-cycling on idle | Helpers exist in hd44780.cpp; not wired to an idle timer |
| 9 | Extract `crc.{hpp,cpp}` | Unlocks Options-CRC host tests + cleans up duplicate CRC literal in options.cpp |
| 10 | History tests against fake EEPROM | Zephyr `eeprom_fake` driver — would test `history_lib.hpp` + `history.cpp` host-side |

None of the gaps block the basic firmware function — measurement,
UI, Modbus, history, persistence all work as designed against the
embassy reference behavior. The gaps are about polish (4-10) and
power (1-2) and bring-up convenience (3, missing MCO means the TDCs
won't actually measure until that's wired).

## References

- `docs/EMBASSY_PORT.md` — predecessor port's notes; many architectural
  comments still apply (task layout, power philosophy)
- `docs/ARCHITECTURE.md` — original RTIC-era system overview;
  hardware sections still apply
- `docs/HISTORY_SYSTEM.md` — ring layout (corrected SIZE_ON_FLASH
  formula now used; see embassy bug discussion in `history_lib.hpp`)
- `docs/MODBUS_MAP.md` — register map; the byte layout is byte-exact
  with embassy so it carries over
- `tests/README.md` — host test harness quickstart
- `CLAUDE.md` — concise day-to-day reference for working on this branch
