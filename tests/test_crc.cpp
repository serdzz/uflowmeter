/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * CRC16-CCITT-FALSE tests. Algorithm parameters (poly 0x1021, init
 * 0xFFFF, no I/O reflection, no XOR-out) match what the Options
 * dual-page layout AND the history rings' ServiceData use.
 *
 * Reference vectors: standard CRC catalogue
 * (https://reveng.sourceforge.io/crc-catalogue/16.htm — CRC-16/IBM-3740).
 */

#include "framework.hpp"

#include "../src/crc.hpp"

#include <cstring>

using uflow::crc::crc16_ccitt_false;

TEST(crc_canonical_check)
{
	/* The standard "check" vector — ASCII "123456789" → 0x29B1. */
	const char s[] = "123456789";
	const auto c = crc16_ccitt_false(
		reinterpret_cast<const std::uint8_t*>(s), 9);
	ASSERT_EQ(c, 0x29B1u);
}

TEST(crc_empty_input)
{
	const auto c = crc16_ccitt_false(nullptr, 0);
	ASSERT_EQ(c, 0xFFFFu);  /* init value, no bytes processed */
}

TEST(crc_single_zero_byte)
{
	const std::uint8_t z = 0;
	const auto c = crc16_ccitt_false(&z, 1);
	/* CRC of a single 0x00 byte under CRC-16/IBM-3740 — independently
	 * cross-checked against pycrc / reveng. */
	ASSERT_EQ(c, 0xE1F0u);
}

TEST(crc_one_byte_repeatable)
{
	const std::uint8_t b = 0xAB;
	const auto c1 = crc16_ccitt_false(&b, 1);
	const auto c2 = crc16_ccitt_false(&b, 1);
	/* Same input → same output (proves no global state leaks). */
	ASSERT_EQ(c1, c2);
}

TEST(crc_concatenation_changes_result)
{
	const std::uint8_t a[] = {0x01, 0x02, 0x03};
	const std::uint8_t b[] = {0x01, 0x02, 0x03, 0x04};
	const auto ca = crc16_ccitt_false(a, sizeof(a));
	const auto cb = crc16_ccitt_false(b, sizeof(b));
	ASSERT_NE(ca, cb);
}

TEST(crc_no_aliasing_with_modbus_crc)
{
	/* Sanity vs the Modbus CRC test vector — same input bytes
	 * should produce DIFFERENT CRCs because the polynomials differ.
	 * Modbus vector from test_modbus.cpp::crc_canonical_vector. */
	const std::uint8_t data[] = {0x01, 0x03, 0x00, 0x00, 0x00, 0x0A};
	const auto ccitt = crc16_ccitt_false(data, sizeof(data));
	/* Modbus CRC of the same bytes is 0xCDC5 (verified in
	 * test_modbus.cpp). CCITT-FALSE must be different. */
	ASSERT_NE(ccitt, 0xCDC5u);
}
