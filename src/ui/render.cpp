/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "render.hpp"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string_view>

#include "../drivers/hd44780.hpp"
#include "../measurement.hpp"
#include "../options.hpp"

namespace uflow::ui {

namespace {

/* 2x16 panel — every line is 16 chars wide. */
constexpr std::size_t LINE_WIDTH = 16;

/* Print `text` at the current cursor and pad with spaces to LINE_WIDTH
 * so previous content doesn't bleed through. Truncates text longer
 * than LINE_WIDTH. */
void print_line(drivers::Hd44780& lcd, std::string_view text)
{
	if (text.size() >= LINE_WIDTH) {
		lcd.print(std::string_view{text.data(), LINE_WIDTH});
		return;
	}
	lcd.print(text);
	for (std::size_t i = text.size(); i < LINE_WIDTH; i++) {
		lcd.print(" ");
	}
}

/* Title line (row 0) per screen. The embassy version uses Cyrillic;
 * we'll port CGRAM glyph support in a follow-up commit and keep Latin
 * here. Stable identifiers — users will retrain on the labels but
 * function won't change.
 *
 * Two-letter prefixes (HF, DF, MV…) make every title distinguishable
 * at a glance even before muscle-memory kicks in. */
const char* title_for(ScreenId s)
{
	switch (s) {
	case ScreenId::HourConsumption: return "Flow rate m3/h";
	case ScreenId::DayConsumption:  return "Daily m3/day";
	case ScreenId::TotalVolume:     return "Volume m3";
	case ScreenId::Uptime:          return "Uptime";
	case ScreenId::HourHistory:     return "Hourly history";
	case ScreenId::DayHistory:      return "Daily history";
	case ScreenId::MonthHistory:    return "Monthly history";
	case ScreenId::DateTime:        return "Date / time";
	case ScreenId::Version:         return "Firmware ver.";
	case ScreenId::Bootloader:      return "Update fw";
	case ScreenId::CommType:        return "Comm type";
	case ScreenId::SlaveAddress:    return "Slave address";
	case ScreenId::Muster:          return "Verification";
	case ScreenId::Negative:        return "Reverse flow";
	case ScreenId::Channel1:        return "01      beam 1";
	case ScreenId::Channel2:        return "02      beam 2";
	case ScreenId::SensorType:      return "Sensor";
	case ScreenId::SerialNumber:    return "Serial number";
	case ScreenId::Calibration:     return "Calibration";
	case ScreenId::_Count:          return "";
	}
	return "";
}

/* Comm-type label cycle from rework/embassy:src/ui.rs format_value. */
const char* comm_type_label(std::uint8_t v)
{
	switch (v) {
	case 0: return "Off";
	case 1: return "M-BUS";
	case 2: return "ModBus";
	case 3: return "4-20mA out";
	default: return "?";
	}
}

const char* sensor_type_label(std::uint8_t v)
{
	switch (v) {
	case 0: return "DN40";
	case 1: return "DN50";
	case 2: return "DN65";
	case 3: return "DN80";
	case 4: return "DN100";
	default: return "?";
	}
}

const char* on_off(bool on) { return on ? "ON" : "OFF"; }

/* Value line (row 1). Returns by value to keep call sites short.
 * Buffer must be ≥ LINE_WIDTH+1; we cap formats at LINE_WIDTH. */
void format_value(ScreenId s, char* buf, std::size_t cap)
{
	const auto& opts = options::g_options;
	const float flow = measurement::latest_flow_m3h.load(std::memory_order_relaxed);

	switch (s) {
	case ScreenId::HourConsumption:
	case ScreenId::DayConsumption:
	case ScreenId::TotalVolume: {
		/* No accumulators yet — show live flow on all three until
		 * the history-rings commit wires up accumulators. */
		if (std::isfinite(flow)) {
			snprintf(buf, cap, "%15.3f", static_cast<double>(flow));
		} else {
			snprintf(buf, cap, "         no data");
		}
		break;
	}
	case ScreenId::Uptime: {
		/* k_uptime is in ms; we use it as a stand-in for the
		 * persisted uptime until backup-register handling lands.
		 * NOTE: the LSI timer drifts ±10-15% per the power
		 * commit's caveat, so this number is approximate. */
		const std::uint64_t up_s = static_cast<std::uint64_t>(k_uptime_get()) / 1000ULL;
		const std::uint32_t days = static_cast<std::uint32_t>(up_s / 86400ULL);
		const std::uint32_t hh   = static_cast<std::uint32_t>((up_s % 86400ULL) / 3600ULL);
		const std::uint32_t mm   = static_cast<std::uint32_t>((up_s % 3600ULL) / 60ULL);
		snprintf(buf, cap, "%ud %02uh %02um", days, hh, mm);
		break;
	}
	case ScreenId::HourHistory:
	case ScreenId::DayHistory:
	case ScreenId::MonthHistory:
		/* History rings not wired yet — placeholder. */
		snprintf(buf, cap, "    (no rings)");
		break;
	case ScreenId::DateTime:
		/* RTC datetime not exposed yet — placeholder. */
		snprintf(buf, cap, " ---- -- -- --");
		break;
	case ScreenId::Version:
		snprintf(buf, cap, "       zephyr-1");
		break;
	case ScreenId::Bootloader:
		snprintf(buf, cap, "    press Enter");
		break;
	case ScreenId::CommType:
		snprintf(buf, cap, "%s", comm_type_label(opts.comm_type));
		break;
	case ScreenId::SlaveAddress:
		snprintf(buf, cap, "%u", opts.slave_address);
		break;
	case ScreenId::Muster:
		/* `enable_negative` doubles as the Muster slot in legacy;
		 * the edit-mode commit untangles the actual fields. */
		snprintf(buf, cap, "%s", on_off(false));
		break;
	case ScreenId::Negative:
		snprintf(buf, cap, "%s", on_off(opts.enable_negative != 0));
		break;
	case ScreenId::Channel1:
	case ScreenId::Channel2:
		snprintf(buf, cap, "absent");
		break;
	case ScreenId::SensorType:
		snprintf(buf, cap, "%s", sensor_type_label(opts.sensor_type));
		break;
	case ScreenId::SerialNumber:
		snprintf(buf, cap, "%u", opts.serial_number);
		break;
	case ScreenId::Calibration:
		snprintf(buf, cap, "    press Enter");
		break;
	case ScreenId::_Count:
		buf[0] = '\0';
		break;
	}
}

} /* namespace */

void render(const MenuController& mc, drivers::Hd44780& lcd)
{
	lcd.set_cursor(0, 0);

	if (mc.active_menu() == MenuId::None) {
		print_line(lcd, "uflowmeter");
		lcd.set_cursor(1, 0);
		print_line(lcd, "press any key");
		return;
	}

	const ScreenId screen = mc.current_screen();
	print_line(lcd, title_for(screen));

	char value_buf[LINE_WIDTH + 1];
	value_buf[0] = '\0';
	format_value(screen, value_buf, sizeof(value_buf));

	lcd.set_cursor(1, 0);
	print_line(lcd, value_buf);
}

} /* namespace uflow::ui */
