/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Modbus RTU codec — frame parse + build + CRC. Pure logic, no
 * hardware dependencies. Layer 1 of 3:
 *
 *   1. Codec (this file)            — parse_request / build_response
 *   2. Register handler             — maps frames to Options / history /
 *                                     measurement reads + writes
 *   3. UART transport (uart.cpp)    — on-demand USART1 session
 *
 * Source: git show rework/embassy:src/modbus.rs.
 *
 * Function codes supported:
 *   0x03  Read Holding Registers  — Options-backed config
 *   0x04  Read Input Registers    — live flow / accumulators
 *   0x06  Write Single Register   — single Options field
 *   0x10  Write Multiple Registers — batch Options write
 *
 * Other function codes return IllegalFunction.
 *
 * Slave address 0 is broadcast — frames addressed there are accepted
 * but produce no response. Otherwise the address byte must match the
 * configured slave or the frame is dropped silently.
 *
 * CRC: CRC16-Modbus (poly 0xA001 reflected, init 0xFFFF, no I/O
 * reflection in code, LE byte order on the wire). DIFFERENT from the
 * CRC16-CCITT-FALSE used for Options + history rings — keep clear.
 *
 * Frame buffer cap: 256 bytes — matches embassy and the practical
 * Modbus max-payload (252 bytes payload + slave + function + CRC).
 */

#pragma once

#include <cstddef>
#include <cstdint>

namespace uflow::modbus {

constexpr std::size_t MAX_FRAME = 256;

enum class FunctionCode : std::uint8_t {
	ReadCoils                = 0x01,
	ReadDiscreteInputs       = 0x02,
	ReadHoldingRegisters     = 0x03,
	ReadInputRegisters       = 0x04,
	WriteSingleCoil          = 0x05,
	WriteSingleRegister      = 0x06,
	WriteMultipleCoils       = 0x0F,
	WriteMultipleRegisters   = 0x10,
	ReadWriteMultipleRegs    = 0x17,
};

enum class ExceptionCode : std::uint8_t {
	IllegalFunction        = 0x01,
	IllegalDataAddress     = 0x02,
	IllegalDataValue       = 0x03,
	ServerDeviceFailure    = 0x04,
};

enum class Error : std::int8_t {
	Ok                = 0,
	InvalidCrc        = -1,
	InvalidLength     = -2,
	InvalidSlaveAddr  = -3,   /* frame for a different slave — drop silently */
	BufferTooSmall    = -4,
	UnknownFunction   = -5,   /* exception 0x01 needed in response */
};

struct Request {
	std::uint8_t   slave_address;
	FunctionCode   function_code;
	std::uint16_t  start_address;
	std::uint16_t  quantity;
	/* Payload for write functions (single byte pair for 0x06,
	 * byte_count-N bytes for 0x10). For read functions, len = 0. */
	std::uint8_t   write_data[MAX_FRAME];
	std::size_t    write_data_len;
};

struct Response {
	std::uint8_t  slave_address;
	std::uint8_t  function_code;
	std::uint8_t  data[MAX_FRAME];
	std::size_t   data_len;
};

/* CRC16-Modbus over `len` bytes. Pure function, exposed for tests. */
std::uint16_t crc16_modbus(const std::uint8_t* data, std::size_t len);

class Codec {
public:
	explicit Codec(std::uint8_t slave_address) : slave_address_{slave_address} {}

	std::uint8_t slave_address() const { return slave_address_; }
	void set_slave_address(std::uint8_t a) { slave_address_ = a; }

	/* Parse a complete RTU frame from `in` (len bytes). Returns Ok
	 * on success, or one of the Error values. Sets `out` on success.
	 * UnknownFunction is returned for function bytes outside the
	 * FunctionCode enum — caller should build an exception response. */
	Error parse_request(const std::uint8_t* in, std::size_t len, Request& out) const;

	/* Build a successful response frame into `out_buf` (cap bytes).
	 * On success writes `*out_len` bytes (≤ cap) and returns Ok.
	 * BufferTooSmall when the response wouldn't fit. */
	Error build_response(const Response& r,
	                     std::uint8_t* out_buf, std::size_t cap,
	                     std::size_t* out_len) const;

	/* Build an exception frame (function | 0x80, exception byte, CRC).
	 * 5 bytes total — never exceeds cap if cap ≥ 5. */
	Error build_exception(std::uint8_t slave, std::uint8_t function,
	                      ExceptionCode exception,
	                      std::uint8_t* out_buf, std::size_t cap,
	                      std::size_t* out_len) const;

private:
	std::uint8_t slave_address_;
};

} /* namespace uflow::modbus */
