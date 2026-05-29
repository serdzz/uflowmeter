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
#include <zephyr/sys/reboot.h>

#include "datetime.hpp"
#include "drivers/eeprom_power.hpp"
#include "drivers/hd44780.hpp"
#include "drivers/keypad.hpp"
#include "drivers/uart.hpp"
#include "history.hpp"
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

void render_under_mutex(uflow::ui::MenuController& mc)
{
	k_mutex_lock(&uflow::drivers::lcd_mutex, K_FOREVER);
	uflow::ui::render(mc, uflow::drivers::lcd());
	k_mutex_unlock(&uflow::drivers::lcd_mutex);
}

/* Wake the EEPROM, save Options, re-park the chip in DP. Two SPI
 * transactions per call — DP exit + RDP wait (~100 µs) plus the actual
 * eeprom_write (page-split + WIP poll). Keeps the EEPROM in DP for
 * the rest of the runtime; we only pay this cost on edit-commit. */
int save_options_through_dp(const struct device* eeprom, std::uint8_t* scratch)
{
	/* Serialize against modbus_handler and history_tick — all three
	 * pump in_dp_state through eeprom_power. */
	k_mutex_lock(&uflow::drivers::eeprom_mutex, K_FOREVER);
	int rc = uflow::drivers::eeprom_exit_deep_power_down();
	if (rc < 0) {
		LOG_ERR("eeprom wake failed (%d) — skipping save", rc);
		k_mutex_unlock(&uflow::drivers::eeprom_mutex);
		return rc;
	}
	rc = uflow::options::save(eeprom, uflow::options::g_options, scratch);
	if (rc < 0) {
		LOG_ERR("options save failed (%d)", rc);
	} else {
		LOG_INF("options saved");
	}
	int rc2 = uflow::drivers::eeprom_enter_deep_power_down();
	if (rc2 < 0) {
		LOG_WRN("eeprom re-DP failed (%d) — chip stays in standby", rc2);
	}
	k_mutex_unlock(&uflow::drivers::eeprom_mutex);
	return rc;
}

void handle_app_request(uflow::ui::MenuController& mc,
                        uflow::ui::AppRequest req,
                        const struct device* eeprom,
                        std::uint8_t* scratch)
{
	using uflow::ui::AppRequest;
	auto& opts = uflow::options::g_options;
	const std::uint8_t v = mc.last_edit_value();

	switch (req) {
	case AppRequest::None:
		return;
	case AppRequest::DeepSleep:
		/* No handler needed — Zephyr's PM policy enters STOP from
		 * the idle thread once all threads sleep. This request
		 * exists for symmetry with the embassy port where it
		 * triggered the explicit DeepSleep path. */
		LOG_INF("DeepSleep (idle PM handles STOP)");
		return;
	case AppRequest::EnterCalibration:
		LOG_INF("EnterCalibration → selecting Calibration menu");
		mc.select(uflow::ui::MenuId::Calibration);
		return;
	case AppRequest::SystemReset:
		LOG_INF("SystemReset → sys_reboot in 100 ms");
		k_msleep(100);  /* drain log buffer before reset */
		sys_reboot(SYS_REBOOT_COLD);
		return;
	case AppRequest::SetCommType:
		LOG_INF("SetCommType: %u", v);
		opts.comm_type = v;
		(void)save_options_through_dp(eeprom, scratch);
		return;
	case AppRequest::SetSlaveAddress:
		LOG_INF("SetSlaveAddress: %u", v);
		opts.slave_address = v;
		(void)save_options_through_dp(eeprom, scratch);
		return;
	case AppRequest::SetNegative:
		LOG_INF("SetNegative: %u", v);
		opts.enable_negative = v;
		(void)save_options_through_dp(eeprom, scratch);
		return;
	case AppRequest::SetSensorType:
		LOG_INF("SetSensorType: %u", v);
		opts.sensor_type = v;
		(void)save_options_through_dp(eeprom, scratch);
		return;
	case AppRequest::SetMuster:
		/* No Options home — log only. State lives on the
		 * MenuController and resets across reboot. */
		LOG_INF("SetMuster (memory-only): %u", v);
		return;
	case AppRequest::SetDateTime: {
		const auto& dt = mc.last_committed_datetime();
		LOG_INF("SetDateTime: %04u-%02u-%02u %02u:%02u:%02u",
			static_cast<unsigned>(dt.year),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
		uflow::datetime::set(dt);
		/* No EEPROM save — the datetime offset is RAM-only this
		 * commit. Future: persist a baseline timestamp in RTC
		 * backup register so wall-clock survives reboot. */
		return;
	}
	case AppRequest::SetHistory: {
		const auto kind = mc.last_history_kind();
		const std::uint32_t ts = mc.last_history_timestamp();
		const auto flow = uflow::history::query(kind, ts);
		uflow::history::set_last_result(
			uflow::history::QueryResult{kind, ts, flow});
		LOG_INF("SetHistory: kind=%u ts=%u value=%s",
			static_cast<unsigned>(kind), ts,
			flow.has_value() ? "<some>" : "<none>");
		return;
	}
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

	/* Load history rings before parking the EEPROM. CRC mismatch on
	 * any ring → that ring starts empty (harmless). */
	rc = uflow::history::init(eeprom);
	if (rc < 0) {
		LOG_ERR("history init failed (%d) — rings will be unavailable", rc);
	}

	rc = uflow::drivers::eeprom_enter_deep_power_down();
	if (rc < 0) {
		LOG_WRN("eeprom DP entry failed (%d) — chip stays in standby", rc);
	} else {
		LOG_INF("eeprom: deep power-down");
	}

	(void)uflow::history::start();
	(void)uflow::drivers::uart::start();

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

	/* Datetime — check RTC BKP0 magic, restore live RTC TR/DR or
	 * apply 2024-01-01 baseline. Must run before history_tick reads
	 * datetime::now(). */
	rc = uflow::datetime::init();
	if (rc < 0) {
		LOG_WRN("datetime init failed (%d) — wall clock unreliable", rc);
	}

	uflow::ui::MenuController controller;
	render_under_mutex(controller);

	uflow::drivers::KeyEvent ev{};
	for (;;) {
		/* Refresh tick: 150 ms while a multi-field edit is active
		 * (DateTime/History) so the blink animation runs at a
		 * perceptible rate, 2 s otherwise to minimize wake events
		 * for live-value updates (flow, uptime). The kernel auto-
		 * enters STOP between wakes via the PM policy. */
		const k_timeout_t timeout = controller.has_active_field_edit() ?
			K_MSEC(150) : K_MSEC(2000);
		const int kr = uflow::drivers::keypad_recv(ev, timeout);
		if (kr == 0) {
			uflow::ui::UiEvent ui_ev;
			if (uflow::ui::ui_event_from_input_code(ev.code, ui_ev)) {
				const auto req = controller.event(ui_ev);
				handle_app_request(controller, req, eeprom, options_scratch);
			}
		}
		render_under_mutex(controller);
	}
}
