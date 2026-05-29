/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * TDC1000 implementation. See tdc1000.hpp for the contract and the
 * register-format / power-cycle commentary.
 */

#include "tdc1000.hpp"

#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/spi.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(tdc1000, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers {

namespace {

#define TDC1000_NODE DT_CHOSEN(uflowmeter_tdc1000)

static_assert(DT_NODE_EXISTS(TDC1000_NODE),
	"DT chosen `uflowmeter,tdc1000` is missing — check your board DTS");

const struct spi_dt_spec tdc1000_spi = SPI_DT_SPEC_GET(
	TDC1000_NODE,
	SPI_OP_MODE_MASTER | SPI_TRANSFER_MSB | SPI_WORD_SET(8),
	0);

const struct gpio_dt_spec en_pin  = GPIO_DT_SPEC_GET(TDC1000_NODE, en_gpios);
const struct gpio_dt_spec rst_pin = GPIO_DT_SPEC_GET(TDC1000_NODE, rst_gpios);

constexpr std::uint8_t CMD_WRITE_BIT = 0x80;

constexpr std::uint8_t REG_CONFIG_2   = 0x02;
constexpr std::uint8_t REG_ERROR_FLAGS = 0x07;

int spi_xfer(const std::uint8_t* tx_buf, std::size_t tx_len,
             std::uint8_t* rx_buf, std::size_t rx_len)
{
	struct spi_buf tx_bufs[1] = {
		{ .buf = const_cast<std::uint8_t*>(tx_buf), .len = tx_len },
	};
	struct spi_buf rx_bufs[2] = {
		{ .buf = nullptr,                            .len = tx_len },
		{ .buf = rx_buf,                             .len = rx_len },
	};
	struct spi_buf_set tx_set = { .buffers = tx_bufs, .count = 1 };
	struct spi_buf_set rx_set = { .buffers = rx_bufs, .count = (rx_len > 0) ? 2u : 0u };

	if (rx_len == 0) {
		return spi_write_dt(&tdc1000_spi, &tx_set);
	}
	return spi_transceive_dt(&tdc1000_spi, &tx_set, &rx_set);
}

} /* namespace */

int Tdc1000::init()
{
	if (!spi_is_ready_dt(&tdc1000_spi)) {
		LOG_ERR("spi bus not ready");
		return -ENODEV;
	}
	if (!gpio_is_ready_dt(&en_pin) || !gpio_is_ready_dt(&rst_pin)) {
		LOG_ERR("gpio ports not ready");
		return -ENODEV;
	}
	/* EN starts inactive (chip off). RST starts ACTIVE — wait, no:
	 * we want the chip to NOT be in reset, so RST stays at its
	 * inactive level (electrically HIGH since the line is
	 * active-LOW). GPIO_OUTPUT_INACTIVE gives us that. */
	int rc = gpio_pin_configure_dt(&en_pin,  GPIO_OUTPUT_INACTIVE);
	if (rc < 0) {
		return rc;
	}
	rc = gpio_pin_configure_dt(&rst_pin, GPIO_OUTPUT_INACTIVE);
	if (rc < 0) {
		return rc;
	}
	return 0;
}

void Tdc1000::power_on()
{
	gpio_pin_set_dt(&en_pin, 1);
}

void Tdc1000::power_off()
{
	gpio_pin_set_dt(&en_pin, 0);
}

int Tdc1000::read_register(std::uint8_t address, std::uint8_t& out_value)
{
	const std::uint8_t cmd = address & 0x7F;  /* R/W bit cleared */
	std::uint8_t rx = 0;
	int rc = spi_xfer(&cmd, 1, &rx, 1);
	if (rc < 0) {
		return rc;
	}
	out_value = rx;
	return 0;
}

int Tdc1000::write_register(std::uint8_t address, std::uint8_t value)
{
	const std::uint8_t buf[2] = {
		static_cast<std::uint8_t>((address & 0x7F) | CMD_WRITE_BIT),
		value,
	};
	return spi_xfer(buf, sizeof(buf), nullptr, 0);
}

int Tdc1000::load_config(const std::uint8_t* regs, std::size_t len)
{
	for (std::size_t i = 0; i < len; i++) {
		int rc = write_register(static_cast<std::uint8_t>(i), regs[i]);
		if (rc < 0) {
			LOG_WRN("load_config: write reg %u failed (%d)", static_cast<unsigned>(i), rc);
			return rc;
		}
	}
	return 0;
}

int Tdc1000::set_channel(bool ch2)
{
	std::uint8_t v = 0;
	int rc = read_register(REG_CONFIG_2, v);
	if (rc < 0) {
		return rc;
	}
	if (ch2) {
		v |= 0x01;
	} else {
		v = static_cast<std::uint8_t>(v & ~0x01u);
	}
	return write_register(REG_CONFIG_2, v);
}

int Tdc1000::clear_error_flags()
{
	return write_register(REG_ERROR_FLAGS, 0xFF);
}

Tdc1000& tdc1000()
{
	static Tdc1000 instance;
	return instance;
}

} /* namespace uflow::drivers */
