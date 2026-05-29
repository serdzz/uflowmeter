/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Real history backend — replaces the commit 4/6 stub. Three rings:
 *   HourHistory  : 2160 slots × 3600 s ≈ 90 days
 *   DayHistory   : 1116 slots × 86400 s ≈ 3 years
 *   MonthHistory :  120 slots × 2678400 s ≈ 10 years
 *
 * Total EEPROM footprint: 4096 (stat page reserve) + 8656 + 4480 + 496
 * = 17728 bytes (13.5% of 128 KB). Plenty of headroom.
 *
 * Lifecycle:
 *   - init(eeprom) at boot loads the three rings.
 *   - start() spawns history_tick_thread which wakes every 60 s,
 *     reads datetime + flow, accumulates into hour/day/month buckets,
 *     writes a slot when the corresponding calendar boundary fires
 *     (minute=0 → hour ring, hour=0 → day ring, day=1 → month ring).
 *   - query(kind, ts) is called by main's SetHistory dispatcher when
 *     the UI cursor moves; it wakes the EEPROM, reads the slot,
 *     re-parks the chip, returns the value.
 *
 * EEPROM concurrency caveat: this module and Options::save (called
 * from main on edit commits) both reach into the eeprom_power
 * wake/sleep cycle without a shared mutex. Collision window is
 * micro-second wide and the operations don't share state on the
 * chip, but the wake/sleep flag race is real — TODO add a k_mutex
 * `eeprom_mutex` if scope grows.
 */

#include "history.hpp"

#include <atomic>
#include <cmath>

#include <zephyr/device.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "datetime.hpp"
#include "drivers/eeprom_power.hpp"
#include "history_lib.hpp"
#include "measurement.hpp"

LOG_MODULE_REGISTER(history, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::history {

namespace {

using HourHistory  = history_lib::RingStorage<0, 2160, 3600>;
using DayHistory   = history_lib::RingStorage<HourHistory::SIZE_ON_FLASH, 1116, 86400>;
using MonthHistory = history_lib::RingStorage<
	HourHistory::SIZE_ON_FLASH + DayHistory::SIZE_ON_FLASH, 120, 2678400>;

HourHistory  hour_ring_;
DayHistory   day_ring_;
MonthHistory month_ring_;

QueryResult g_last_result{};

/* Flow accumulators in m³, summed across measurement ticks. Reset on
 * each ring boundary write. NaN-initialized so we can distinguish
 * "no measurements yet" from a real zero. */
double hour_flow_  = 0.0;
double day_flow_   = 0.0;
double month_flow_ = 0.0;

const struct device* eeprom_dev_ = nullptr;

K_THREAD_STACK_DEFINE(history_stack, 1024);
struct k_thread history_thread_data;
k_tid_t history_tid = nullptr;

/* Wake the EEPROM, run `fn`, re-park. fn signature must be
 * `int fn(const struct device*)`. Returns fn's return code (or the
 * wake/sleep failure if those go wrong). */
template <typename F>
int with_eeprom_awake(F&& fn)
{
	/* Serialize against options::save call sites in main + modbus. */
	k_mutex_lock(&drivers::eeprom_mutex, K_FOREVER);
	int rc = drivers::eeprom_exit_deep_power_down();
	if (rc < 0) {
		LOG_ERR("eeprom wake failed (%d)", rc);
		k_mutex_unlock(&drivers::eeprom_mutex);
		return rc;
	}
	int op_rc = fn(eeprom_dev_);
	int sleep_rc = drivers::eeprom_enter_deep_power_down();
	if (sleep_rc < 0) {
		LOG_WRN("eeprom re-DP failed (%d) — chip stays in standby", sleep_rc);
	}
	k_mutex_unlock(&drivers::eeprom_mutex);
	return op_rc;
}

std::uint32_t timestamp_now()
{
	return datetime::to_timestamp(datetime::now());
}

void history_tick_thread(void*, void*, void*)
{
	LOG_INF("history tick thread up");
	for (;;) {
		k_sleep(K_SECONDS(60));

		const auto dt = datetime::now();
		const float flow = measurement::latest_flow_m3h.load(std::memory_order_relaxed);
		const float sample = std::isfinite(flow) ? flow : 0.0f;

		hour_flow_  += sample;
		day_flow_   += sample;
		month_flow_ += sample;

		const std::uint32_t ts = datetime::to_timestamp(dt);

		/* Hour boundary: minute is 0. Write hour_flow_ into the
		 * hour ring, then reset. */
		if (dt.minute != 0) {
			continue;
		}

		const std::int32_t hour_val = static_cast<std::int32_t>(hour_flow_);
		int rc = with_eeprom_awake([&](const struct device* d) {
			return hour_ring_.add(d, hour_val, ts);
		});
		if (rc < 0) {
			LOG_ERR("hour ring add failed (%d)", rc);
		} else {
			LOG_INF("hour ring: %d @ %u", static_cast<int>(hour_val), ts);
			hour_flow_ = 0.0;
		}

		/* Day boundary: hour=0 too. */
		if (dt.hour != 0) {
			continue;
		}
		const std::int32_t day_val = static_cast<std::int32_t>(day_flow_);
		rc = with_eeprom_awake([&](const struct device* d) {
			return day_ring_.add(d, day_val, ts);
		});
		if (rc < 0) {
			LOG_ERR("day ring add failed (%d)", rc);
		} else {
			LOG_INF("day ring: %d @ %u", static_cast<int>(day_val), ts);
			day_flow_ = 0.0;
		}

		/* Month boundary: day=1. */
		if (dt.day != 1) {
			continue;
		}
		const std::int32_t month_val = static_cast<std::int32_t>(month_flow_);
		rc = with_eeprom_awake([&](const struct device* d) {
			return month_ring_.add(d, month_val, ts);
		});
		if (rc < 0) {
			LOG_ERR("month ring add failed (%d)", rc);
		} else {
			LOG_INF("month ring: %d @ %u", static_cast<int>(month_val), ts);
			month_flow_ = 0.0;
		}
	}
}

} /* namespace */

std::optional<float> query(HistoryType type, std::uint32_t timestamp)
{
	if (eeprom_dev_ == nullptr) {
		return std::nullopt;
	}
	std::optional<std::int32_t> raw;
	int rc = with_eeprom_awake([&](const struct device* d) {
		switch (type) {
		case HistoryType::Hour:  raw = hour_ring_.find(d, timestamp);  break;
		case HistoryType::Day:   raw = day_ring_.find(d, timestamp);   break;
		case HistoryType::Month: raw = month_ring_.find(d, timestamp); break;
		}
		return 0;
	});
	if (rc < 0 || !raw.has_value()) {
		return std::nullopt;
	}
	return static_cast<float>(*raw);
}

const QueryResult& last_result()
{
	return g_last_result;
}

void set_last_result(const QueryResult& r)
{
	g_last_result = r;
}

int init(const struct device* eeprom)
{
	if (eeprom == nullptr) {
		return -EINVAL;
	}
	eeprom_dev_ = eeprom;
	/* Caller must ensure the EEPROM is awake before init — typically
	 * init() runs between Options::load and eeprom_enter_deep_power_
	 * down() at boot. Our load() calls are direct eeprom_read; no
	 * extra wake needed. */
	int rc = hour_ring_.load(eeprom);
	if (rc < 0) {
		LOG_ERR("hour ring load failed (%d)", rc);
		return rc;
	}
	rc = day_ring_.load(eeprom);
	if (rc < 0) {
		LOG_ERR("day ring load failed (%d)", rc);
		return rc;
	}
	rc = month_ring_.load(eeprom);
	if (rc < 0) {
		LOG_ERR("month ring load failed (%d)", rc);
		return rc;
	}
	LOG_INF("history: loaded sizes hour=%u day=%u month=%u",
		hour_ring_.size(), day_ring_.size(), month_ring_.size());
	return 0;
}

float hour_accumulator()  { return static_cast<float>(hour_flow_);  }
float day_accumulator()   { return static_cast<float>(day_flow_);   }
float month_accumulator() { return static_cast<float>(month_flow_); }

int start()
{
	if (history_tid != nullptr) {
		return 0;
	}
	history_tid = k_thread_create(
		&history_thread_data,
		history_stack,
		K_THREAD_STACK_SIZEOF(history_stack),
		history_tick_thread,
		nullptr, nullptr, nullptr,
		K_PRIO_PREEMPT(8),
		0,
		K_NO_WAIT);
	k_thread_name_set(history_tid, "history_tick");
	(void)timestamp_now;  /* reserved for future use in gap-fill audit */
	return 0;
}

} /* namespace uflow::history */
