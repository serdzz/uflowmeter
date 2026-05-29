/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * CRC primitives — pure logic, no Zephyr or hardware dependencies,
 * so the test build can compile + exercise them on the host.
 *
 * Used by:
 *   - src/options.cpp        — dual-page Options layout CRC
 *   - src/history_lib.hpp    — RingStorage ServiceData CRC
 *
 * Modbus uses a DIFFERENT polynomial (0xA001 reflected, init 0xFFFF,
 * LE byte order on wire) — see src/modbus.cpp::crc16_modbus. Two
 * CRCs with similar-looking names; keep clear.
 *
 * CRC16-CCITT-FALSE:
 *   polynomial = 0x1021
 *   init       = 0xFFFF
 *   refIn      = false
 *   refOut     = false
 *   xorOut     = 0x0000
 *   check      = 0x29B1 for "123456789"
 */

#pragma once

#include <cstddef>
#include <cstdint>

namespace uflow::crc {

std::uint16_t crc16_ccitt_false(const std::uint8_t* data, std::size_t len);

} /* namespace uflow::crc */
