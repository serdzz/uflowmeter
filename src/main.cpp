/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * uflowmeter — Zephyr port entrypoint.
 *
 * Boot sequence:
 *   1. Init LCD, show "uflowmeter / press any key".
 *   2. Load Options from EEPROM; populate options::g_options.
 *      Defaults on CRC failure.
 *   3. Park EEPROM in deep power-down.
 *   4. Init keypad (PB6-PB9 EXTI).
 *   5. Spawn measurement thread (writes only `latest_flow_m3h`, not
 *      the LCD — the UI owns the display from here on).
 *   6. Arm STOP-mode PM.
 *   7. Construct the UI MenuController and drive the run loop:
 *      block-wait for a keypress (with a 2 s refresh timeout so live
 *      values like flow + uptime update without input). Dispatch the
 *      key through MenuController. Re-render. Repeat.
 *
 * AppRequest dispatch is stubbed for this UI port commit (commit 1
 * of 6) — DeepSleep / SystemReset / EnterCalibration are logged but
 * not acted upon. Wired up alongside the edit-mode + Modbus commits.
 */

#include <cstdio>

#include <zephyr/device.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "drivers/eeprom_power.hpp"
#include "drivers/hd44780.hpp"
#include "drivers/keypad.hpp"
#include "measurement.hpp"
#include "options.hpp"
#include "power.hpp"
#include "ui/events.hpp"
#include "ui/menu_controller.hpp"
#include "ui/render.hpp"

LOG_MODULE_REGISTER(main, CONFIG_LOG_DEFAULT_LEVEL);

namespace {

/* 1024-byte scratch buffer for Options load/save. Lives in BSS, not on
 * the main stack (CONFIG_MAIN_STACK_SIZE is 2 KB; a 1 KB on-stack
 * buffer would consume half of it). */
std::uint8_t options_scratch[uflow::options::OPTIONS_PAGE_SIZE];

void render_under_mutex(const uflow::ui::MenuController& mc)
{
	k_mutex_lock(&uflow::drivers::lcd_mutex, K_FOREVER);
	uflow::ui::render(mc, uflow::drivers::lcd());
	k_mutex_unlock(&uflow::drivers::lcd_mutex);
}

void log_app_request(uflow::ui::AppRequest req)
{
	using uflow::ui::AppRequest;
	switch (req) {
	case AppRequest::None:             return;
	case AppRequest::DeepSleep:        LOG_INF("AppRequest: DeepSleep (no handler yet)"); return;
	case AppRequest::EnterCalibration: LOG_INF("AppRequest: EnterCalibration (no handler yet)"); return;
	case AppRequest::SystemReset:      LOG_INF("AppRequest: SystemReset (no handler yet)"); return;
	}
}

} /* namespace */

int main(void)
{
	LOG_INF("uflowmeter (zephyr): boot");

	auto& lcd = uflow::drivers::lcd();
	int rc = lcd.init();
	if (rc < 0) {
		LOG_ERR("lcd init failed (%d)", rc);
		return rc;
	}

	const struct device* eeprom = DEVICE_DT_GET(DT_CHOSEN(uflowmeter_eeprom));
	using uflow::options::LoadResult;
	const LoadResult lr = uflow::options::load(
		eeprom, uflow::options::g_options, options_scratch);
	switch (lr) {
	case LoadResult::OkPrimary:
		LOG_INF("options: loaded primary");
		break;
	case LoadResult::OkSecondary:
		LOG_WRN("options: primary corrupt, using secondary");
		break;
	case LoadResult::BothCorrupt:
		LOG_ERR("options: both copies corrupt — using defaults");
		uflow::options::g_options = uflow::options::Options{};
		break;
	case LoadResult::IoError:
		LOG_ERR("options: eeprom IO error — using defaults");
		uflow::options::g_options = uflow::options::Options{};
		break;
	}
	const auto& opts = uflow::options::g_options;
	LOG_INF("options: serial=%u sensor=%u slave_addr=%u comm=%u",
		opts.serial_number, opts.sensor_type,
		opts.slave_address, opts.comm_type);

	rc = uflow::drivers::eeprom_enter_deep_power_down();
	if (rc < 0) {
		LOG_WRN("eeprom DP entry failed (%d) — chip stays in standby", rc);
	} else {
		LOG_INF("eeprom: deep power-down");
	}

	rc = uflow::drivers::keypad_init();
	if (rc < 0) {
		LOG_ERR("keypad init failed (%d)", rc);
		return rc;
	}

	rc = uflow::measurement::start();
	if (rc < 0) {
		LOG_ERR("measurement start failed (%d)", rc);
		/* Non-fatal — keep serving keypad + LCD even with no flow. */
	}

	rc = uflow::power::init();
	if (rc < 0) {
		LOG_WRN("power init failed (%d) — STOP mode disabled", rc);
	} else {
		LOG_INF("power: STOP mode armed (RTC WUT wake)");
	}

	uflow::ui::MenuController controller;
	render_under_mutex(controller);

	uflow::drivers::KeyEvent ev{};
	for (;;) {
		/* 2 s refresh timeout keeps live values (flow, uptime)
		 * updating between key presses. Hit on every key, also
		 * fired periodically — render is idempotent + paints under
		 * the mutex so it's safe to call back-to-back. */
		const int kr = uflow::drivers::keypad_recv(ev, K_MSEC(2000));
		if (kr == 0) {
			uflow::ui::UiEvent ui_ev;
			if (uflow::ui::ui_event_from_input_code(ev.code, ui_ev)) {
				const auto req = controller.event(ui_ev);
				log_app_request(req);
			}
		}
		render_under_mutex(controller);
	}
}
