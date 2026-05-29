/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * STOP-mode power management.
 *
 * Wires Zephyr's pm_state_set / pm_state_exit_post_ops hooks to the
 * STM32L1 PWR + RCC registers so SUSPEND_TO_IDLE actually enters
 * STOP mode (regulator in low-power, PLL+HSE off, ~5 µA system draw).
 *
 * Sleep coverage in this commit:
 *   ✓ CPU            → STOP via PWR.LPSDSR + SLEEPDEEP + WFI
 *   ✓ LCD backlight  → off (PC5 active-LOW HIGH)
 *   ✓ LCD VCC        → off (PC0 active-LOW HIGH); re-init on wake
 *   ✓ EEPROM         → already in deep-power-down since boot
 *   ✓ TDC1000/7200   → already EN=LOW between measurement cycles
 *
 * Wake sources during STOP:
 *   - RTC WUT (EXTI22) — the kernel timer driver programmed it for
 *     the next k_sleep deadline.
 *   - Keypad EXTI 6-9 (PB6-PB9) — falling edge wakes the chip; the
 *     keypad ISR runs immediately and pushes a KeyEvent onto the
 *     msgq so main thread can consume.
 *
 * After wake, pm_state_exit_post_ops re-enables HSE+PLL, switches
 * SYSCLK back to PLL, restores LCD power + reruns the HD44780 init
 * dance (~50 ms blocking; happens once per STOP exit). Until that
 * runs, the CPU is on MSI (~2 MHz) — peripheral timing is wrong, so
 * we keep PRIMASK locked across the restore.
 */

#pragma once

namespace uflow::power {

/* Initialize the PM subsystem — currently a no-op stub. RTC + LSI
 * setup happens in the timer driver (PRE_KERNEL_2); PWR + DBP setup
 * happens there too as a side-effect. Reserved for future per-device
 * runtime PM registration. */
int init();

} /* namespace uflow::power */
