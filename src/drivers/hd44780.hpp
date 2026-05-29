/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Minimal HD44780 character-LCD driver, 4-bit parallel mode.
 *
 * Ported from the embassy-stm32 Rust driver at
 * git show rework/embassy:src/drivers/hd44780.rs — same init sequence,
 * same timing table, just on Zephyr's GPIO API instead of embassy's
 * `Output` pins and `Timer`.
 *
 * Pin bindings come from the DT chosen node `uflowmeter,lcd`
 * (see boards/uflowmeter/uflowmeter_v1/uflowmeter_v1.dts).
 */

#pragma once

#include <cstdint>
#include <string_view>

#include <zephyr/kernel.h>

namespace uflow::drivers {

/* Shared LCD mutex. Multiple threads write to the LCD (main thread for
 * keypress feedback, measurement thread for flow display); acquire
 * this before any sequence of LCD ops to keep cursor moves and
 * subsequent characters atomic per-thread.
 *
 * Defined in hd44780.cpp via K_MUTEX_DEFINE. */
extern struct k_mutex lcd_mutex;

class Hd44780 {
public:
	/* Run the 4-bit init dance. Returns 0 on success, negative errno
	 * on GPIO configuration failure. Safe to call again to re-init
	 * after the LCD has been power-cycled. */
	int init();

	/* DDRAM clear + cursor home. ~1.6 ms blocking. */
	void clear();

	/* row: 0 = top, 1 = bottom (2x16 panel). col: 0-based. */
	void set_cursor(std::uint8_t row, std::uint8_t col);

	/* Write ASCII at the current cursor position. No wrapping. */
	void print(std::string_view text);

	/* Upload an 8-byte 5x8 glyph into CGRAM slot 0..7. Restores the
	 * DDRAM cursor afterward so the next print() lands where it was.
	 * Pattern bit 4 = leftmost pixel; bits 7..5 are ignored. Used by
	 * the UI renderer to support Cyrillic glyphs not present in the
	 * HD44780 character ROM. */
	void upload_custom_char(std::uint8_t slot, const std::uint8_t (&pattern)[8]);

private:
	void write_nibble(std::uint8_t nibble);
	void write_byte(std::uint8_t byte);
	void command(std::uint8_t byte);
	void data(std::uint8_t byte);

	std::uint8_t cursor_col_{0};
	std::uint8_t cursor_row_{0};
};

/* Singleton — there's exactly one LCD on this board, declared in DT. */
Hd44780& lcd();

/* Power gate control. PC0 (active-LOW) drops LCD VCC; PC5 (active-LOW)
 * drops the backlight. Cutting VCC loses display state — caller is
 * responsible for re-running lcd().init() after lcd_power_on(). The
 * STOP-mode pm hook in power.cpp drives both. */
void lcd_power_on();
void lcd_power_off();
void lcd_backlight_on();
void lcd_backlight_off();

} /* namespace uflow::drivers */
