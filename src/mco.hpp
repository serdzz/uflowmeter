/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * MCO (Master Clock Output) on PA8 — drives the TDC1000 + TDC7200
 * CLOCK pins with the 8 MHz HSE crystal.
 *
 * Zephyr 4.4's `st,stm32-clock-mco` DT binding exists on L4+ but not
 * on L1, so we configure RCC->CFGR.MCOSEL/MCOPRE + PA8 AF0 directly
 * via CMSIS (same approach as src/power.cpp and the custom sys_clock
 * driver). PA8 is otherwise unused — declaring it as a generic GPIO
 * elsewhere would collide with the runtime AF setup here.
 *
 * Embassy parity: the embassy port set up MCO identically via its
 * `embassy_stm32::rcc::Mco::new(...)` with `HseMode::Oscillator` +
 * `McoConfig::default()` (HSE source, /1 prescaler) — see
 * `git show rework/embassy:src/main.rs` lines 131-140.
 *
 * Call from main once at boot, AFTER Zephyr's clock_control has
 * enabled HSE (the &clk_hse DT node is `status = "okay"`), BEFORE
 * any code expects the TDC chips to be clocked.
 */

#pragma once

namespace uflow::mco {

/* Configure PA8 as AF0 (MCO) and route HSE/1 = 8 MHz to it. Returns
 * void — there's no failure mode beyond "HSE wasn't ready" which the
 * clock driver would already have aborted boot on. */
void init();

} /* namespace uflow::mco */
