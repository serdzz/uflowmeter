/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Deep-power-down control for the 25LC1024. See eeprom_power.hpp for
 * the contract and rationale.
 *
 * Implementation note: this module owns its own spi_dt_spec pointing
 * at the same eeprom@0 DT node that the at25 driver consumes. Zephyr's
 * SPI driver serializes access via its bus mutex (acquired internally
 * by spi_write_dt / spi_transceive_dt), so concurrent at25 traffic
 * doesn't interleave with our DP/RDP commands at the wire level. Our
 * spec inherits cs-gpios[0] (PC10), spi-max-frequency (1 MHz), and SPI
 * mode 0 from the parent &spi2 binding — identical config to at25, so
 * the controller doesn't need to be reconfigured between callers.
 */

#include "eeprom_power.hpp"

#include <cstdint>

#include <zephyr/device.h>
#include <zephyr/drivers/spi.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(eeprom_power, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers {

namespace {

#define EEPROM_NODE DT_CHOSEN(uflowmeter_eeprom)

static_assert(DT_NODE_EXISTS(EEPROM_NODE),
	"DT chosen `uflowmeter,eeprom` is missing — check your board DTS");

constexpr std::uint8_t CMD_DP  = 0xB9;
constexpr std::uint8_t CMD_RDP = 0xAB;

/* tRDP per datasheet Table 1-3 ("RDP to Read"): max 100 µs at 3 V.
 * We round up; the busy-wait happens once per wake. */
constexpr std::uint32_t T_RDP_US = 100;

/* SPI mode 0, MSB-first, 8-bit words — matches what the at25 driver
 * configures the controller for, so no SPI reconfig overhead between
 * our calls and at25's. */
const struct spi_dt_spec eeprom_spi = SPI_DT_SPEC_GET(
	EEPROM_NODE,
	SPI_OP_MODE_MASTER | SPI_TRANSFER_MSB | SPI_WORD_SET(8),
	0);

/* State — single-threaded usage assumed (see header). */
bool in_dp_state = false;

int send_command_byte(std::uint8_t cmd)
{
	struct spi_buf tx_buf = { .buf = &cmd, .len = 1 };
	struct spi_buf_set tx_set = { .buffers = &tx_buf, .count = 1 };
	return spi_write_dt(&eeprom_spi, &tx_set);
}

} /* namespace */

int eeprom_enter_deep_power_down()
{
	if (in_dp_state) {
		return 0;
	}
	int rc = send_command_byte(CMD_DP);
	if (rc < 0) {
		LOG_ERR("DP command failed: %d", rc);
		return rc;
	}
	in_dp_state = true;
	LOG_DBG("eeprom -> deep power-down");
	return 0;
}

int eeprom_exit_deep_power_down()
{
	if (!in_dp_state) {
		return 0;
	}
	int rc = send_command_byte(CMD_RDP);
	if (rc < 0) {
		LOG_ERR("RDP command failed: %d", rc);
		return rc;
	}
	k_busy_wait(T_RDP_US);
	in_dp_state = false;
	LOG_DBG("eeprom <- deep power-down (awake after %u us)", T_RDP_US);
	return 0;
}

bool eeprom_is_powered_down()
{
	return in_dp_state;
}

} /* namespace uflow::drivers */
