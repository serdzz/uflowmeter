/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Implementation of the Options EEPROM I/O + CRC. See options.hpp for
 * the byte-layout commitment and the dual-page protocol.
 */

#include "options.hpp"

#include <cstring>

#include <zephyr/device.h>
#include <zephyr/drivers/eeprom.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(options, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::options {

std::uint16_t crc16_ccitt_false(const std::uint8_t* data, std::size_t len)
{
	std::uint16_t crc = 0xFFFF;
	for (std::size_t i = 0; i < len; i++) {
		crc ^= static_cast<std::uint16_t>(data[i]) << 8;
		for (int j = 0; j < 8; j++) {
			crc = (crc & 0x8000) ? static_cast<std::uint16_t>((crc << 1) ^ 0x1021)
			                     : static_cast<std::uint16_t>(crc << 1);
		}
	}
	return crc;
}

namespace {

/* Read one 1024-byte page and validate the CRC. On success, copy the
 * first sizeof(Options) bytes into `out` and return true. */
bool try_load_copy(const struct device* eeprom,
                   std::uint32_t        offset,
                   Options&             out,
                   std::uint8_t*        scratch)
{
	int rc = eeprom_read(eeprom, offset, scratch, OPTIONS_PAGE_SIZE);
	if (rc < 0) {
		LOG_ERR("eeprom_read(@%u) failed: %d", offset, rc);
		return false;
	}

	const std::uint16_t computed = crc16_ccitt_false(scratch + 2,
		OPTIONS_PAGE_SIZE - 2);

	/* The on-disk CRC sits in the first two bytes of the page. We copy
	 * the Options-sized prefix into `out` and compare against its
	 * `crc` field. */
	std::memcpy(&out, scratch, sizeof(Options));
	if (out.crc != computed) {
		LOG_WRN("CRC mismatch @%u: stored=0x%04x computed=0x%04x",
			offset, out.crc, computed);
		return false;
	}
	return true;
}

} /* namespace */

LoadResult load(const struct device* eeprom, Options& out, std::uint8_t* scratch)
{
	if (!device_is_ready(eeprom)) {
		LOG_ERR("eeprom device not ready");
		return LoadResult::IoError;
	}
	if (try_load_copy(eeprom, OPTIONS_OFFSET_PRIMARY, out, scratch)) {
		return LoadResult::OkPrimary;
	}
	if (try_load_copy(eeprom, OPTIONS_OFFSET_SECONDARY, out, scratch)) {
		LOG_WRN("primary copy bad, secondary OK");
		return LoadResult::OkSecondary;
	}
	LOG_ERR("both Options copies corrupt");
	return LoadResult::BothCorrupt;
}

int save(const struct device* eeprom, Options& opts, std::uint8_t* scratch)
{
	if (!device_is_ready(eeprom)) {
		return -ENODEV;
	}

	/* Build the full 1024-byte image: zero the buffer, copy Options
	 * prefix in, compute CRC over bytes 2..1024, patch the CRC field,
	 * recopy. */
	std::memset(scratch, 0, OPTIONS_PAGE_SIZE);
	std::memcpy(scratch, &opts, sizeof(Options));

	const std::uint16_t crc = crc16_ccitt_false(scratch + 2,
		OPTIONS_PAGE_SIZE - 2);
	opts.crc = crc;
	std::memcpy(scratch, &opts, sizeof(Options));

	int rc = eeprom_write(eeprom, OPTIONS_OFFSET_PRIMARY, scratch, OPTIONS_PAGE_SIZE);
	if (rc < 0) {
		LOG_ERR("eeprom_write primary failed: %d", rc);
		return rc;
	}
	rc = eeprom_write(eeprom, OPTIONS_OFFSET_SECONDARY, scratch, OPTIONS_PAGE_SIZE);
	if (rc < 0) {
		LOG_ERR("eeprom_write secondary failed: %d", rc);
		return rc;
	}
	return 0;
}

} /* namespace uflow::options */
