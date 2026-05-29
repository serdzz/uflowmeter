/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Custom Zephyr sys_clock driver backed by STM32L1 RTC.
 *
 * REPLACES the standard ARM SysTick driver (CONFIG_CORTEX_M_SYSTICK=n)
 * because SysTick stops in STOP mode while the RTC keeps running off
 * LSI through STOP — that's what makes k_sleep() survive STOP for us.
 *
 * Clock layout:
 *   LSI (~37 kHz typical, ±10-15% accuracy without calibration)
 *     └─> RTC, PREDIV_A=35, PREDIV_S=1023
 *           ├─> ck_apre  = LSI / 36 ≈ 1027 Hz   (calendar prescaler)
 *           ├─> ck_spre  = ck_apre / 1024 ≈ 1 Hz (calendar 1-Hz tick)
 *           └─> Subseconds counter: counts down from PREDIV_S=1023
 *                                    at ck_apre rate (~1024 Hz)
 *
 * Kernel cycle/tick: 1024 Hz. Both
 *   CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC = 1024
 *   CONFIG_SYS_CLOCK_TICKS_PER_SEC    = 1024
 * so 1 tick == 1 cycle == ~1 ms.
 *
 * Programmable wakeup uses WUT with WUCKSEL = 000 (RTC/16 ≈ 2312 Hz),
 * giving a max single-shot sleep of WUTR_MAX/2312 ≈ 28 s. For longer
 * kernel timeouts we re-program WUT in the ISR and continue.
 *
 * Accuracy caveats:
 *   - LSI is uncalibrated and drifts ±10-15%. Real time will not
 *     match k_uptime to high precision. For an application where the
 *     measurement cycle is 5 s and the failure mode of "fired at 4.5 s"
 *     is harmless, this is acceptable.
 *   - When a real-time-of-day clock is needed (history rings, RTC
 *     calendar), introduce an LSE crystal on the next board rev or
 *     periodically calibrate LSI against HSE via the RCC CIR.
 *
 * Untested: this file was written without a Zephyr SDK on the
 * authoring host. Expect register-name or LL-macro drift on first
 * `west build` against Zephyr v3.7 LTS.
 */

#include <stm32l1xx.h>

#include <zephyr/arch/cpu.h>
#include <zephyr/drivers/timer/system_timer.h>
#include <zephyr/init.h>
#include <zephyr/irq.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/util.h>
#include <zephyr/sys_clock.h>

#define LSI_FREQ_HZ            37000U   /* nominal — datasheet typ */
#define RTC_PREDIV_A           35U      /* asynch prescaler - 1 */
#define RTC_PREDIV_S           1023U    /* synch prescaler - 1 (subseconds reload) */

#define WUT_CLOCK_HZ           ((LSI_FREQ_HZ + 8U) / 16U)  /* RTC/16 ~2312 Hz */

/* Convert kernel ticks (at 1024 Hz) to WUT counts (at WUT_CLOCK_HZ).
 * Both rates are derived from LSI so drift cancels — the ratio
 * WUT_CLOCK_HZ / TICKS_PER_SEC ≈ 2312/1024 ≈ 2.258 is stable. */
#define TICKS_TO_WUT(t)        ((uint32_t)(((uint64_t)(t) * WUT_CLOCK_HZ + \
                                            (CONFIG_SYS_CLOCK_TICKS_PER_SEC / 2)) / \
                                           CONFIG_SYS_CLOCK_TICKS_PER_SEC))
#define WUT_MAX                65535U
#define MAX_TICKS              ((uint32_t)((uint64_t)WUT_MAX * CONFIG_SYS_CLOCK_TICKS_PER_SEC / WUT_CLOCK_HZ))

/* Monotonic cycle counter: extends the ~1 Hz wrap of (PREDIV_S - SSR)
 * into a 64-bit accumulator. Bumped in the ISR after each WUT fire and
 * lazily on every sys_clock_cycle_get_32 call to absorb the in-flight
 * subseconds delta. */
static uint64_t cycle_count_high;
static uint16_t last_ssr;

/* Number of ticks the kernel hasn't been told about yet. Drained by
 * sys_clock_announce on each ISR + on sys_clock_idle_exit. */
static uint32_t pending_ticks;

/* Read the subseconds counter — counts DOWN from RTC_PREDIV_S at
 * ~1024 Hz. To get a count-UP value: (RTC_PREDIV_S - SSR). */
static inline uint16_t rtc_subsec_read(void)
{
	/* SSR is updated on the falling edge of ck_apre. Read TR after
	 * SSR to unlock the shadow registers (required by the RTC
	 * datasheet's read coherency rule). */
	uint32_t ssr = RTC->SSR & 0xFFFFu;
	(void)RTC->TR;
	(void)RTC->DR;
	return (uint16_t)ssr;
}

/* Unlock the RTC write-protected registers (WPR sequence). Pair with
 * rtc_lock() — must not be skipped or future writes are silently
 * dropped by the chip. */
static inline void rtc_unlock(void)
{
	RTC->WPR = 0xCAu;
	RTC->WPR = 0x53u;
}
static inline void rtc_lock(void)
{
	RTC->WPR = 0xFFu;
}

/* RTC INITF dance: required to write the prescalers. */
static int rtc_enter_init_mode(void)
{
	RTC->ISR |= RTC_ISR_INIT;
	for (int i = 0; i < 100000; i++) {
		if (RTC->ISR & RTC_ISR_INITF) {
			return 0;
		}
	}
	return -ETIMEDOUT;
}

static void rtc_exit_init_mode(void)
{
	RTC->ISR &= ~RTC_ISR_INIT;
}

/* Program WUT for `wut_count` ticks (range 1..65535). Enables IRQ.
 * Must be called with WUT disabled or the chip drops the write. */
static int wut_program(uint32_t wut_count)
{
	if (wut_count == 0) {
		wut_count = 1;
	}
	if (wut_count > WUT_MAX) {
		wut_count = WUT_MAX;
	}

	rtc_unlock();
	RTC->CR &= ~RTC_CR_WUTE;
	/* WUTWF must be set before WUTR can be written. */
	for (int i = 0; i < 100000; i++) {
		if (RTC->ISR & RTC_ISR_WUTWF) {
			break;
		}
	}
	RTC->WUTR = (wut_count - 1u) & 0xFFFFu;
	RTC->ISR &= ~RTC_ISR_WUTF;
	RTC->CR |= RTC_CR_WUTIE | RTC_CR_WUTE;
	rtc_lock();
	return 0;
}

/* RTC_WKUP_IRQn is a direct ISR — minimal latency, no kernel
 * scheduling overhead. We clear the EXTI22 line + WUTF, announce the
 * ticks the kernel was waiting on, and return. */
ISR_DIRECT_DECLARE(rtc_wakeup_isr)
{
	/* Clear EXTI22 pending bit (RTC WUT routes through EXTI line 22). */
	EXTI->PR = (1U << 22);

	if (RTC->ISR & RTC_ISR_WUTF) {
		rtc_unlock();
		RTC->ISR &= ~RTC_ISR_WUTF;
		rtc_lock();

		uint32_t ticks = pending_ticks;
		pending_ticks = 0;
		if (ticks == 0) {
			ticks = 1;
		}
		sys_clock_announce((int32_t)ticks);
	}

	ISR_DIRECT_PM();
	return 0;
}

void sys_clock_set_timeout(int32_t ticks, bool idle)
{
	ARG_UNUSED(idle);

	if (ticks == K_TICKS_FOREVER) {
		/* No timeout requested — disable WUT, the kernel will be
		 * woken by some other IRQ (keypad EXTI, TDC INT, etc.). */
		rtc_unlock();
		RTC->CR &= ~(RTC_CR_WUTE | RTC_CR_WUTIE);
		rtc_lock();
		pending_ticks = 0;
		return;
	}

	uint32_t t = (ticks <= 0) ? 1u : (uint32_t)ticks;
	if (t > MAX_TICKS) {
		t = MAX_TICKS;
	}
	pending_ticks = t;
	wut_program(TICKS_TO_WUT(t));
}

uint32_t sys_clock_elapsed(void)
{
	/* Subseconds advance ~1024×/sec; the count we owe the kernel is
	 * (last_ssr - now_ssr) mod (PREDIV_S+1), assuming we don't wrap
	 * more than once between calls (we wouldn't — the kernel polls
	 * us at least every tick). */
	uint16_t now = rtc_subsec_read();
	uint16_t diff;
	if (now <= last_ssr) {
		diff = last_ssr - now;
	} else {
		/* SSR wrapped (counted down past 0, reloaded to PREDIV_S). */
		diff = (uint16_t)((RTC_PREDIV_S + 1u) - now + last_ssr);
	}
	return diff;
}

uint32_t sys_clock_cycle_get_32(void)
{
	/* Lazy roll-up of cycle_count_high. Each call samples SSR,
	 * computes the delta since last sample, adds to the accumulator. */
	uint16_t now = rtc_subsec_read();
	uint16_t diff;
	if (now <= last_ssr) {
		diff = last_ssr - now;
	} else {
		diff = (uint16_t)((RTC_PREDIV_S + 1u) - now + last_ssr);
	}
	cycle_count_high += diff;
	last_ssr = now;
	return (uint32_t)(cycle_count_high & 0xFFFFFFFFu);
}

void sys_clock_idle_exit(void)
{
	/* Called by the kernel after returning from idle. Drain any
	 * pending ticks the WUT IRQ couldn't capture (e.g., if we exited
	 * STOP via a non-RTC source like the keypad EXTI). */
	if (pending_ticks > 0) {
		uint32_t ticks = pending_ticks;
		pending_ticks = 0;
		sys_clock_announce((int32_t)ticks);
	}
}

static int sys_clock_driver_init(void)
{
	/* Enable PWR + backup-domain access. The RTC lives in the backup
	 * power domain and requires DBP set before any register writes. */
	RCC->APB1ENR |= RCC_APB1ENR_PWREN;
	PWR->CR |= PWR_CR_DBP;

	/* LSI on. */
	RCC->CSR |= RCC_CSR_LSION;
	while (!(RCC->CSR & RCC_CSR_LSIRDY)) { }

	/* Reset the backup domain ONLY if RTC isn't already on a
	 * compatible source — preserves backup-register state across
	 * software resets. For first power-up the RTC isn't enabled, so
	 * we configure from scratch. */
	if ((RCC->CSR & RCC_CSR_RTCSEL) != RCC_CSR_RTCSEL_LSI) {
		RCC->CSR |= RCC_CSR_RTCRST;
		RCC->CSR &= ~RCC_CSR_RTCRST;
		RCC->CSR = (RCC->CSR & ~RCC_CSR_RTCSEL_Msk) | RCC_CSR_RTCSEL_LSI;
	}
	RCC->CSR |= RCC_CSR_RTCEN;

	/* Configure prescalers in init mode. */
	rtc_unlock();
	int rc = rtc_enter_init_mode();
	if (rc < 0) {
		rtc_lock();
		return rc;
	}
	/* PRER: SYNCH in [14:0], ASYNCH in [22:16]. Write SYNCH first
	 * by datasheet — Cortex Atomic Read-Modify-Write would do this
	 * via two writes anyway. */
	RTC->PRER = (RTC->PRER & 0xFF800000u) | (uint32_t)RTC_PREDIV_S;
	RTC->PRER = ((uint32_t)RTC_PREDIV_A << 16) | (uint32_t)RTC_PREDIV_S;
	/* WUCKSEL = 000 → RTC/16. */
	RTC->CR &= ~RTC_CR_WUCKSEL_Msk;
	rtc_exit_init_mode();
	rtc_lock();

	/* EXTI line 22 wires the RTC WUT event into the NVIC. Configure
	 * as rising-edge wake source. */
	EXTI->RTSR |= (1U << 22);
	EXTI->IMR  |= (1U << 22);

	/* Initial cycle-counter baseline. */
	last_ssr = rtc_subsec_read();
	cycle_count_high = 0;
	pending_ticks = 0;

	/* Normal-priority direct ISR — IRQ_ZERO_LATENCY requires
	 * CONFIG_ZERO_LATENCY_IRQS which isn't on by default. */
	IRQ_DIRECT_CONNECT(RTC_WKUP_IRQn, 0, rtc_wakeup_isr, 0);
	irq_enable(RTC_WKUP_IRQn);

	return 0;
}

SYS_INIT(sys_clock_driver_init, PRE_KERNEL_2, CONFIG_SYSTEM_CLOCK_INIT_PRIORITY);
