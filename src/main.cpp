/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * uflowmeter — first commit on the zephyr branch.
 *
 * Brings up the HD44780 LCD and the 4-button keypad, then loops
 * receiving key events and echoing the button label to the LCD. The
 * rest of the firmware (TDC measurement, EEPROM, Modbus, history rings,
 * UI state machine) lands in follow-up commits — see
 * /Users/sergejlepin/.claude/plans/zephyr-linked-snowflake.md for the
 * roadmap.
 */

#include <string_view>

#include <zephyr/kernel.h>
#include <zephyr/dt-bindings/input/input-event-codes.h>
#include <zephyr/logging/log.h>

#include "drivers/hd44780.hpp"
#include "drivers/keypad.hpp"

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
	lcd.print("zephyr boot");

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
