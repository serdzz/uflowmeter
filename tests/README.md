# Host-side test harness

Pure-logic tests that run on the developer's host without flashing
to the STM32. Independent of Zephyr — uses the system's default
C++ compiler.

## Quick start

```sh
cmake -S tests -B build-tests
cmake --build build-tests
./build-tests/uflowmeter_tests
```

Expected output ends with `N passed, 0 failed (of N)`. Exit status is
non-zero if any assertion tripped.

CMake's `ctest` integration also works:

```sh
ctest --test-dir build-tests --output-on-failure
```

## What's tested

| Module | File | Coverage |
|---|---|---|
| Modbus RTU codec | `src/modbus.cpp` | CRC vectors, parse for each function code, slave address filtering, broadcast handling, response build + round-trip, exception frame, buffer-too-small |
| Shell parser | `src/shell.cpp` | Each command (help, date, zero, calibrate, set_serial, set_verbose, get_settings, get_calibration), parse_action side-effect extraction, error paths, \r\n trimming |
| Calibration | `src/calibration.cpp` | All four `apply_ratio` zones (dead/K0/K0→K1/K1→K2/K2/clamp), raw volume positive + invalid TOF, dtof0 zero-flow case, get_volume composite |
| CRC16-CCITT-FALSE | `src/crc.cpp` | Canonical IBM-3740 check vector (0x29B1), empty input, single-zero-byte vector, repeatability, concatenation distinctness, polynomial separation from the Modbus CRC |
| History RingStorage | `src/history_lib.hpp` | Empty-load, add/find round-trip, chained-ring non-overlap (`SIZE_ON_FLASH` invariant), wrap-around eviction, gap-fill of skipped periods, large-jump full-ring reset, CRC-corruption restart, eeprom read-error propagation. Backed by `tests/fake_eeprom.{hpp,cpp}` + shim headers under `tests/shims/zephyr/`. |

## What's NOT tested here (and why)

| Module | Why deferred |
|---|---|
| `src/datetime.cpp` | Direct STM32L1 RTC TR/DR register access. Pure-logic helpers (to_timestamp/from_timestamp/inc_*/dec_*) could be split into a separate file and tested; pending refactor. |
| `src/options.cpp` | Pulls in `<zephyr/drivers/eeprom.h>`. The CRC function is now extracted to `src/crc.{hpp,cpp}` and tested; the dual-page protocol body itself still needs the fake-eeprom shim from the history tests to be threaded through. |
| `src/history.cpp` | Owns the 60 s tick thread + `datetime::now()`. Would need a kernel + RTC shim on top of what `test_history_lib.cpp` already wires up. |
| `src/measurement.cpp`, `src/power.cpp`, `src/timer/*` | Tightly coupled to Zephyr kernel + STM32L1 hardware. Integration-level concerns. |
| `src/ui/render.cpp` | Reads from HD44780 driver; needs a mock CharacterDisplay. The pure-logic UI (menu_controller, menu_list, screen, events) IS testable in isolation — added in a follow-up. |
| All `src/drivers/*` and `src/ui/menu_controller.cpp` | Either HAL-bound or pulls in global Zephyr state. |

## Framework

`tests/framework.{hpp,cpp}` is ~80 lines total. `TEST(name) { ... }`
registers via a static-constructor; `ASSERT_EQ` / `ASSERT_TRUE` /
`ASSERT_NEAR` / `ASSERT_NE` / `ASSERT_FALSE` are non-aborting (they
log + return from the test fn, runner counts the case as FAIL).

When the suite grows past a few hundred cases, the natural upgrade
is `west twister` against Zephyr's `native_sim` board so the same
binary also exercises the HAL-bound code via Zephyr device emulation.
