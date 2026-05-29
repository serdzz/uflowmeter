/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "datetime.hpp"

#include <zephyr/kernel.h>

namespace uflow::datetime {

namespace {

constexpr std::uint8_t DAYS_PER_MONTH[] = {
	31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
};

/* In-RAM offset: now() = from_timestamp(uptime_seconds + offset_).
 * Initial value sets boot to 2024-01-01 00:00:00 so a fresh device
 * displays a sane date until the user sets it. */
DateTimeFields default_boot()
{
	return DateTimeFields{
		.year = 2024, .month = 1, .day = 1,
		.hour = 0, .minute = 0, .second = 0,
	};
}

std::int64_t offset_ = 0;
bool offset_initialized_ = false;

std::uint32_t uptime_seconds()
{
	return static_cast<std::uint32_t>(k_uptime_get() / 1000);
}

void ensure_initialized()
{
	if (!offset_initialized_) {
		offset_ = static_cast<std::int64_t>(to_timestamp(default_boot()));
		offset_initialized_ = true;
	}
}

} /* namespace */

bool is_leap_year(std::uint16_t year)
{
	return (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
}

std::uint8_t days_in_month(std::uint8_t month, std::uint16_t year)
{
	if (month < 1 || month > 12) {
		return 31;
	}
	std::uint8_t d = DAYS_PER_MONTH[month - 1];
	if (month == 2 && is_leap_year(year)) {
		d = 29;
	}
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
	ensure_initialized();
	const std::int64_t ts = static_cast<std::int64_t>(uptime_seconds()) + offset_;
	const std::uint32_t clamped = (ts < 0) ? 0u : static_cast<std::uint32_t>(ts);
	return from_timestamp(clamped);
}

void set(const DateTimeFields& dt)
{
	const std::int64_t target = static_cast<std::int64_t>(to_timestamp(dt));
	offset_ = target - static_cast<std::int64_t>(uptime_seconds());
	offset_initialized_ = true;
}

/* Field-stepping helpers. Cascading handled by converting to timestamp
 * and back where overflow needs day/month/year coupling; year/month
 * mutations stay in-place. */

void inc_year(DateTimeFields& dt)
{
	if (dt.year < YEAR_MAX) {
		dt.year++;
	}
	const std::uint8_t dpm = days_in_month(dt.month, dt.year);
	if (dt.day > dpm) dt.day = dpm;
}

void dec_year(DateTimeFields& dt)
{
	if (dt.year > YEAR_MIN) {
		dt.year--;
	}
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

void inc_day(DateTimeFields& dt)
{
	dt = from_timestamp(to_timestamp(dt) + 86400u);
}

void dec_day(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 86400u ? ts - 86400u : 0u);
}

void inc_hour(DateTimeFields& dt)
{
	dt = from_timestamp(to_timestamp(dt) + 3600u);
}

void dec_hour(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 3600u ? ts - 3600u : 0u);
}

void inc_minute(DateTimeFields& dt)
{
	dt = from_timestamp(to_timestamp(dt) + 60u);
}

void dec_minute(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 60u ? ts - 60u : 0u);
}

void inc_second(DateTimeFields& dt)
{
	dt = from_timestamp(to_timestamp(dt) + 1u);
}

void dec_second(DateTimeFields& dt)
{
	const std::uint32_t ts = to_timestamp(dt);
	dt = from_timestamp(ts >= 1u ? ts - 1u : 0u);
}

} /* namespace uflow::datetime */
