/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "crc.hpp"

namespace uflow::crc {

std::uint16_t crc16_ccitt_false(const std::uint8_t* data, std::size_t len)
{
	std::uint16_t crc = 0xFFFF;
	for (std::size_t i = 0; i < len; i++) {
		crc ^= static_cast<std::uint16_t>(data[i]) << 8;
		for (int j = 0; j < 8; j++) {
			crc = (crc & 0x8000)
				? static_cast<std::uint16_t>((crc << 1) ^ 0x1021)
				: static_cast<std::uint16_t>(crc << 1);
		}
	}
	return crc;
}

} /* namespace uflow::crc */
