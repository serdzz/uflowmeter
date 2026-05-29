/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Minimal wall-clock datetime. Mirrors the embassy code's use of the
 * `time` crate's PrimitiveDateTime — same field semantics, same
 * per-field arithmetic with day-into-month cascading.
 *
 * Backing this commit: k_uptime_get() + an in-RAM offset. set() anchors
 * the offset so subsequent now() calls reflect the user's input. The
 * offset is lost on reboot and on STOP-mode wake clock-restoration
 * (kernel uptime advances during STOP via the RTC, so the math holds,
 * but a power-on resets us to the build epoch).
 *
 * Future commit will back this with the STM32L1 RTC TR/DR registers
 * via direct CMSIS access (the Zephyr stm32 rtc driver is off because
 * our sys_clock driver owns the chip — see src/timer/uflowmeter_rtc_timer.c).
 */

#pragma once

#include <cstdint>

namespace uflow::datetime {

struct DateTimeFields {
	std::uint16_t year;   /* full year, e.g. 2024 */
	std::uint8_t  month;  /* 1..12 */
	std::uint8_t  day;    /* 1..31 (validated against month/year) */
	std::uint8_t  hour;   /* 0..23 */
	std::uint8_t  minute; /* 0..59 */
	std::uint8_t  second; /* 0..59 */
};

/* Current wall-clock datetime. Cheap — single uptime read + math. */
DateTimeFields now();

/* Anchor the wall clock so now() returns `dt` (approximately —
 * truncated to second resolution). */
void set(const DateTimeFields& dt);

/* Per-field arithmetic. inc_day cascades into month/year via the
 * days-in-month table + leap rule; inc_hour cascades into day; etc.
 * dec_X mirrors with underflow cascading. Year is clamped to
 * [YEAR_MIN, YEAR_MAX] without rollover. */
void inc_year(DateTimeFields& dt);
void dec_year(DateTimeFields& dt);
void inc_month(DateTimeFields& dt);
void dec_month(DateTimeFields& dt);
void inc_day(DateTimeFields& dt);
void dec_day(DateTimeFields& dt);
void inc_hour(DateTimeFields& dt);
void dec_hour(DateTimeFields& dt);
void inc_minute(DateTimeFields& dt);
void dec_minute(DateTimeFields& dt);
void inc_second(DateTimeFields& dt);
void dec_second(DateTimeFields& dt);

constexpr std::uint16_t YEAR_MIN = 2000;
constexpr std::uint16_t YEAR_MAX = 2099;

/* Days in `month` for `year` — handles February's leap behavior. */
std::uint8_t days_in_month(std::uint8_t month, std::uint16_t year);

bool is_leap_year(std::uint16_t year);

/* Convert to/from a seconds-since-2000-01-01 timestamp. Used for the
 * EEPROM history ring timestamps in a follow-up commit; exposed here
 * because it's pure-logic adjacent to the field math. */
std::uint32_t to_timestamp(const DateTimeFields& dt);
DateTimeFields from_timestamp(std::uint32_t ts);

} /* namespace uflow::datetime */
