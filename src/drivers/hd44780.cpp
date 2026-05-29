/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * HD44780 4-bit driver implementation. See hd44780.hpp for the API
 * contract and the parity statement vs. the embassy-era Rust driver.
 *
 * Timing budget (per HD44780U datasheet, verified empirically on this
 * board's panel during the embassy port):
 *   - Internal power-up    >=  40 ms  (we wait 50)
 *   - Function-set settle  ~   5 ms after first 0x3 nibble
 *   - Subsequent init      ~ 150 µs between nibbles
 *   - clear() / home       ~ 1.5 ms  (we wait 2)
 *   - Generic command/data ~  40 µs internal cycle
 *   - E-pulse width        >= 230 ns (we hold 1 µs for level-shifter slack)
 */

#include "hd44780.hpp"

#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(hd44780, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers {

K_MUTEX_DEFINE(lcd_mutex);

namespace {

#define LCD_NODE DT_CHOSEN(uflowmeter_lcd)

static_assert(DT_NODE_EXISTS(LCD_NODE),
	"DT chosen `uflowmeter,lcd` is missing — check your board DTS");

const struct gpio_dt_spec rs_pin = GPIO_DT_SPEC_GET(LCD_NODE, rs_gpios);
const struct gpio_dt_spec rw_pin = GPIO_DT_SPEC_GET_OR(LCD_NODE, rw_gpios, {0});
const struct gpio_dt_spec e_pin  = GPIO_DT_SPEC_GET(LCD_NODE, e_gpios);
const struct gpio_dt_spec d4_pin = GPIO_DT_SPEC_GET(LCD_NODE, d4_gpios);
const struct gpio_dt_spec d5_pin = GPIO_DT_SPEC_GET(LCD_NODE, d5_gpios);
const struct gpio_dt_spec d6_pin = GPIO_DT_SPEC_GET(LCD_NODE, d6_gpios);
const struct gpio_dt_spec d7_pin = GPIO_DT_SPEC_GET(LCD_NODE, d7_gpios);

constexpr const gpio_dt_spec* data_lines[4] = {&d4_pin, &d5_pin, &d6_pin, &d7_pin};

void set_data_bit(std::uint8_t idx, std::uint8_t value)
{
	gpio_pin_set_dt(data_lines[idx], value);
}

int configure_outputs()
{
	const gpio_dt_spec* outs[] = {&rs_pin, &e_pin, &d4_pin, &d5_pin, &d6_pin, &d7_pin};
	for (const auto* pin : outs) {
		if (!gpio_is_ready_dt(pin)) {
			LOG_ERR("gpio port not ready for pin %u", pin->pin);
			return -ENODEV;
		}
		int rc = gpio_pin_configure_dt(pin, GPIO_OUTPUT_INACTIVE);
		if (rc < 0) {
			LOG_ERR("gpio_pin_configure_dt failed (%d) for pin %u", rc, pin->pin);
			return rc;
		}
	}
	/* RW is optional. If the binding declared it, force it LOW (write-only). */
	if (rw_pin.port != nullptr) {
		if (!gpio_is_ready_dt(&rw_pin)) {
			return -ENODEV;
		}
		int rc = gpio_pin_configure_dt(&rw_pin, GPIO_OUTPUT_INACTIVE);
		if (rc < 0) {
			return rc;
		}
	}
	return 0;
}

} /* namespace */

int Hd44780::init()
{
	int rc = configure_outputs();
	if (rc < 0) {
		return rc;
	}

	/* Wait for LCD internal power-up (datasheet says >=40 ms). */
	k_msleep(50);

	/* Three function-set 0x3 nibbles to force 8-bit mode, then drop to 4. */
	gpio_pin_set_dt(&rs_pin, 0);
	write_nibble(0x3);
	k_msleep(5);
	write_nibble(0x3);
	k_busy_wait(150);
	write_nibble(0x3);
	k_busy_wait(150);
	/* 4-bit mode select. */
	write_nibble(0x2);
	k_busy_wait(150);

	command(0x28);  /* function-set: 4-bit, 2-line, 5x8 font */
	command(0x08);  /* display off */
	command(0x01);  /* clear */
	k_msleep(2);
	command(0x06);  /* entry mode: increment, no shift */
	command(0x0C);  /* display on, cursor off, blink off */

	cursor_col_ = 0;
	cursor_row_ = 0;
	return 0;
}

void Hd44780::clear()
{
	command(0x01);
	k_msleep(2);
	cursor_col_ = 0;
	cursor_row_ = 0;
}

void Hd44780::set_cursor(std::uint8_t row, std::uint8_t col)
{
	const std::uint8_t addr = (row == 0) ? col : (0x40 | col);
	command(0x80 | addr);
	cursor_col_ = col;
	cursor_row_ = row;
}

void Hd44780::print(std::string_view text)
{
	for (char c : text) {
		data(static_cast<std::uint8_t>(c));
	}
}

void Hd44780::command(std::uint8_t byte)
{
	gpio_pin_set_dt(&rs_pin, 0);
	write_byte(byte);
	k_busy_wait(40);
}

void Hd44780::data(std::uint8_t byte)
{
	gpio_pin_set_dt(&rs_pin, 1);
	write_byte(byte);
	k_busy_wait(40);
}

void Hd44780::write_byte(std::uint8_t byte)
{
	write_nibble(byte >> 4);
	write_nibble(byte & 0x0F);
}

void Hd44780::write_nibble(std::uint8_t nibble)
{
	set_data_bit(0, nibble & 0x1);
	set_data_bit(1, (nibble >> 1) & 0x1);
	set_data_bit(2, (nibble >> 2) & 0x1);
	set_data_bit(3, (nibble >> 3) & 0x1);
	/* Pulse E: high for >=230 ns @ 5V, we hold 1 µs to be safe at 3.3 V
	 * with level shifters. */
	gpio_pin_set_dt(&e_pin, 1);
	k_busy_wait(1);
	gpio_pin_set_dt(&e_pin, 0);
	k_busy_wait(1);
}

Hd44780& lcd()
{
	static Hd44780 instance;
	return instance;
}

/* Power-gate pins. Sourced from the same DT aliases declared in the
 * board overlay — gpio-leds children whose default state is "off"
 * (electrically HIGH on active-LOW pins). gpio_pin_set_dt() with
 * value 1 = "on" = electrically LOW; value 0 = "off" = electrically
 * HIGH. */
namespace {

const struct gpio_dt_spec lcd_power_spec     = GPIO_DT_SPEC_GET(DT_NODELABEL(lcd_power), gpios);
const struct gpio_dt_spec lcd_backlight_spec = GPIO_DT_SPEC_GET(DT_NODELABEL(backlight), gpios);

void ensure_power_pins_configured()
{
	static bool configured = false;
	if (configured) {
		return;
	}
	if (gpio_is_ready_dt(&lcd_power_spec)) {
		(void)gpio_pin_configure_dt(&lcd_power_spec, GPIO_OUTPUT_ACTIVE);
	}
	if (gpio_is_ready_dt(&lcd_backlight_spec)) {
		(void)gpio_pin_configure_dt(&lcd_backlight_spec, GPIO_OUTPUT_INACTIVE);
	}
	configured = true;
}

} /* namespace */

void lcd_power_on()
{
	ensure_power_pins_configured();
	gpio_pin_set_dt(&lcd_power_spec, 1);
}

void lcd_power_off()
{
	gpio_pin_set_dt(&lcd_power_spec, 0);
}

void lcd_backlight_on()
{
	ensure_power_pins_configured();
	gpio_pin_set_dt(&lcd_backlight_spec, 1);
}

void lcd_backlight_off()
{
	gpio_pin_set_dt(&lcd_backlight_spec, 0);
}

} /* namespace uflow::drivers */
