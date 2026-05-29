/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Modbus codec tests. Verified test vectors from
 * git show rework/embassy:src/modbus.rs#tests where possible.
 */

#include "framework.hpp"

#include "../src/modbus.hpp"

#include <cstring>

using uflow::modbus::Codec;
using uflow::modbus::Error;
using uflow::modbus::ExceptionCode;
using uflow::modbus::FunctionCode;
using uflow::modbus::Request;
using uflow::modbus::Response;
using uflow::modbus::crc16_modbus;

TEST(crc_canonical_vector)
{
	/* Embassy test_crc_calculation: input → CRC 0xCDC5. */
	const std::uint8_t data[] = {0x01, 0x03, 0x00, 0x00, 0x00, 0x0A};
	const auto crc = crc16_modbus(data, sizeof(data));
	ASSERT_EQ(crc, 0xCDC5u);
}

TEST(crc_empty_input)
{
	const auto crc = crc16_modbus(nullptr, 0);
	ASSERT_EQ(crc, 0xFFFFu);  /* init value, no bytes processed */
}

TEST(crc_single_byte)
{
	const std::uint8_t one = 0x01;
	const auto crc = crc16_modbus(&one, 1);
	ASSERT_EQ(crc, 0x807Eu);  /* well-known Modbus CRC for {0x01} */
}

TEST(parse_read_holding_for_us)
{
	Codec codec(0x01);
	/* Read 10 holding regs at 0x0000, CRC = 0xCDC5 (LE → C5 CD). */
	const std::uint8_t frame[] = {0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC5, 0xCD};
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::Ok);
	ASSERT_EQ(req.slave_address, 0x01u);
	ASSERT_TRUE(req.function_code == FunctionCode::ReadHoldingRegisters);
	ASSERT_EQ(req.start_address, 0x0000u);
	ASSERT_EQ(req.quantity, 10u);
}

TEST(parse_for_other_slave_is_dropped)
{
	Codec codec(0x01);  /* we are slave 1 */
	/* Frame for slave 0x05. */
	std::uint8_t frame[] = {0x05, 0x03, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00};
	const std::uint16_t crc = crc16_modbus(frame, 6);
	frame[6] = static_cast<std::uint8_t>(crc & 0xFFu);
	frame[7] = static_cast<std::uint8_t>(crc >> 8);
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::InvalidSlaveAddr);
}

TEST(parse_broadcast_is_accepted)
{
	Codec codec(0x01);
	std::uint8_t frame[] = {0x00, 0x06, 0x00, 0x03, 0xAB, 0xCD, 0x00, 0x00};
	const std::uint16_t crc = crc16_modbus(frame, 6);
	frame[6] = static_cast<std::uint8_t>(crc & 0xFFu);
	frame[7] = static_cast<std::uint8_t>(crc >> 8);
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::Ok);
	ASSERT_EQ(req.slave_address, 0x00u);
	ASSERT_TRUE(req.function_code == FunctionCode::WriteSingleRegister);
	ASSERT_EQ(req.write_data_len, 2u);
	ASSERT_EQ(req.write_data[0], 0xABu);
	ASSERT_EQ(req.write_data[1], 0xCDu);
}

TEST(parse_bad_crc_rejected)
{
	Codec codec(0x01);
	/* Tamper with last byte of the canonical CRC vector. */
	const std::uint8_t frame[] = {0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC5, 0xCE};
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::InvalidCrc);
}

TEST(parse_too_short_rejected)
{
	Codec codec(0x01);
	const std::uint8_t frame[] = {0x01, 0x03};
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::InvalidLength);
}

TEST(parse_unknown_function_returns_error)
{
	Codec codec(0x01);
	std::uint8_t frame[] = {0x01, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
	const std::uint16_t crc = crc16_modbus(frame, 6);
	frame[6] = static_cast<std::uint8_t>(crc & 0xFFu);
	frame[7] = static_cast<std::uint8_t>(crc >> 8);
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::UnknownFunction);
}

TEST(parse_write_multiple)
{
	Codec codec(0x01);
	/* Write 2 regs at 0x0010, byte_count=4, payload [DE AD BE EF]. */
	std::uint8_t frame[] = {
		0x01, 0x10,         /* slave, fc */
		0x00, 0x10,         /* start */
		0x00, 0x02,         /* qty */
		0x04,               /* byte count */
		0xDE, 0xAD, 0xBE, 0xEF,  /* payload */
		0x00, 0x00,         /* crc placeholder */
	};
	const std::uint16_t crc = crc16_modbus(frame, sizeof(frame) - 2);
	frame[sizeof(frame) - 2] = static_cast<std::uint8_t>(crc & 0xFFu);
	frame[sizeof(frame) - 1] = static_cast<std::uint8_t>(crc >> 8);
	Request req{};
	ASSERT_EQ(codec.parse_request(frame, sizeof(frame), req), Error::Ok);
	ASSERT_TRUE(req.function_code == FunctionCode::WriteMultipleRegisters);
	ASSERT_EQ(req.start_address, 0x0010u);
	ASSERT_EQ(req.quantity, 2u);
	ASSERT_EQ(req.write_data_len, 4u);
	ASSERT_EQ(req.write_data[0], 0xDEu);
	ASSERT_EQ(req.write_data[3], 0xEFu);
}

TEST(build_response_round_trip)
{
	Codec codec(0x01);
	Response r{};
	r.slave_address = 0x01;
	r.function_code = static_cast<std::uint8_t>(FunctionCode::ReadHoldingRegisters);
	r.data[0] = 0x04;  /* byte count */
	r.data[1] = 0xAA; r.data[2] = 0xBB; r.data[3] = 0xCC; r.data[4] = 0xDD;
	r.data_len = 5;

	std::uint8_t buf[uflow::modbus::MAX_FRAME];
	std::size_t  len = 0;
	ASSERT_EQ(codec.build_response(r, buf, sizeof(buf), &len), Error::Ok);
	ASSERT_EQ(len, 9u);  /* slave + fc + 5 data + 2 crc */
	ASSERT_EQ(buf[0], 0x01u);
	ASSERT_EQ(buf[1], 0x03u);
	ASSERT_EQ(buf[2], 0x04u);
	/* CRC should validate. */
	const std::uint16_t expected = crc16_modbus(buf, len - 2);
	ASSERT_EQ(buf[len - 2], static_cast<std::uint8_t>(expected & 0xFFu));
	ASSERT_EQ(buf[len - 1], static_cast<std::uint8_t>(expected >> 8));
}

TEST(build_exception_frame)
{
	Codec codec(0x01);
	std::uint8_t buf[uflow::modbus::MAX_FRAME];
	std::size_t  len = 0;
	ASSERT_EQ(codec.build_exception(0x01,
		static_cast<std::uint8_t>(FunctionCode::ReadHoldingRegisters),
		ExceptionCode::IllegalDataAddress,
		buf, sizeof(buf), &len), Error::Ok);
	ASSERT_EQ(len, 5u);
	ASSERT_EQ(buf[0], 0x01u);
	ASSERT_EQ(buf[1], 0x83u);  /* function | 0x80 */
	ASSERT_EQ(buf[2], 0x02u);  /* IllegalDataAddress */
}

TEST(build_response_too_small_buffer)
{
	Codec codec(0x01);
	Response r{};
	r.slave_address = 0x01;
	r.function_code = 0x03;
	r.data_len = 100;  /* needs 104 bytes total */

	std::uint8_t buf[50];
	std::size_t  len = 0;
	ASSERT_EQ(codec.build_response(r, buf, sizeof(buf), &len), Error::BufferTooSmall);
}
