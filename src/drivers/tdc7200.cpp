/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * TDC7200 implementation. See tdc7200.hpp for the contract.
 *
 * The INT line wiring: a single static gpio_callback fires on falling
 * edge of PB0, gives a static k_sem. The measurement thread resets
 * the sem before kicking off start_measurement, then blocks in
 * wait_for_completion. Stale wakes from prior cycles (if any) are
 * discarded by the sem-reset.
 */

#include "tdc7200.hpp"

#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/spi.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(tdc7200, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers {

namespace {

#define TDC7200_NODE DT_CHOSEN(uflowmeter_tdc7200)

static_assert(DT_NODE_EXISTS(TDC7200_NODE),
	"DT chosen `uflowmeter,tdc7200` is missing — check your board DTS");

const struct spi_dt_spec tdc7200_spi = SPI_DT_SPEC_GET(
	TDC7200_NODE,
	SPI_OP_MODE_MASTER | SPI_TRANSFER_MSB | SPI_WORD_SET(8),
	0);

const struct gpio_dt_spec en_pin  = GPIO_DT_SPEC_GET(TDC7200_NODE, en_gpios);
const struct gpio_dt_spec int_pin = GPIO_DT_SPEC_GET(TDC7200_NODE, int_gpios);

constexpr std::uint8_t CMD_WRITE_BIT     = 0x40;  /* sets auto-inc + write */
constexpr std::uint8_t REG_CONFIG1       = 0x00;
constexpr std::uint8_t CONFIG1_START_MEAS = 0x01;
constexpr std::uint8_t REG_TIME1         = 0x10;

K_SEM_DEFINE(int_sem, 0, 1);
struct gpio_callback int_cb;

void int_isr(const struct device*, struct gpio_callback*, gpio_port_pins_t)
{
	k_sem_give(&int_sem);
}

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
		return spi_write_dt(&tdc7200_spi, &tx_set);
	}
	return spi_transceive_dt(&tdc7200_spi, &tx_set, &rx_set);
}

} /* namespace */

int Tdc7200::init()
{
	if (!spi_is_ready_dt(&tdc7200_spi)) {
		LOG_ERR("spi bus not ready");
		return -ENODEV;
	}
	if (!gpio_is_ready_dt(&en_pin) || !gpio_is_ready_dt(&int_pin)) {
		LOG_ERR("gpio ports not ready");
		return -ENODEV;
	}
	int rc = gpio_pin_configure_dt(&en_pin, GPIO_OUTPUT_INACTIVE);
	if (rc < 0) {
		return rc;
	}
	rc = gpio_pin_configure_dt(&int_pin, GPIO_INPUT);
	if (rc < 0) {
		return rc;
	}
	/* GPIO_INT_EDGE_TO_ACTIVE: trigger on the edge that goes from
	 * inactive to active. INT is declared active-LOW in DT, so this
	 * is the falling edge — same semantics as embassy's
	 * wait_for_falling_edge. */
	rc = gpio_pin_interrupt_configure_dt(&int_pin, GPIO_INT_EDGE_TO_ACTIVE);
	if (rc < 0) {
		return rc;
	}
	gpio_init_callback(&int_cb, int_isr, BIT(int_pin.pin));
	rc = gpio_add_callback(int_pin.port, &int_cb);
	if (rc < 0) {
		LOG_ERR("gpio_add_callback failed (%d)", rc);
		return rc;
	}
	return 0;
}

void Tdc7200::power_on()
{
	gpio_pin_set_dt(&en_pin, 1);
}

void Tdc7200::power_off()
{
	gpio_pin_set_dt(&en_pin, 0);
}

int Tdc7200::read_register(std::uint8_t address, std::uint8_t& out_value)
{
	const std::uint8_t cmd = address & 0x1F;
	std::uint8_t rx = 0;
	int rc = spi_xfer(&cmd, 1, &rx, 1);
	if (rc < 0) {
		return rc;
	}
	out_value = rx;
	return 0;
}

int Tdc7200::write_register(std::uint8_t address, std::uint8_t value)
{
	const std::uint8_t buf[2] = {
		static_cast<std::uint8_t>((address & 0x1F) | CMD_WRITE_BIT),
		value,
	};
	return spi_xfer(buf, sizeof(buf), nullptr, 0);
}

int Tdc7200::read_u24(std::uint8_t address, std::uint32_t& out_value)
{
	const std::uint8_t cmd = address & 0x1F;
	std::uint8_t rx[3] = {0, 0, 0};
	int rc = spi_xfer(&cmd, 1, rx, sizeof(rx));
	if (rc < 0) {
		return rc;
	}
	out_value = (static_cast<std::uint32_t>(rx[0]) << 16) |
	            (static_cast<std::uint32_t>(rx[1]) << 8) |
	             static_cast<std::uint32_t>(rx[2]);
	return 0;
}

int Tdc7200::load_config(const std::uint8_t* regs, std::size_t len)
{
	for (std::size_t i = 0; i < len; i++) {
		int rc = write_register(static_cast<std::uint8_t>(i), regs[i]);
		if (rc < 0) {
			LOG_WRN("load_config: write reg %u failed (%d)",
				static_cast<unsigned>(i), rc);
			return rc;
		}
	}
	return 0;
}

int Tdc7200::start_measurement()
{
	k_sem_reset(&int_sem);
	return write_register(REG_CONFIG1, CONFIG1_START_MEAS);
}

int Tdc7200::read_time1(std::uint32_t& out_value)
{
	return read_u24(REG_TIME1, out_value);
}

int Tdc7200::wait_for_completion(k_timeout_t timeout)
{
	return k_sem_take(&int_sem, timeout);
}

Tdc7200& tdc7200()
{
	static Tdc7200 instance;
	return instance;
}

} /* namespace uflow::drivers */
