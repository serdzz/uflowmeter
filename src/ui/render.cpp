/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "render.hpp"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string_view>

#include "../datetime.hpp"
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
 * Buffer must be ≥ LINE_WIDTH+1; we cap formats at LINE_WIDTH.
 *
 * `editing` + `edit_cursor` are the controller's view of the in-
 * progress edit. When `editing`, value gets wrapped in [brackets]
 * and reflects the cursor instead of the persisted Options field.
 * No blink animation yet (CGRAM commit territory). */
void format_value(ScreenId s, char* buf, std::size_t cap,
                  bool editing, std::uint8_t edit_cursor)
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
		/* Drawn from the two-line renderer in render() — this code
		 * path is only hit for screens that share the single value-
		 * line layout. DateTime has its own painter. */
		buf[0] = '\0';
		break;
	case ScreenId::Version:
		snprintf(buf, cap, "       zephyr-1");
		break;
	case ScreenId::Bootloader:
		snprintf(buf, cap, "    press Enter");
		break;
	case ScreenId::CommType: {
		const std::uint8_t v = editing ? edit_cursor : opts.comm_type;
		if (editing) {
			snprintf(buf, cap, "[%s]", comm_type_label(v));
		} else {
			snprintf(buf, cap, "%s", comm_type_label(v));
		}
		break;
	}
	case ScreenId::SlaveAddress: {
		const std::uint8_t v = editing ? edit_cursor : opts.slave_address;
		if (editing) {
			snprintf(buf, cap, "[%u]", v);
		} else {
			snprintf(buf, cap, "%u", v);
		}
		break;
	}
	case ScreenId::Muster: {
		/* Memory-only flag (no Options home this commit). When not
		 * editing we still show the controller's last state, which
		 * is what the user just confirmed. Display passes through
		 * `editing+edit_cursor` for both modes. */
		const std::uint8_t v = edit_cursor;
		if (editing) {
			snprintf(buf, cap, "[%s]", on_off(v != 0));
		} else {
			snprintf(buf, cap, "%s", on_off(v != 0));
		}
		break;
	}
	case ScreenId::Negative: {
		const std::uint8_t v = editing ? edit_cursor :
			(opts.enable_negative != 0 ? 1 : 0);
		if (editing) {
			snprintf(buf, cap, "[%s]", on_off(v != 0));
		} else {
			snprintf(buf, cap, "%s", on_off(v != 0));
		}
		break;
	}
	case ScreenId::Channel1:
	case ScreenId::Channel2:
		snprintf(buf, cap, "absent");
		break;
	case ScreenId::SensorType: {
		const std::uint8_t v = editing ? edit_cursor : opts.sensor_type;
		if (editing) {
			snprintf(buf, cap, "[%s]", sensor_type_label(v));
		} else {
			snprintf(buf, cap, "%s", sensor_type_label(v));
		}
		break;
	}
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

namespace {

/* Paint the DateTime screen — overrides the standard title+value
 * single-line layout because date and time both need their own row.
 * When editing, wraps the active field in [brackets]; the other
 * fields show the in-progress working buffer (so the user sees the
 * effect of inc_X / dec_X immediately). When not editing, both rows
 * source from datetime::now().
 *
 * 16-char per row layout:
 *   non-edit:   "Date    DD/MM/YY"  (4 + 4 + 8 = 16)
 *   editing:    "Date  [DD]/MM/YY"  (4 + 2 + 10 = 16)
 */
void render_datetime(const MenuController& mc, drivers::Hd44780& lcd)
{
	const bool editing = mc.is_editing_datetime();
	const auto field = mc.current_datetime_field();
	const auto dt = editing ? mc.edited_datetime() : datetime::now();
	const std::uint8_t y2 = static_cast<std::uint8_t>(dt.year % 100u);

	char row[LINE_WIDTH + 1];

	if (editing && (field == DateTimeEditField::Day ||
	                field == DateTimeEditField::Month ||
	                field == DateTimeEditField::Year)) {
		const char* fmt =
			(field == DateTimeEditField::Day)   ? "Date  [%02u]/%02u/%02u"  :
			(field == DateTimeEditField::Month) ? "Date  %02u/[%02u]/%02u"  :
			                                      "Date  %02u/%02u/[%02u]";
		snprintf(row, sizeof(row), fmt,
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(y2));
	} else {
		snprintf(row, sizeof(row), "Date    %02u/%02u/%02u",
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(y2));
	}
	lcd.set_cursor(0, 0);
	print_line(lcd, row);

	if (editing && (field == DateTimeEditField::Hours ||
	                field == DateTimeEditField::Minutes ||
	                field == DateTimeEditField::Seconds)) {
		const char* fmt =
			(field == DateTimeEditField::Hours)   ? "Time  [%02u]:%02u:%02u" :
			(field == DateTimeEditField::Minutes) ? "Time  %02u:[%02u]:%02u" :
			                                        "Time  %02u:%02u:[%02u]";
		snprintf(row, sizeof(row), fmt,
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
	} else {
		snprintf(row, sizeof(row), "Time    %02u:%02u:%02u",
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
	}
	lcd.set_cursor(1, 0);
	print_line(lcd, row);
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
	if (screen == ScreenId::DateTime) {
		render_datetime(mc, lcd);
		return;
	}

	print_line(lcd, title_for(screen));

	char value_buf[LINE_WIDTH + 1];
	value_buf[0] = '\0';
	format_value(screen, value_buf, sizeof(value_buf),
		mc.is_editing_current(),
		mc.edit_cursor_for_current());

	lcd.set_cursor(1, 0);
	print_line(lcd, value_buf);
}

} /* namespace uflow::ui */
