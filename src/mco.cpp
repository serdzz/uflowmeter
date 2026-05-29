/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * MCO PA8 = HSE/1 via direct CMSIS register access. See mco.hpp.
 *
 * STM32L1 RCC->CFGR layout (RM0038 §6.3.3):
 *   bits 26:24  MCOSEL  0=disabled, 1=SYSCLK, 2=HSI, 3=MSI,
 *                       4=HSE, 5=PLL, 6=LSI, 7=LSE
 *   bits 30:28  MCOPRE  0=/1, 1=/2, 2=/4, 3=/8, 4=/16
 *
 * GPIO PA8 needs MODER=10 (AF) + AFRH[3:0]=0 (AF0 = MCO on L1).
 * High output speed so the 8 MHz square wave is clean on the TDC
 * input pins.
 */

#include "mco.hpp"

#include <stm32l1xx.h>

#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(mco, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::mco {

void init()
{
	/* GPIOA clock is already enabled by Zephyr's clock_control on
	 * boot (every Zephyr STM32 board enables all GPIO ports for the
	 * stm32-gpio driver). Just configure PA8 mode + AF. */

	/* PA8 MODER bits 17:16 → 10 (AF). Clear first, then set. */
	GPIOA->MODER = (GPIOA->MODER & ~GPIO_MODER_MODER8_Msk) |
	               (0b10u << GPIO_MODER_MODER8_Pos);

	/* PA8 AFRH bits 3:0 (alternate function for pin 8) → 0 (AF0 = MCO). */
	GPIOA->AFR[1] = GPIOA->AFR[1] & ~(0xFu << ((8 - 8) * 4));

	/* High output speed for the 8 MHz waveform — OSPEEDR bits 17:16 → 11. */
	GPIOA->OSPEEDR = (GPIOA->OSPEEDR & ~GPIO_OSPEEDER_OSPEEDR8_Msk) |
	                 (0b11u << GPIO_OSPEEDER_OSPEEDR8_Pos);

	/* No pull (TDC chip has its own input bias). PUPDR bits 17:16 → 00. */
	GPIOA->PUPDR = GPIOA->PUPDR & ~GPIO_PUPDR_PUPDR8_Msk;

	/* RCC->CFGR MCOSEL=HSE (4), MCOPRE=/1 (0). */
	RCC->CFGR = (RCC->CFGR & ~(RCC_CFGR_MCOSEL_Msk | RCC_CFGR_MCOPRE_Msk)) |
	            (0b100u << RCC_CFGR_MCOSEL_Pos) |
	            (0b000u << RCC_CFGR_MCOPRE_Pos);

	LOG_INF("MCO armed: PA8 = HSE / 1 = 8 MHz (TDC reference)");
}

} /* namespace uflow::mco */
