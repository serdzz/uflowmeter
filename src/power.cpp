/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * STM32L1 STOP-mode entry/exit via Zephyr PM hooks. See power.hpp.
 */

#include "power.hpp"

#include <stm32l1xx.h>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/pm/pm.h>

#include "drivers/hd44780.hpp"

LOG_MODULE_REGISTER(power, CONFIG_LOG_DEFAULT_LEVEL);

namespace {

/* HSE/PLL parameters MUST match what the board's clock-control driver
 * configured at boot. From boards/uflowmeter/uflowmeter_v1/...dts:
 *   HSE 8 MHz / 3 × 6 = 16 MHz SYSCLK. */
constexpr std::uint32_t PLL_MUL = RCC_CFGR_PLLMUL6;
constexpr std::uint32_t PLL_DIV = RCC_CFGR_PLLDIV3;

void restore_clocks(void)
{
	/* Sequence per STM32L1 reference manual §6.3:
	 *   1. HSE on, wait HSERDY
	 *   2. PLL config (MUL, DIV, SRC=HSE) — only while PLL is OFF
	 *   3. PLL on, wait PLLRDY
	 *   4. SYSCLK switch to PLL, wait SWS=PLL
	 *
	 * The PLL config bits were set at boot; STOP doesn't reset
	 * CFGR, so we only need to flip the enables here.
	 */
	RCC->CR |= RCC_CR_HSEON;
	while (!(RCC->CR & RCC_CR_HSERDY)) { }

	RCC->CR |= RCC_CR_PLLON;
	while (!(RCC->CR & RCC_CR_PLLRDY)) { }

	RCC->CFGR = (RCC->CFGR & ~RCC_CFGR_SW_Msk) | RCC_CFGR_SW_PLL;
	while ((RCC->CFGR & RCC_CFGR_SWS_Msk) != RCC_CFGR_SWS_PLL) { }
}

} /* namespace */

/* Zephyr calls these as weak symbols — our definitions override the
 * default no-op stubs. They run from the idle thread context with the
 * scheduler locked. */
extern "C" void pm_state_set(enum pm_state state, std::uint8_t substate_id)
{
	ARG_UNUSED(substate_id);

	if (state != PM_STATE_SUSPEND_TO_IDLE) {
		return;
	}

	/* Cut LCD VCC + backlight IF the user is actively looking at
	 * the screen. If main's idle timer already turned the LCD off,
	 * we don't need to touch it (saves cycles + LCD-init time on
	 * the way back). Either way, the LCD logic loses state during
	 * STOP — we re-init in pm_state_exit_post_ops only if the
	 * user expects it back on. */
	if (uflow::drivers::lcd_user_wants_on) {
		(void)k_mutex_lock(&uflow::drivers::lcd_mutex, K_FOREVER);
		uflow::drivers::lcd_backlight_off();
		uflow::drivers::lcd_power_off();
		k_mutex_unlock(&uflow::drivers::lcd_mutex);
	}

	/* Configure STOP mode in PWR_CR:
	 *   - LPSDSR  = 1 (regulator in low-power mode during deep sleep)
	 *   - PDDS    = 0 (STOP, not STANDBY)
	 *   - CWUF    = 1 (clear any prior wake flag)
	 */
	PWR->CR = (PWR->CR & ~(PWR_CR_PDDS)) | PWR_CR_LPSDSR | PWR_CR_CWUF;

	SCB->SCR |= SCB_SCR_SLEEPDEEP_Msk;

	__DSB();
	__WFI();
	__ISB();

	/* WFI returned. The kernel will call pm_state_exit_post_ops
	 * next; clocks are still on MSI right now and any code that
	 * runs between here and the clock restore sees wrong peripheral
	 * timing. The kernel arranges for pm_state_exit_post_ops to run
	 * with IRQs still masked, so no thread switches in the gap. */
}

extern "C" void pm_state_exit_post_ops(enum pm_state state, std::uint8_t substate_id)
{
	ARG_UNUSED(substate_id);

	if (state != PM_STATE_SUSPEND_TO_IDLE) {
		return;
	}

	SCB->SCR &= ~SCB_SCR_SLEEPDEEP_Msk;
	PWR->CR &= ~PWR_CR_LPSDSR;

	restore_clocks();

	/* Restore LCD power IFF the user still wants it. Skip the
	 * ~50 ms re-init when the idle timer has parked the screen —
	 * keeps the wake fast for measurement-cycle-only STOP exits. */
	if (uflow::drivers::lcd_user_wants_on) {
		(void)k_mutex_lock(&uflow::drivers::lcd_mutex, K_FOREVER);
		uflow::drivers::lcd_power_on();
		(void)uflow::drivers::lcd().init();
		uflow::drivers::lcd_backlight_on();
		k_mutex_unlock(&uflow::drivers::lcd_mutex);
	}
}

namespace uflow::power {

int init()
{
	/* Reserved for future per-device PM registration. RTC + DBP +
	 * LSI setup happens in src/timer/uflowmeter_rtc_timer.c at
	 * PRE_KERNEL_2 — much earlier than this hook would run. */
	return 0;
}

} /* namespace uflow::power */
