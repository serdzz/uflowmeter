/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Wall-clock datetime backed by the STM32L1 RTC TR (Time) and DR
 * (Date) registers.
 *
 * Why direct CMSIS register access: our sys_clock driver in
 * src/timer/uflowmeter_rtc_timer.c already owns the RTC for kernel
 * timekeeping (subseconds counter + WUT for tickless wakes). It
 * configured PREDIV_A/PREDIV_S at PRE_KERNEL_2 + unlocked the backup
 * domain (PWR.DBP). The TR/DR + BKP registers are untouched and
 * available to us as a parallel concern — calendar time-of-day vs the
 * kernel's monotonic uptime.
 *
 * The Zephyr stm32 rtc driver is OFF (CONFIG_RTC=n) because the
 * sys_clock driver claims the chip; we manage TR/DR directly here.
 *
 * Persistence model:
 *   - RTC TR/DR auto-update from the 1 Hz ck_spre tick.
 *   - They survive STOP mode (RTC runs from LSI throughout).
 *   - They survive RESET when VBAT is present (backup domain).
 *   - On a cold boot without VBAT, TR/DR is 00:00 on 01-Jan-2000.
 *
 * BKP0R holds a magic value (0x44544D45 "DTME") indicating the
 * calendar has been initialized to something meaningful. init()
 * checks the magic — if absent, sets TR/DR to 2024-01-01 00:00:00
 * and stamps the magic so subsequent boots load the live RTC.
 *
 * BCD layout per STM32L1 RM0038:
 *   TR  bits  3:0 = SU (second units, 0..9)
 *             6:4 = ST (second tens, 0..5)
 *            11:8 = MNU (minute units)
 *           14:12 = MNT (minute tens)
 *           19:16 = HU (hour units)
 *           21:20 = HT (hour tens, 0..2)
 *              22 = PM (we use 24-hour mode → ignored)
 *   DR  bits  3:0 = DU (day units)
 *             5:4 = DT (day tens, 0..3)
 *            11:8 = MU (month units)
 *              12 = MT (month tens, 0..1)
 *           15:13 = WDU (weekday — we don't track, write 1)
 *           19:16 = YU (year units)
 *           23:20 = YT (year tens, 0..9)
 *
 * Year is stored modulo 100 (00..99 → 2000..2099). Range matches
 * YEAR_MIN/MAX in datetime.hpp.
 */

#include "datetime.hpp"

#include <stm32l1xx.h>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(datetime, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::datetime {

namespace {

constexpr std::uint32_t MAGIC = 0x44544D45u;  /* "EMTD" little-endian "DTME" */

constexpr std::uint8_t DAYS_PER_MONTH[] = {
	31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
};

inline std::uint8_t bin_to_bcd(std::uint8_t v)
{
	return static_cast<std::uint8_t>(((v / 10) << 4) | (v % 10));
}

inline std::uint8_t bcd_to_bin(std::uint8_t v)
{
	return static_cast<std::uint8_t>(((v >> 4) * 10) + (v & 0x0Fu));
}

void rtc_unlock()  { RTC->WPR = 0xCAu; RTC->WPR = 0x53u; }
void rtc_lock()    { RTC->WPR = 0xFFu; }

int rtc_enter_init()
{
	RTC->ISR |= RTC_ISR_INIT;
	for (int i = 0; i < 100000; i++) {
		if (RTC->ISR & RTC_ISR_INITF) return 0;
	}
	return -ETIMEDOUT;
}

void rtc_exit_init()  { RTC->ISR &= ~RTC_ISR_INIT; }

/* Wait for RTC shadow registers to refresh (RSF cleared by write,
 * set when shadow is coherent). Required after exiting init mode
 * or after a STOP wake. */
void rtc_wait_for_sync()
{
	RTC->ISR &= ~RTC_ISR_RSF;
	for (int i = 0; i < 100000; i++) {
		if (RTC->ISR & RTC_ISR_RSF) return;
	}
}

DateTimeFields read_rtc_calendar()
{
	/* Read SSR first (unlocks shadow), then TR, then DR. Last
	 * read latches the shadow until next SSR read — required
	 * coherency dance per RM0038 §20.3.8. */
	(void)RTC->SSR;
	const std::uint32_t tr = RTC->TR;
	const std::uint32_t dr = RTC->DR;

	DateTimeFields out{};
	out.second = bcd_to_bin(static_cast<std::uint8_t>(tr & 0x7Fu));
	out.minute = bcd_to_bin(static_cast<std::uint8_t>((tr >> 8) & 0x7Fu));
	out.hour   = bcd_to_bin(static_cast<std::uint8_t>((tr >> 16) & 0x3Fu));
	out.day    = bcd_to_bin(static_cast<std::uint8_t>(dr & 0x3Fu));
	out.month  = bcd_to_bin(static_cast<std::uint8_t>((dr >> 8) & 0x1Fu));
	out.year   = static_cast<std::uint16_t>(
		2000u + bcd_to_bin(static_cast<std::uint8_t>((dr >> 16) & 0xFFu)));
	return out;
}

int write_rtc_calendar(const DateTimeFields& dt)
{
	const std::uint8_t y2 = static_cast<std::uint8_t>(dt.year % 100u);
	const std::uint32_t tr =
		(static_cast<std::uint32_t>(bin_to_bcd(dt.hour))   << 16) |
		(static_cast<std::uint32_t>(bin_to_bcd(dt.minute)) << 8)  |
		(static_cast<std::uint32_t>(bin_to_bcd(dt.second)));
	const std::uint32_t dr =
		(static_cast<std::uint32_t>(bin_to_bcd(y2))        << 16) |
		(static_cast<std::uint32_t>(1u)                    << 13) | /* weekday=Mon */
		(static_cast<std::uint32_t>(bin_to_bcd(dt.month))  << 8)  |
		(static_cast<std::uint32_t>(bin_to_bcd(dt.day)));

	rtc_unlock();
	int rc = rtc_enter_init();
	if (rc < 0) {
		rtc_lock();
		return rc;
	}
	RTC->TR = tr;
	RTC->DR = dr;
	rtc_exit_init();
	rtc_lock();
	rtc_wait_for_sync();

	/* Stamp the magic so a subsequent boot trusts TR/DR even on
	 * a cold reset (with VBAT). Backup domain was unlocked by the
	 * sys_clock driver at PRE_KERNEL_2; PWR.DBP is still set. */
	RTC->BKP0R = MAGIC;
	return 0;
}

K_MUTEX_DEFINE(set_mutex);

DateTimeFields default_baseline()
{
	return DateTimeFields{
		.year = 2024, .month = 1, .day = 1,
		.hour = 0, .minute = 0, .second = 0,
	};
}

} /* namespace */

bool is_leap_year(std::uint16_t year)
{
	return (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
}

std::uint8_t days_in_month(std::uint8_t month, std::uint16_t year)
{
	if (month < 1 || month > 12) return 31;
	std::uint8_t d = DAYS_PER_MONTH[month - 1];
	if (month == 2 && is_leap_year(year)) d = 29;
	return d;
}

std::uint32_t to_timestamp(const DateTimeFields& dt)
{
	std::uint32_t days = 0;
	for (std::uint16_t y = YEAR_MIN; y < dt.year; y++) {
		days += is_leap_year(y) ? 366 : 365;
	}
	for (std::uint8_t m = 1; m < dt.month; m++) {
		days += days_in_month(m, dt.year);
	}
	days += static_cast<std::uint32_t>(dt.day - 1);
	return days * 86400u +
	       static_cast<std::uint32_t>(dt.hour) * 3600u +
	       static_cast<std::uint32_t>(dt.minute) * 60u +
	       static_cast<std::uint32_t>(dt.second);
}

DateTimeFields from_timestamp(std::uint32_t ts)
{
	DateTimeFields out{};
	std::uint32_t days = ts / 86400u;
	std::uint32_t rem  = ts % 86400u;
	out.hour   = static_cast<std::uint8_t>(rem / 3600u);
	rem        = rem % 3600u;
	out.minute = static_cast<std::uint8_t>(rem / 60u);
	out.second = static_cast<std::uint8_t>(rem % 60u);

	std::uint16_t year = YEAR_MIN;
	while (true) {
		std::uint32_t y_days = is_leap_year(year) ? 366u : 365u;
		if (days < y_days) break;
		days -= y_days;
		year++;
		if (year > YEAR_MAX) { year = YEAR_MAX; days = 0; break; }
	}
	out.year = year;

	std::uint8_t month = 1;
	while (month <= 12) {
		std::uint8_t dpm = days_in_month(month, year);
		if (days < dpm) break;
		days -= dpm;
		month++;
	}
	out.month = month;
	out.day   = static_cast<std::uint8_t>(days + 1);
	return out;
}

DateTimeFields now()
{
	return read_rtc_calendar();
}

void set(const DateTimeFields& dt)
{
	/* Serialize against the other writer (UI dispatcher and shell
	 * SetDateUnix both reach here). RTC writes require an init-
	 * mode dance that's not atomic with respect to a concurrent
	 * write. */
	k_mutex_lock(&set_mutex, K_FOREVER);
	int rc = write_rtc_calendar(dt);
	k_mutex_unlock(&set_mutex);
	if (rc < 0) {
		LOG_ERR("RTC TR/DR write failed (%d)", rc);
	} else {
		LOG_INF("RTC set: %04u-%02u-%02u %02u:%02u:%02u",
			static_cast<unsigned>(dt.year),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
	}
}

int init()
{
	const std::uint32_t magic = RTC->BKP0R;
	if (magic == MAGIC) {
		const auto live = read_rtc_calendar();
		LOG_INF("RTC restored: %04u-%02u-%02u %02u:%02u:%02u",
			static_cast<unsigned>(live.year),
			static_cast<unsigned>(live.month),
			static_cast<unsigned>(live.day),
			static_cast<unsigned>(live.hour),
			static_cast<unsigned>(live.minute),
			static_cast<unsigned>(live.second));
		return 0;
	}
	LOG_WRN("RTC BKP0 magic missing (0x%08x) — applying baseline 2024-01-01",
		magic);
	const auto baseline = default_baseline();
	int rc = write_rtc_calendar(baseline);
	if (rc < 0) {
		LOG_ERR("RTC baseline write failed (%d)", rc);
	}
	return rc;
}

void inc_year(DateTimeFields& dt)
{
	if (dt.year < YEAR_MAX) dt.year++;
	const std::uint8_t dpm = days_in_month(dt.month, dt.year);
	if (dt.day > dpm) dt.day = dpm;
}

void dec_year(DateTimeFields& dt)
{
	if (dt.year > YEAR_MIN) dt.year--;
	const std::uint8_t dpm = days_in_month(dt.month, dt.year);
	if (dt.day > dpm) dt.day = dpm;
}

void inc_month(DateTimeFields& dt)
{
	dt.month++;
	if (dt.month > 12) dt.month = 1;
	const std::uint8_t dpm = days_in_month(dt.month, dt.year);
	if (dt.day > dpm) dt.day = dpm;
}

void dec_month(DateTimeFields& dt)
{
	if (dt.month <= 1) dt.month = 12;
	else dt.month--;
	const std::uint8_t dpm = days_in_month(dt.month, dt.year);
	if (dt.day > dpm) dt.day = dpm;
}

void inc_day(DateTimeFields& dt)    { dt = from_timestamp(to_timestamp(dt) + 86400u); }
void dec_day(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 86400u ? ts - 86400u : 0u);
}

void inc_hour(DateTimeFields& dt)   { dt = from_timestamp(to_timestamp(dt) + 3600u); }
void dec_hour(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 3600u ? ts - 3600u : 0u);
}

void inc_minute(DateTimeFields& dt) { dt = from_timestamp(to_timestamp(dt) + 60u); }
void dec_minute(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 60u ? ts - 60u : 0u);
}

void inc_second(DateTimeFields& dt) { dt = from_timestamp(to_timestamp(dt) + 1u); }
void dec_second(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 1u ? ts - 1u : 0u);
}

} /* namespace uflow::datetime */
