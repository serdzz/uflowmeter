/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Modbus register handler implementation. See modbus_handler.hpp for
 * the contract + register map.
 *
 * Style notes:
 *   - All response composition writes into a scratch payload[] then
 *     hands the Response struct to Codec::build_response which adds
 *     framing + CRC. This keeps the codec layer ignorant of register
 *     semantics.
 *   - Floats are big-endian on the wire (Modbus convention). We pun
 *     via memcpy → byte swap; no aliasing UB.
 *   - Write paths reach into Options + eeprom_power. The eeprom DP
 *     wake/sleep cycle is identical to what main.cpp does in
 *     handle_app_request when a UI edit commits.
 */

#include "modbus_handler.hpp"

#include <cstring>

#include <zephyr/device.h>
#include <zephyr/logging/log.h>

#include "drivers/eeprom_power.hpp"
#include "history.hpp"
#include "measurement.hpp"
#include "options.hpp"

LOG_MODULE_REGISTER(modbus_handler, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::modbus_handler {

namespace {

/* Convert f32 → 4 big-endian bytes. Modbus register order: high u16
 * first, then low u16. Each u16 is also big-endian on the wire. */
void f32_to_be(float v, std::uint8_t out[4])
{
	std::uint32_t bits;
	std::memcpy(&bits, &v, 4);
	out[0] = static_cast<std::uint8_t>((bits >> 24) & 0xFFu);
	out[1] = static_cast<std::uint8_t>((bits >> 16) & 0xFFu);
	out[2] = static_cast<std::uint8_t>((bits >>  8) & 0xFFu);
	out[3] = static_cast<std::uint8_t>((bits      ) & 0xFFu);
}

} /* namespace */

Handler::Handler() : codec_{options::g_options.slave_address} {}

modbus::Error Handler::emit_exception(std::uint8_t slave, std::uint8_t fc,
                                      modbus::ExceptionCode ex,
                                      std::uint8_t* out, std::size_t cap,
                                      std::size_t* out_len)
{
	return codec_.build_exception(slave, fc, ex, out, cap, out_len);
}

modbus::Error Handler::process(const std::uint8_t* frame, std::size_t frame_len,
                               const struct device* eeprom,
                               std::uint8_t* scratch,
                               std::uint8_t* response_buf, std::size_t cap,
                               std::size_t* out_len)
{
	*out_len = 0;
	/* Refresh slave_address — UI edits or prior Modbus writes may
	 * have changed it. */
	codec_.set_slave_address(options::g_options.slave_address);

	modbus::Request req{};
	modbus::Error err = codec_.parse_request(frame, frame_len, req);

	if (err == modbus::Error::InvalidSlaveAddr) {
		return err;  /* silent drop */
	}
	if (err == modbus::Error::InvalidCrc ||
	    err == modbus::Error::InvalidLength ||
	    err == modbus::Error::BufferTooSmall) {
		/* Don't reply to corrupt frames — peer will time out and
		 * retry. */
		return err;
	}
	if (err == modbus::Error::UnknownFunction) {
		const std::uint8_t fc_raw = static_cast<std::uint8_t>(req.function_code);
		return emit_exception(req.slave_address, fc_raw,
		                      modbus::ExceptionCode::IllegalFunction,
		                      response_buf, cap, out_len);
	}

	switch (req.function_code) {
	case modbus::FunctionCode::ReadHoldingRegisters:
		return handle_read_holding(req, response_buf, cap, out_len);
	case modbus::FunctionCode::ReadInputRegisters:
		return handle_read_input(req, response_buf, cap, out_len);
	case modbus::FunctionCode::WriteSingleRegister:
		return handle_write_single(req, eeprom, scratch,
		                           response_buf, cap, out_len);
	case modbus::FunctionCode::WriteMultipleRegisters:
		return handle_write_multiple(req, eeprom, scratch,
		                             response_buf, cap, out_len);
	default:
		return emit_exception(req.slave_address,
		                      static_cast<std::uint8_t>(req.function_code),
		                      modbus::ExceptionCode::IllegalFunction,
		                      response_buf, cap, out_len);
	}
}

modbus::Error Handler::handle_read_holding(const modbus::Request& req,
                                           std::uint8_t* out, std::size_t cap,
                                           std::size_t* out_len)
{
	const std::uint16_t start = req.start_address;
	const std::uint16_t quantity = req.quantity;

	if (quantity == 0 || quantity > 125) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	modbus::Response resp{};
	resp.slave_address = req.slave_address;
	resp.function_code = static_cast<std::uint8_t>(req.function_code);

	const std::uint8_t byte_count = static_cast<std::uint8_t>(quantity * 2);
	resp.data[0] = byte_count;
	resp.data_len = 1u + byte_count;
	if (resp.data_len > modbus::MAX_FRAME) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	if (start <= registers::OPTIONS_END) {
		/* Options bytes — direct memcpy from the packed struct. */
		const auto& opts = options::g_options;
		const std::uint8_t* opts_bytes = reinterpret_cast<const std::uint8_t*>(&opts);
		const std::size_t opts_size = sizeof(options::Options);
		const std::size_t start_byte = static_cast<std::size_t>(
			(start - registers::OPTIONS_START) * 2);
		const std::size_t end_byte = start_byte + byte_count;
		if (end_byte > opts_size) {
			return emit_exception(req.slave_address,
				static_cast<std::uint8_t>(req.function_code),
				modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
		}
		std::memcpy(&resp.data[1], opts_bytes + start_byte, byte_count);
	} else if (start >= registers::FLOW_RATE && start < registers::FLOW_END) {
		/* 4 floats × 2 regs = 8 regs covering flow_rate / hour /
		 * day / month. */
		float values[4] = {
			measurement::latest_flow_m3h.load(std::memory_order_relaxed),
			history::hour_accumulator(),
			history::day_accumulator(),
			history::month_accumulator(),
		};
		std::uint8_t bytes[16];  /* 4 floats × 4 bytes */
		for (int i = 0; i < 4; i++) {
			f32_to_be(values[i], &bytes[i * 4]);
		}
		const std::size_t start_byte = static_cast<std::size_t>(
			(start - registers::FLOW_RATE) * 2);
		const std::size_t end_byte = start_byte + byte_count;
		if (end_byte > sizeof(bytes)) {
			return emit_exception(req.slave_address,
				static_cast<std::uint8_t>(req.function_code),
				modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
		}
		std::memcpy(&resp.data[1], &bytes[start_byte], byte_count);
	} else {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}

	return codec_.build_response(resp, out, cap, out_len);
}

modbus::Error Handler::handle_read_input(const modbus::Request& req,
                                         std::uint8_t* out, std::size_t cap,
                                         std::size_t* out_len)
{
	const std::uint16_t start = req.start_address;
	const std::uint16_t quantity = req.quantity;

	if (quantity == 0 || quantity > 125) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	if (start >= 8) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}

	modbus::Response resp{};
	resp.slave_address = req.slave_address;
	resp.function_code = static_cast<std::uint8_t>(req.function_code);
	const std::uint8_t byte_count = static_cast<std::uint8_t>(quantity * 2);
	resp.data[0] = byte_count;
	resp.data_len = 1u + byte_count;
	if (resp.data_len > modbus::MAX_FRAME) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	float values[4] = {
		measurement::latest_flow_m3h.load(std::memory_order_relaxed),
		history::hour_accumulator(),
		history::day_accumulator(),
		history::month_accumulator(),
	};
	std::uint8_t bytes[16];
	for (int i = 0; i < 4; i++) {
		f32_to_be(values[i], &bytes[i * 4]);
	}
	const std::size_t start_byte = static_cast<std::size_t>(start * 2);
	const std::size_t end_byte = start_byte + byte_count;
	if (end_byte > sizeof(bytes)) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}
	std::memcpy(&resp.data[1], &bytes[start_byte], byte_count);

	return codec_.build_response(resp, out, cap, out_len);
}

modbus::Error Handler::handle_write_single(const modbus::Request& req,
                                           const struct device* eeprom,
                                           std::uint8_t* scratch,
                                           std::uint8_t* out, std::size_t cap,
                                           std::size_t* out_len)
{
	const std::uint16_t address = req.start_address;
	if (address > registers::OPTIONS_END) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}
	if (req.write_data_len != 2) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	/* Patch the Options struct bytes in place. */
	std::uint8_t* opts_bytes = reinterpret_cast<std::uint8_t*>(&options::g_options);
	const std::size_t byte_offset = static_cast<std::size_t>(
		(address - registers::OPTIONS_START) * 2);
	if (byte_offset + 2 > sizeof(options::Options)) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}
	opts_bytes[byte_offset]     = req.write_data[0];
	opts_bytes[byte_offset + 1] = req.write_data[1];

	int rc = options::save_through_dp(eeprom, scratch);
	if (rc < 0) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::ServerDeviceFailure, out, cap, out_len);
	}
	LOG_INF("modbus write_single: addr=0x%04x value=0x%02x%02x",
		address, req.write_data[0], req.write_data[1]);

	/* Echo back: address + value. */
	modbus::Response resp{};
	resp.slave_address = req.slave_address;
	resp.function_code = static_cast<std::uint8_t>(req.function_code);
	resp.data[0] = static_cast<std::uint8_t>((address >> 8) & 0xFFu);
	resp.data[1] = static_cast<std::uint8_t>(address & 0xFFu);
	resp.data[2] = req.write_data[0];
	resp.data[3] = req.write_data[1];
	resp.data_len = 4;
	return codec_.build_response(resp, out, cap, out_len);
}

modbus::Error Handler::handle_write_multiple(const modbus::Request& req,
                                             const struct device* eeprom,
                                             std::uint8_t* scratch,
                                             std::uint8_t* out, std::size_t cap,
                                             std::size_t* out_len)
{
	const std::uint16_t start = req.start_address;
	const std::uint16_t quantity = req.quantity;
	const std::size_t expected_bytes = static_cast<std::size_t>(quantity) * 2u;

	if (quantity == 0 ||
	    static_cast<std::uint32_t>(start) + quantity - 1u > registers::OPTIONS_END) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}
	if (req.write_data_len != expected_bytes) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataValue, out, cap, out_len);
	}

	std::uint8_t* opts_bytes = reinterpret_cast<std::uint8_t*>(&options::g_options);
	const std::size_t start_byte = static_cast<std::size_t>(
		(start - registers::OPTIONS_START) * 2);
	if (start_byte + expected_bytes > sizeof(options::Options)) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::IllegalDataAddress, out, cap, out_len);
	}
	std::memcpy(opts_bytes + start_byte, req.write_data, expected_bytes);

	int rc = options::save_through_dp(eeprom, scratch);
	if (rc < 0) {
		return emit_exception(req.slave_address,
			static_cast<std::uint8_t>(req.function_code),
			modbus::ExceptionCode::ServerDeviceFailure, out, cap, out_len);
	}
	LOG_INF("modbus write_multiple: start=0x%04x qty=%u",
		start, static_cast<unsigned>(quantity));

	/* Echo back: start + quantity. */
	modbus::Response resp{};
	resp.slave_address = req.slave_address;
	resp.function_code = static_cast<std::uint8_t>(req.function_code);
	resp.data[0] = static_cast<std::uint8_t>((start >> 8) & 0xFFu);
	resp.data[1] = static_cast<std::uint8_t>(start & 0xFFu);
	resp.data[2] = static_cast<std::uint8_t>((quantity >> 8) & 0xFFu);
	resp.data[3] = static_cast<std::uint8_t>(quantity & 0xFFu);
	resp.data_len = 4;
	return codec_.build_response(resp, out, cap, out_len);
}

Handler& instance()
{
	static Handler h;
	return h;
}

} /* namespace uflow::modbus_handler */
