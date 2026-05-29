/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Modbus RTU codec — see modbus.hpp for the contract.
 *
 * Implementation strategy: completely allocation-free. Caller supplies
 * the destination buffer for build_response / build_exception; we
 * write bytes + return the length. Request parsing fills a Request
 * struct in-place — no copies of the write_data payload beyond the
 * one memcpy from the input frame.
 */

#include "modbus.hpp"

#include <cstring>

namespace uflow::modbus {

namespace {

constexpr std::size_t MIN_FRAME = 8;   /* slave + function + 4 data + 2 CRC */

bool is_known_function(std::uint8_t fc)
{
	switch (fc) {
	case 0x01: case 0x02: case 0x03: case 0x04:
	case 0x05: case 0x06: case 0x0F: case 0x10: case 0x17:
		return true;
	default:
		return false;
	}
}

inline std::uint16_t be16(const std::uint8_t* p)
{
	return static_cast<std::uint16_t>((static_cast<std::uint16_t>(p[0]) << 8) |
	                                   static_cast<std::uint16_t>(p[1]));
}

} /* namespace */

std::uint16_t crc16_modbus(const std::uint8_t* data, std::size_t len)
{
	std::uint16_t crc = 0xFFFFu;
	for (std::size_t i = 0; i < len; i++) {
		crc ^= static_cast<std::uint16_t>(data[i]);
		for (int b = 0; b < 8; b++) {
			if (crc & 0x0001u) {
				crc = static_cast<std::uint16_t>((crc >> 1) ^ 0xA001u);
			} else {
				crc = static_cast<std::uint16_t>(crc >> 1);
			}
		}
	}
	return crc;
}

Error Codec::parse_request(const std::uint8_t* in, std::size_t len, Request& out) const
{
	if (len < MIN_FRAME) {
		return Error::InvalidLength;
	}

	const std::uint8_t slave = in[0];
	/* Slave 0 = broadcast (accept); otherwise must match ours. */
	if (slave != 0 && slave != slave_address_) {
		return Error::InvalidSlaveAddr;
	}

	/* CRC is LE on the wire — low byte first. */
	const std::uint16_t received_crc = static_cast<std::uint16_t>(
		static_cast<std::uint16_t>(in[len - 2]) |
		(static_cast<std::uint16_t>(in[len - 1]) << 8));
	const std::uint16_t computed_crc = crc16_modbus(in, len - 2);
	if (received_crc != computed_crc) {
		return Error::InvalidCrc;
	}

	const std::uint8_t fc_raw = in[1];
	if (!is_known_function(fc_raw)) {
		out.slave_address  = slave;
		out.function_code  = static_cast<FunctionCode>(fc_raw);
		out.start_address  = 0;
		out.quantity       = 0;
		out.write_data_len = 0;
		return Error::UnknownFunction;
	}
	const FunctionCode fc = static_cast<FunctionCode>(fc_raw);

	out.slave_address = slave;
	out.function_code = fc;
	out.start_address = 0;
	out.quantity      = 0;
	out.write_data_len = 0;

	switch (fc) {
	case FunctionCode::ReadCoils:
	case FunctionCode::ReadDiscreteInputs:
	case FunctionCode::ReadHoldingRegisters:
	case FunctionCode::ReadInputRegisters:
		if (len < MIN_FRAME) {
			return Error::InvalidLength;
		}
		out.start_address = be16(&in[2]);
		out.quantity      = be16(&in[4]);
		return Error::Ok;

	case FunctionCode::WriteSingleCoil:
	case FunctionCode::WriteSingleRegister:
		if (len < MIN_FRAME) {
			return Error::InvalidLength;
		}
		out.start_address = be16(&in[2]);
		out.quantity      = 1;
		out.write_data[0] = in[4];
		out.write_data[1] = in[5];
		out.write_data_len = 2;
		return Error::Ok;

	case FunctionCode::WriteMultipleCoils:
	case FunctionCode::WriteMultipleRegisters: {
		/* Header is 7 bytes: slave + fn + addr(2) + qty(2) + byte_count(1). */
		if (len < 7 + 2) {
			return Error::InvalidLength;
		}
		out.start_address = be16(&in[2]);
		out.quantity      = be16(&in[4]);
		const std::size_t byte_count = in[6];
		if (len < 7 + byte_count + 2) {
			return Error::InvalidLength;
		}
		if (byte_count > MAX_FRAME) {
			return Error::BufferTooSmall;
		}
		std::memcpy(out.write_data, &in[7], byte_count);
		out.write_data_len = byte_count;
		return Error::Ok;
	}

	case FunctionCode::ReadWriteMultipleRegs:
		/* Not used by this device's register map. Caller may still
		 * want to send an IllegalFunction exception. */
		return Error::UnknownFunction;
	}

	return Error::UnknownFunction;
}

Error Codec::build_response(const Response& r,
                            std::uint8_t* out_buf, std::size_t cap,
                            std::size_t* out_len) const
{
	const std::size_t needed = 1 /*slave*/ + 1 /*fc*/ + r.data_len + 2 /*crc*/;
	if (cap < needed) {
		return Error::BufferTooSmall;
	}
	out_buf[0] = r.slave_address;
	out_buf[1] = r.function_code;
	if (r.data_len > 0) {
		std::memcpy(&out_buf[2], r.data, r.data_len);
	}
	const std::uint16_t crc = crc16_modbus(out_buf, 2 + r.data_len);
	out_buf[2 + r.data_len + 0] = static_cast<std::uint8_t>(crc & 0xFFu);
	out_buf[2 + r.data_len + 1] = static_cast<std::uint8_t>((crc >> 8) & 0xFFu);
	*out_len = needed;
	return Error::Ok;
}

Error Codec::build_exception(std::uint8_t slave, std::uint8_t function,
                             ExceptionCode exception,
                             std::uint8_t* out_buf, std::size_t cap,
                             std::size_t* out_len) const
{
	constexpr std::size_t needed = 5;  /* slave + fn|0x80 + exception + 2 CRC */
	if (cap < needed) {
		return Error::BufferTooSmall;
	}
	out_buf[0] = slave;
	out_buf[1] = static_cast<std::uint8_t>(function | 0x80u);
	out_buf[2] = static_cast<std::uint8_t>(exception);
	const std::uint16_t crc = crc16_modbus(out_buf, 3);
	out_buf[3] = static_cast<std::uint8_t>(crc & 0xFFu);
	out_buf[4] = static_cast<std::uint8_t>((crc >> 8) & 0xFFu);
	*out_len = needed;
	return Error::Ok;
}

} /* namespace uflow::modbus */
