/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * uflowmeter — Zephyr port entrypoint.
 *
 * Brings up the HD44780 LCD, loads persisted Options from the on-board
 * 25LC1024 EEPROM, then arms the 4-button keypad and loops echoing key
 * labels to the LCD. The rest of the firmware (TDC measurement, Modbus,
 * history rings, UI state machine) lands in follow-up commits — see
 * /Users/sergejlepin/.claude/plans/zephyr-linked-snowflake.md.
 */

#include <cstdio>
#include <string_view>

#include <zephyr/device.h>
#include <zephyr/kernel.h>
#include <zephyr/dt-bindings/input/input-event-codes.h>
#include <zephyr/logging/log.h>

#include "drivers/hd44780.hpp"
#include "drivers/keypad.hpp"
#include "options.hpp"

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
	lcd.set_cursor(0, 0);
	lcd.print("uflowmeter");
	lcd.set_cursor(1, 0);
	lcd.print("loading opts");

	const struct device* eeprom = DEVICE_DT_GET(DT_CHOSEN(uflowmeter_eeprom));
	uflow::options::Options opts{};
	using uflow::options::LoadResult;
	const LoadResult lr = uflow::options::load(eeprom, opts, options_scratch);
	switch (lr) {
	case LoadResult::OkPrimary:
		LOG_INF("options: loaded primary");
		break;
	case LoadResult::OkSecondary:
		LOG_WRN("options: primary corrupt, using secondary");
		break;
	case LoadResult::BothCorrupt:
		LOG_ERR("options: both copies corrupt — using defaults");
		opts = uflow::options::Options{};
		break;
	case LoadResult::IoError:
		LOG_ERR("options: eeprom IO error — using defaults");
		opts = uflow::options::Options{};
		break;
	}
	LOG_INF("options: serial=%u sensor=%u slave_addr=%u comm=%u",
		opts.serial_number, opts.sensor_type,
		opts.slave_address, opts.comm_type);

	/* Show the serial number on the LCD so a human at the bench can
	 * read it off without a serial cable. */
	char line2[17];
	snprintf(line2, sizeof(line2), "SN:%u", opts.serial_number);
	lcd.clear();
	lcd.set_cursor(0, 0);
	lcd.print("uflowmeter");
	lcd.set_cursor(1, 0);
	lcd.print(line2);

	rc = uflow::drivers::keypad_init();
	if (rc < 0) {
		LOG_ERR("keypad init failed (%d)", rc);
		return rc;
	}

	uflow::drivers::KeyEvent ev{};
	for (;;) {
		if (uflow::drivers::keypad_recv(ev) < 0) {
			continue;
		}
		const auto label = label_for(ev.code);
		LOG_INF("key: %.*s", static_cast<int>(label.size()), label.data());
		lcd.clear();
		lcd.set_cursor(0, 0);
		lcd.print("key:");
		lcd.set_cursor(1, 0);
		lcd.print(label);
	}
}
