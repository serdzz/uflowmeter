/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Measurement-thread implementation. See measurement.hpp for the
 * cycle description and concurrency contract.
 */

#include "measurement.hpp"

#include <cmath>
#include <cstdio>
#include <cstring>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "calibration.hpp"
#include "drivers/hd44780.hpp"
#include "drivers/tdc1000.hpp"
#include "drivers/tdc7200.hpp"
#include "options.hpp"

LOG_MODULE_REGISTER(measurement, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::measurement {

std::atomic<float> latest_flow_m3h{std::nanf("")};

namespace {

constexpr std::size_t TDC_REG_COUNT = 10;

/* tof_max / tof_min on a healthy unit are roughly 10–100 µs of real
 * time → much faster than 50 ms. Anything longer than the budget is
 * a stuck transducer; we abort the cycle and try the next one. */
constexpr k_timeout_t TDC_INT_TIMEOUT = K_MSEC(50);

/* Stack for the measurement thread. 1 KB is generous — biggest live
 * frame is a 17-byte LCD line buffer + a CalibTable / MeterConfig. */
K_THREAD_STACK_DEFINE(measurement_stack, 1024);
struct k_thread measurement_thread_data;
k_tid_t measurement_tid = nullptr;

float bits_to_float(std::uint32_t bits)
{
	float f;
	std::memcpy(&f, &bits, sizeof(f));
	return f;
}

calibration::CalibTable calib_table_from_options(const options::Options& opts)
{
	using calibration::CalibData;
	using calibration::CalibTable;
	return CalibTable{
		.dtof0 = bits_to_float(opts.zero1),
		.data  = {
			CalibData{ .v = bits_to_float(opts.v11), .k = bits_to_float(opts.k11) },
			CalibData{ .v = bits_to_float(opts.v12), .k = bits_to_float(opts.k12) },
			CalibData{ .v = bits_to_float(opts.v13), .k = bits_to_float(opts.k13) },
		},
	};
}

calibration::MeterConfig meter_config_from_options(const options::Options& opts)
{
	auto cfg = calibration::MeterConfig::defaults();
	cfg.const_val = bits_to_float(opts.const_val);
	return cfg;
}

/* Run one channel: power must already be on + config loaded. Returns
 * true on success and writes the raw 24-bit TIME1 reading. */
bool single_measurement(drivers::Tdc7200& tdc, std::uint32_t& out_tof)
{
	int rc = tdc.start_measurement();
	if (rc < 0) {
		LOG_ERR("tdc7200 start_measurement failed: %d", rc);
		return false;
	}
	rc = tdc.wait_for_completion(TDC_INT_TIMEOUT);
	if (rc < 0) {
		LOG_WRN("tdc7200 INT timeout (%d)", rc);
		return false;
	}
	rc = tdc.read_time1(out_tof);
	if (rc < 0) {
		LOG_ERR("tdc7200 read_time1 failed: %d", rc);
		return false;
	}
	return true;
}

/* Draw the flow value on LCD row 1 under the shared mutex. Truncates
 * to 16 chars for a 2x16 panel. */
void display_flow(float flow)
{
	auto& lcd = drivers::lcd();
	char line[17];
	if (std::isfinite(flow)) {
		snprintf(line, sizeof(line), "Flow %8.2f m3/h", static_cast<double>(flow));
	} else {
		snprintf(line, sizeof(line), "Flow:    ---    ");
	}
	k_mutex_lock(&drivers::lcd_mutex, K_FOREVER);
	lcd.set_cursor(1, 0);
	lcd.print(line);
	k_mutex_unlock(&drivers::lcd_mutex);
}

void measurement_thread(void*, void*, void*)
{
	auto& tdc1000 = drivers::tdc1000();
	auto& tdc7200 = drivers::tdc7200();

	LOG_INF("measurement thread up");

	for (;;) {
		k_sleep(K_SECONDS(5));

		const auto& opts = options::g_options;
		const auto table = calib_table_from_options(opts);
		const calibration::Calculator calc(meter_config_from_options(opts));

		tdc1000.power_on();
		tdc7200.power_on();
		k_msleep(1);  /* regulator settle */

		int rc = tdc1000.load_config(opts.tdc1000_regs, TDC_REG_COUNT);
		if (rc < 0) {
			LOG_WRN("tdc1000 load_config failed (%d) — skipping cycle", rc);
			tdc1000.power_off();
			tdc7200.power_off();
			continue;
		}
		rc = tdc7200.load_config(opts.tdc7200_regs, TDC_REG_COUNT);
		if (rc < 0) {
			LOG_WRN("tdc7200 load_config failed (%d) — skipping cycle", rc);
			tdc1000.power_off();
			tdc7200.power_off();
			continue;
		}
		(void)tdc1000.clear_error_flags();

		std::uint32_t tof_down = 0;
		std::uint32_t tof_up   = 0;
		bool ok_down = false;
		bool ok_up   = false;

		if (tdc1000.set_channel(false) == 0) {
			ok_down = single_measurement(tdc7200, tof_down);
		}
		if (tdc1000.set_channel(true) == 0) {
			ok_up = single_measurement(tdc7200, tof_up);
		}

		tdc1000.power_off();
		tdc7200.power_off();

		if (ok_down && ok_up) {
			const float flow = calc.get_volume(
				table,
				static_cast<float>(tof_up),
				static_cast<float>(tof_down));
			LOG_INF("tof down=%u up=%u flow=%.4f m3/h",
				tof_down, tof_up, static_cast<double>(flow));
			if (std::isfinite(flow)) {
				latest_flow_m3h.store(flow, std::memory_order_relaxed);
				display_flow(flow);
			}
		} else {
			LOG_WRN("tof partial: down_ok=%d up_ok=%d", ok_down, ok_up);
		}
	}
}

} /* namespace */

int start()
{
	if (measurement_tid != nullptr) {
		return 0;  /* idempotent */
	}

	int rc = drivers::tdc1000().init();
	if (rc < 0) {
		LOG_ERR("tdc1000 init failed: %d", rc);
		return rc;
	}
	rc = drivers::tdc7200().init();
	if (rc < 0) {
		LOG_ERR("tdc7200 init failed: %d", rc);
		return rc;
	}

	measurement_tid = k_thread_create(
		&measurement_thread_data,
		measurement_stack,
		K_THREAD_STACK_SIZEOF(measurement_stack),
		measurement_thread,
		nullptr, nullptr, nullptr,
		K_PRIO_PREEMPT(7),
		0,
		K_NO_WAIT);
	k_thread_name_set(measurement_tid, "measurement");
	return 0;
}

} /* namespace uflow::measurement */
