/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * 25LC1024 deep-power-down (DP) control.
 *
 * The Zephyr at25 driver handles READ / WRITE / WREN / RDSR but does
 * NOT expose DP / RDP — those are Microchip-specific commands on top
 * of the AT25 command set:
 *
 *   0xB9  DP   Deep Power-Down. Chip drops from ~5 µA standby to
 *              ~1 µA. All commands except RDP are ignored until
 *              release. Latched on the rising edge of CS after the
 *              command byte (the chip needs a complete CS cycle —
 *              issuing 0xB9 is enough; tDP is sub-microsecond).
 *
 *   0xAB  RDP  Release From Deep Power-Down. Chip starts waking on
 *              the rising edge of CS. tRDP up to 100 µs (datasheet
 *              Table 1-3, "RDP to Read"). Subsequent commands work
 *              once tRDP elapses.
 *
 * Power policy this driver enables: after the boot-time Options load,
 * the chip is parked in DP and stays there until the firmware actively
 * needs to read or write — which on a calibrated meter is rare
 * (Modbus writes during configuration, history saves once an hour /
 * day / month). Continuous savings: 5 → 1 µA, i.e. ~4 µA shaved off
 * sleep current. Meaningful in a battery-powered design where every
 * µA × hours matters.
 *
 * Constraints / invariants (callers must observe):
 *
 *   - exit_deep_power_down() MUST be called before any
 *     eeprom_read() / eeprom_write() on the Zephyr at25 device while
 *     the chip is in DP, or those calls will read garbage / silently
 *     succeed but write nothing.
 *
 *   - This module is NOT thread-safe. State is a single bool. All
 *     calls must come from the same thread (cooperative). Lift to
 *     atomic + add a mutex if a second consumer thread shows up.
 *
 *   - NOT ISR-safe — uses Zephyr SPI APIs that may block on the bus
 *     mutex. Call from thread context only. STOP-mode integration
 *     belongs in an idle hook that runs cooperatively.
 *
 *   - On hardware reset the chip wakes from DP automatically (DP
 *     state is volatile). After-reset is equivalent to a fresh boot
 *     where in_dp_ defaults to false; the chip really is awake.
 */

#pragma once

#include <zephyr/kernel.h>

namespace uflow::drivers {

/* Single mutex guarding the entire wake/op/sleep sequence around the
 * EEPROM. Callers that touch options::save / history rings / DP
 * commands MUST k_mutex_lock(&eeprom_mutex) before the first wake
 * call and unlock after the corresponding sleep — otherwise the
 * in_dp_state bool below races with concurrent callers (main UI
 * dispatch, modbus_handler write, history_tick boundary write).
 *
 * The Zephyr SPI bus mutex serializes at the wire level so data
 * doesn't corrupt, but in_dp_state could go out of sync and result
 * in spurious "chip silently ignored" symptoms. Defined in
 * eeprom_power.cpp via K_MUTEX_DEFINE. */
extern struct k_mutex eeprom_mutex;

/* Issue 0xB9 over the EEPROM's SPI chip-select. Sets internal state
 * to "powered down" on success. No-op if already powered down.
 * Returns 0 on success, negative errno on SPI failure. */
int eeprom_enter_deep_power_down();

/* Issue 0xAB then k_busy_wait(100) for tRDP. Sets internal state to
 * "awake" on success. No-op if not currently powered down.
 * Returns 0 on success, negative errno on SPI failure. */
int eeprom_exit_deep_power_down();

/* Current state. False at boot (chip wakes from reset awake). */
bool eeprom_is_powered_down();

} /* namespace uflow::drivers */
