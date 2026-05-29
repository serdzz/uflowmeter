/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * UI rendering with Cyrillic. Source strings are UTF-8; each codepoint
 * is rendered as ONE LCD column via:
 *   - ASCII (cp < 0x80): straight through.
 *   - Cyrillic with Latin lookalike: substitute the ROM byte (Р → P).
 *     Saves a CGRAM slot AND keeps the visual style consistent with
 *     adjacent Latin glyphs.
 *   - Cyrillic without lookalike: allocate next CGRAM slot (8 total
 *     per frame), upload the 5x8 pattern from drivers::cyrillic::FONT,
 *     emit the slot byte (0x00..0x07).
 *
 * CGRAM allocation state lives function-static on this TU and resets
 * at the top of every render() call. Within a frame, repeated
 * codepoints reuse the same slot. The per-frame budget is 8 unique
 * Cyrillic codepoints not in the lookalike table.
 *
 * If a frame exceeds 8 slots, the excess characters print as '?' so
 * the layout doesn't shift. Check `worst-case CGRAM` in the per-screen
 * comments below when adding new labels.
 */

#include "render.hpp"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string_view>

#include "../datetime.hpp"
#include "../drivers/cyrillic.hpp"
#include "../drivers/hd44780.hpp"
#include "../history.hpp"
#include "../measurement.hpp"
#include "../options.hpp"

#include <zephyr/kernel.h>

namespace uflow::ui {

namespace {

/* 2x16 panel — every line is 16 columns wide. */
constexpr std::size_t LINE_WIDTH = 16;
constexpr std::size_t CGRAM_SLOTS = 8;

/* Per-frame CGRAM bookkeeping. cached[i] = codepoint installed in
 * CGRAM slot i this frame, or 0 if unused. */
struct CgramState {
	char32_t cached[CGRAM_SLOTS];
	std::uint8_t used;

	void reset()
	{
		for (auto& c : cached) c = 0;
		used = 0;
	}

	/* Returns the slot byte (0..7) to emit for this codepoint, or
	 * -1 if no glyph exists and no slot available. Allocates on
	 * first use of a given codepoint within the frame. */
	int allocate(char32_t cp, drivers::Hd44780& lcd)
	{
		for (std::uint8_t i = 0; i < used; i++) {
			if (cached[i] == cp) {
				return i;
			}
		}
		if (used >= CGRAM_SLOTS) {
			return -1;
		}
		auto pattern = drivers::cyrillic::lookup(cp);
		if (!pattern.has_value()) {
			return -1;
		}
		const std::uint8_t* src = pattern.value();
		const std::uint8_t bytes[8] = {
			src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
		};
		lcd.upload_custom_char(used, bytes);
		cached[used] = cp;
		return used++;
	}
};

CgramState cgram_;

/* Minimal UTF-8 decoder. Handles 1- and 2-byte forms cleanly (Cyrillic
 * lives in the 2-byte band 0x0400..0x04FF). 3- and 4-byte forms get
 * '?' rather than mangling the layout. Returns the codepoint and
 * advances *pp. */
char32_t decode_utf8(const char*& pp, const char* end)
{
	if (pp >= end) return 0;
	const auto b0 = static_cast<std::uint8_t>(*pp++);
	if ((b0 & 0x80) == 0) {
		return b0;
	}
	if ((b0 & 0xE0) == 0xC0 && pp < end) {
		const auto b1 = static_cast<std::uint8_t>(*pp++);
		return (static_cast<char32_t>(b0 & 0x1F) << 6) |
		       static_cast<char32_t>(b1 & 0x3F);
	}
	/* Skip remaining continuation bytes of unknown wide sequences. */
	while (pp < end && ((static_cast<std::uint8_t>(*pp) & 0xC0) == 0x80)) {
		pp++;
	}
	return '?';
}

/* Emit one column for `cp`. Returns true if a byte was written. */
bool emit_codepoint(char32_t cp, drivers::Hd44780& lcd)
{
	char buf[1];

	if (cp < 0x80) {
		buf[0] = static_cast<char>(cp);
		lcd.print(std::string_view{buf, 1});
		return true;
	}
	if (auto la = drivers::cyrillic::latin_lookalike(cp); la.has_value()) {
		buf[0] = static_cast<char>(la.value());
		lcd.print(std::string_view{buf, 1});
		return true;
	}
	int slot = cgram_.allocate(cp, lcd);
	if (slot < 0) {
		buf[0] = '?';
		lcd.print(std::string_view{buf, 1});
		return true;
	}
	buf[0] = static_cast<char>(slot);
	lcd.print(std::string_view{buf, 1});
	return true;
}

/* Print a UTF-8 string at the current cursor + pad with spaces to
 * LINE_WIDTH columns. Truncates at LINE_WIDTH columns (not bytes).
 * Cursor must be set by caller. */
void print_line(drivers::Hd44780& lcd, std::string_view text)
{
	const char* p = text.data();
	const char* end = p + text.size();
	std::size_t cols = 0;
	while (p < end && cols < LINE_WIDTH) {
		char32_t cp = decode_utf8(p, end);
		if (cp == 0) break;
		emit_codepoint(cp, lcd);
		cols++;
	}
	for (std::size_t i = cols; i < LINE_WIDTH; i++) {
		lcd.print(" ");
	}
}

/* Title line per screen. Russian where it matters; ASCII for purely
 * technical labels (numbers, units). Worst CGRAM cost (across title
 * + value) noted per screen in inline comments — keep under 8. */
const char* title_for(ScreenId s)
{
	switch (s) {
	/* "Расход   Qм3/ч" — CGRAM: д, м, ч = 3. */
	case ScreenId::HourConsumption: return "Расход   Qм3/ч";
	/* "Расход Qм3/сут" — CGRAM: д, м, т = 3. */
	case ScreenId::DayConsumption:  return "Расход Qм3/сут";
	/* "Объем     Vм3 " — CGRAM: б, ъ, м = 3. */
	case ScreenId::TotalVolume:     return "Объем     Vм3";
	/* "Время работы" — CGRAM: м, я, б, т, ы = 5; value adds д ч = +2 = 7. */
	case ScreenId::Uptime:          return "Время работы";
	/* History/DateTime screens are painted by render_history /
	 * render_datetime — title_for is unused. */
	case ScreenId::HourHistory:
	case ScreenId::DayHistory:
	case ScreenId::MonthHistory:
	case ScreenId::DateTime:        return "";
	/* "Версия ПО" — CGRAM: и, я, П = 3. */
	case ScreenId::Version:         return "Версия ПО";
	/* "Обновить ПО" — CGRAM: б, н, в, и, т, ь, П = 7; value empty. */
	case ScreenId::Bootloader:      return "Обновить ПО";
	/* "Тип связи" — CGRAM: и, п, в, я, з = 5; value Cyrillic adds
	 * up to 2 more (Ы, Л) for "ВЫКЛ" — 7 worst. */
	case ScreenId::CommType:        return "Тип связи";
	/* "Адрес" — CGRAM: д = 1 (А, е, р, с all lookalikes). */
	case ScreenId::SlaveAddress:    return "Адрес";
	/* "Поверка" — CGRAM: П, в, к = 3; value adds Ы, Л = 5. */
	case ScreenId::Muster:          return "Поверка";
	/* "Реверс" — CGRAM: в = 1; value adds Ы, Л = 3. */
	case ScreenId::Negative:        return "Реверс";
	/* "01     луч 1" — CGRAM: л, у, ч = 3; value "отсутствует"
	 * adds т, в, у, ю, щ, и = 5 more = 8 total. Borderline. */
	case ScreenId::Channel1:        return "01     луч 1";
	case ScreenId::Channel2:        return "02     луч 2";
	/* "Датчик" — CGRAM: Д, т, ч, и = 4; value Cyrillic "ДУ40" adds
	 * Д(reused), У(У lookalike Y) = 0 more. ✓ */
	case ScreenId::SensorType:      return "Датчик";
	/* "Номер прибора" — CGRAM: м, п, и, б = 4. */
	case ScreenId::SerialNumber:    return "Номер прибора";
	/* "Калибровка" — CGRAM: л, и, б, в, к = 5. */
	case ScreenId::Calibration:     return "Калибровка";
	case ScreenId::_Count:          return "";
	}
	return "";
}

const char* comm_type_label(std::uint8_t v)
{
	switch (v) {
	case 0: return "ВЫКЛ";          /* CGRAM: Ы, Л = 2 */
	case 1: return "M-BUS";
	case 2: return "ModBus";
	case 3: return "Выход 4-20mA"; /* CGRAM: ы, х, д = 3 */
	default: return "?";
	}
}

const char* sensor_type_label(std::uint8_t v)
{
	switch (v) {
	case 0: return "ДУ40";   /* Д = CGRAM (1 new), У = У lookalike Y */
	case 1: return "ДУ50";
	case 2: return "ДУ65";
	case 3: return "ДУ80";
	case 4: return "ДУ100";
	default: return "?";
	}
}

const char* on_off(bool on) { return on ? "ВКЛ" : "ВКЛ"; /* placeholder — overridden below */ }

/* Russian on/off — `on` = "ВКЛ" (1 new CGRAM: Л), `off` = "ВЫКЛ"
 * (2 new: Ы, Л). */
const char* ru_on_off(bool on) { return on ? "ВКЛ" : "ВЫКЛ"; }

/* Value line composer. Buf must hold ≥48 bytes (16 cols × 3 bytes/cp
 * worst case + null + safety). */
void format_value(ScreenId s, char* buf, std::size_t cap,
                  bool editing, std::uint8_t edit_cursor)
{
	(void)on_off;  /* silence unused — kept as a hook for tests */
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
		const std::uint64_t up_s = static_cast<std::uint64_t>(k_uptime_get()) / 1000ULL;
		const std::uint32_t days = static_cast<std::uint32_t>(up_s / 86400ULL);
		const std::uint32_t hh   = static_cast<std::uint32_t>((up_s % 86400ULL) / 3600ULL);
		const std::uint32_t mm   = static_cast<std::uint32_t>((up_s % 3600ULL) / 60ULL);
		/* "Nд HHч MMм" — CGRAM д, ч, м. */
		snprintf(buf, cap, "%uд %02uч %02uм", days, hh, mm);
		break;
	}
	case ScreenId::HourHistory:
	case ScreenId::DayHistory:
	case ScreenId::MonthHistory:
	case ScreenId::DateTime:
		buf[0] = '\0';
		break;
	case ScreenId::Version:
		snprintf(buf, cap, "       zephyr-1");
		break;
	case ScreenId::Bootloader:
		buf[0] = '\0';
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
		const std::uint8_t v = edit_cursor;
		if (editing) {
			snprintf(buf, cap, "[%s]", ru_on_off(v != 0));
		} else {
			snprintf(buf, cap, "%s", ru_on_off(v != 0));
		}
		break;
	}
	case ScreenId::Negative: {
		const std::uint8_t v = editing ? edit_cursor :
			(opts.enable_negative != 0 ? 1 : 0);
		if (editing) {
			snprintf(buf, cap, "[%s]", ru_on_off(v != 0));
		} else {
			snprintf(buf, cap, "%s", ru_on_off(v != 0));
		}
		break;
	}
	case ScreenId::Channel1:
	case ScreenId::Channel2:
		/* "отсутствует" — CGRAM т, в, у, ю, щ, и = 5. With title's
		 * л, у, ч we share у so total = 7 unique. ✓ */
		snprintf(buf, cap, "отсутствует");
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
		buf[0] = '\0';
		break;
	case ScreenId::_Count:
		buf[0] = '\0';
		break;
	}
}

/* ────── DateTime painter ────────────────────────────────────────────
 * "Дата" = 4 cols (2 CGRAM: Д, т). "Время" = 5 cols (2 CGRAM: м, я).
 * Total per frame: 4 CGRAM. ✓
 */
void render_datetime(const MenuController& mc, drivers::Hd44780& lcd)
{
	const bool editing = mc.is_editing_datetime();
	const auto field = mc.current_datetime_field();
	const auto dt = editing ? mc.edited_datetime() : datetime::now();
	const std::uint8_t y2 = static_cast<std::uint8_t>(dt.year % 100u);

	char row[48];

	if (editing && (field == DateTimeEditField::Day ||
	                field == DateTimeEditField::Month ||
	                field == DateTimeEditField::Year)) {
		const char* fmt =
			(field == DateTimeEditField::Day)   ? "Дата  [%02u]/%02u/%02u"  :
			(field == DateTimeEditField::Month) ? "Дата  %02u/[%02u]/%02u"  :
			                                      "Дата  %02u/%02u/[%02u]";
		snprintf(row, sizeof(row), fmt,
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(y2));
	} else {
		snprintf(row, sizeof(row), "Дата    %02u/%02u/%02u",
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
			(field == DateTimeEditField::Hours)   ? "Время [%02u]:%02u:%02u" :
			(field == DateTimeEditField::Minutes) ? "Время %02u:[%02u]:%02u" :
			                                        "Время %02u:%02u:[%02u]";
		snprintf(row, sizeof(row), fmt,
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
	} else {
		snprintf(row, sizeof(row), "Время   %02u:%02u:%02u",
			static_cast<unsigned>(dt.hour),
			static_cast<unsigned>(dt.minute),
			static_cast<unsigned>(dt.second));
	}
	lcd.set_cursor(1, 0);
	print_line(lcd, row);
}

/* ────── History painter ─────────────────────────────────────────────
 * Constant label "Расход за" (9 cols, CGRAM: д, з). For Hour kind
 * the trailing 7 cols of row 0 hold " HH:00" / "[HH]:00". For Day /
 * Month kinds those cols are blank. Row 1: date + flow value
 * (all ASCII). Worst frame CGRAM: 2. ✓
 */
void render_history(const MenuController& mc, drivers::Hd44780& lcd)
{
	const ScreenId screen = mc.current_screen();
	const auto kind = (screen == ScreenId::DayHistory)   ? history::HistoryType::Day :
	                  (screen == ScreenId::MonthHistory) ? history::HistoryType::Month :
	                                                       history::HistoryType::Hour;
	const bool editing = mc.is_editing_history();
	const auto field   = mc.current_history_field();
	const auto dt      = editing ? mc.edited_history_datetime() : datetime::now();
	const std::uint8_t y2 = static_cast<std::uint8_t>(dt.year % 100u);

	char row[48];

	if (kind == history::HistoryType::Hour) {
		if (editing && field == HistoryEditField::Hour) {
			snprintf(row, sizeof(row), "Расход за[%02u]:00",
				static_cast<unsigned>(dt.hour));
		} else {
			snprintf(row, sizeof(row), "Расход за %02u:00",
				static_cast<unsigned>(dt.hour));
		}
	} else {
		snprintf(row, sizeof(row), "Расход за");
	}
	lcd.set_cursor(0, 0);
	print_line(lcd, row);

	char date_part[16];
	const bool show_day = (kind != history::HistoryType::Month);
	if (editing && field == HistoryEditField::Day && show_day) {
		snprintf(date_part, sizeof(date_part), "[%02u]/%02u/%02u",
			static_cast<unsigned>(dt.day),
			static_cast<unsigned>(dt.month),
			static_cast<unsigned>(y2));
	} else if (editing && field == HistoryEditField::Month) {
		if (show_day) {
			snprintf(date_part, sizeof(date_part), "%02u/[%02u]/%02u",
				static_cast<unsigned>(dt.day),
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		} else {
			snprintf(date_part, sizeof(date_part), "  /[%02u]/%02u",
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		}
	} else if (editing && field == HistoryEditField::Year) {
		if (show_day) {
			snprintf(date_part, sizeof(date_part), "%02u/%02u/[%02u]",
				static_cast<unsigned>(dt.day),
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		} else {
			snprintf(date_part, sizeof(date_part), "  /%02u/[%02u]",
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		}
	} else {
		if (show_day) {
			snprintf(date_part, sizeof(date_part), "%02u/%02u/%02u",
				static_cast<unsigned>(dt.day),
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		} else {
			snprintf(date_part, sizeof(date_part), "  /%02u/%02u",
				static_cast<unsigned>(dt.month),
				static_cast<unsigned>(y2));
		}
	}

	const auto& last = history::last_result();
	const std::uint32_t cursor_ts = datetime::to_timestamp(dt);
	const bool match = (last.type == kind) && (last.timestamp == cursor_ts);
	char flow_part[12];
	if (match && last.flow.has_value()) {
		snprintf(flow_part, sizeof(flow_part), "%7.2f", static_cast<double>(*last.flow));
	} else {
		snprintf(flow_part, sizeof(flow_part), "   None");
	}

	snprintf(row, sizeof(row), "%s%s", date_part, flow_part);
	lcd.set_cursor(1, 0);
	print_line(lcd, row);
}

} /* namespace */

void render(const MenuController& mc, drivers::Hd44780& lcd)
{
	/* Reset CGRAM bookkeeping at the top of every frame. Glyphs
	 * accumulate across the two rows but each frame starts clean. */
	cgram_.reset();

	lcd.set_cursor(0, 0);

	if (mc.active_menu() == MenuId::None) {
		/* CGRAM-free wake screen — keep everything ASCII so the
		 * user always sees something even if the LCD got a partial
		 * init. */
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
	if (screen == ScreenId::HourHistory ||
	    screen == ScreenId::DayHistory ||
	    screen == ScreenId::MonthHistory) {
		render_history(mc, lcd);
		return;
	}

	print_line(lcd, title_for(screen));

	char value_buf[48];
	value_buf[0] = '\0';
	format_value(screen, value_buf, sizeof(value_buf),
		mc.is_editing_current(),
		mc.edit_cursor_for_current());

	lcd.set_cursor(1, 0);
	print_line(lcd, value_buf);
}

} /* namespace uflow::ui */
