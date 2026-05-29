/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * uflowmeter — Zephyr port entrypoint.
 *
 * Boot sequence:
 *   1. Init LCD, show "loading opts" so the user sees something.
 *   2. Load Options from EEPROM (primary or secondary copy), populate
 *      the global options::g_options. Defaults on CRC failure.
 *   3. Park EEPROM in deep power-down to drop ~4 µA of standby current.
 *   4. Init keypad (PB6-PB9 EXTI).
 *   5. Spawn the measurement thread — runs every 5 s, writes flow on
 *      LCD row 1, publishes to atomic latest_flow_m3h.
 *   6. Main thread loops on keypad events, writes key label on LCD
 *      row 0 — coexists with the measurement thread via the
 *      lcd_mutex declared in drivers/hd44780.hpp.
 */

#include <cstdio>
#include <string_view>

#include <zephyr/device.h>
#include <zephyr/kernel.h>
#include <zephyr/dt-bindings/input/input-event-codes.h>
#include <zephyr/logging/log.h>

#include "drivers/eeprom_power.hpp"
#include "drivers/hd44780.hpp"
#include "drivers/keypad.hpp"
#include "measurement.hpp"
#include "options.hpp"
#include "power.hpp"

LOG_MODULE_REGISTER(main, CONFIG_LOG_DEFAULT_LEVEL);

namespace {

std::string_view label_for(std::uint16_t code)
{
	switch (code) {
	case INPUT_KEY_MENU:  return "CONFIG";
	case INPUT_KEY_ENTER: return "ENTER";
	case INPUT_KEY_LEFT:  return "DOWN";
	case INPUT_KEY_RIGHT: return "UP";
	default:              return "?";
	}
}

/* 1024-byte scratch buffer for Options load/save. Lives in BSS, not on
 * the main stack (CONFIG_MAIN_STACK_SIZE is 2 KB; a 1 KB on-stack
 * buffer would consume half of it). */
std::uint8_t options_scratch[uflow::options::OPTIONS_PAGE_SIZE];

void lcd_print_line(std::uint8_t row, std::string_view text)
{
	auto& lcd = uflow::drivers::lcd();
	k_mutex_lock(&uflow::drivers::lcd_mutex, K_FOREVER);
	lcd.set_cursor(row, 0);
	lcd.print(text);
	/* Pad to 16 chars so previous text doesn't bleed through. */
	for (std::size_t i = text.size(); i < 16; i++) {
		lcd.print(" ");
	}
	k_mutex_unlock(&uflow::drivers::lcd_mutex);
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
	lcd_print_line(0, "uflowmeter");
	lcd_print_line(1, "loading opts");

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

	char sn_line[17];
	snprintf(sn_line, sizeof(sn_line), "SN:%u", opts.serial_number);
	lcd_print_line(0, sn_line);
	lcd_print_line(1, "warming up...");

	uflow::drivers::KeyEvent ev{};
	for (;;) {
		if (uflow::drivers::keypad_recv(ev) < 0) {
			continue;
		}
		const auto label = label_for(ev.code);
		LOG_INF("key: %.*s", static_cast<int>(label.size()), label.data());
		char key_line[17];
		snprintf(key_line, sizeof(key_line), "key:%.*s",
			static_cast<int>(label.size()), label.data());
		lcd_print_line(0, key_line);
		/* Don't touch row 1 — that's the measurement thread's territory. */
	}
}
