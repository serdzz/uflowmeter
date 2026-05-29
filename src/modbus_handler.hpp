/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Modbus register handler — layer 2 of 3 in the RTU port.
 *
 * Maps incoming Modbus RTU frames to:
 *   - Options struct (Holding registers 0x0000..0x0039 = 116 bytes,
 *     full bitfield including the calibration table)
 *   - Live flow + accumulators (Holding registers 0x0064..0x006B)
 *   - Live flow data (Input registers 0x0000..0x0007)
 *
 * Write paths (0x06 + 0x10) update Options + trigger Options::save
 * through eeprom_power's DP wake/sleep cycle.
 *
 * Source: git show rework/embassy:src/modbus_handler.rs.
 *
 * Concurrency note: this commit's process() expects single-threaded
 * invocation. When layer 3/3 (USART1 transport) lands, the transport
 * thread calls process() — that's the only consumer, so still single-
 * threaded at the Modbus layer. The shared race remains around
 * eeprom_power's `in_dp_state` bool (UI Options::save + Modbus write
 * + history ring writes all touch it without a mutex). Documented in
 * history.cpp; same caveat applies here.
 *
 * Register layout (echoed from embassy modbus_handler.rs:registers):
 *   0x0000..0x0039  Options bytes        (R/W via 0x03/0x06/0x10)
 *   0x0064..0x0065  flow_rate     f32 BE (R via 0x03)
 *   0x0066..0x0067  hour_flow     f32 BE
 *   0x0068..0x0069  day_flow      f32 BE
 *   0x006A..0x006B  month_flow    f32 BE
 *   Input registers 0x0000..0x0007: same f32 layout as 0x0064 (0x04)
 */

#pragma once

#include <cstddef>
#include <cstdint>

#include "modbus.hpp"

struct device;

namespace uflow::modbus_handler {

namespace registers {
constexpr std::uint16_t OPTIONS_START = 0x0000;
constexpr std::uint16_t OPTIONS_END   = 0x0039;
constexpr std::uint16_t FLOW_RATE     = 0x0064;
constexpr std::uint16_t FLOW_END      = 0x006C;  /* exclusive */
}

class Handler {
public:
	Handler();

	/* Process one inbound frame. Refreshes slave_address from
	 * options::g_options at the top of each call so address
	 * changes via either UI or Modbus take effect immediately.
	 *
	 *   frame, frame_len  — inbound bytes (caller's responsibility
	 *                       to detect frame boundaries via inter-
	 *                       frame silence in the transport layer)
	 *   eeprom            — device pointer for Options::save on
	 *                       write paths; may be nullptr if you've
	 *                       arranged to ignore writes (e.g. tests)
	 *   scratch           — 1024-byte buffer for Options::save
	 *   response_buf      — outbound frame goes here
	 *   cap               — response_buf capacity (recommend
	 *                       MAX_FRAME = 256)
	 *   out_len           — bytes written to response_buf
	 *
	 * Returns modbus::Error::Ok with *out_len > 0 → send response.
	 * Returns Error::InvalidSlaveAddr with *out_len = 0 → silent
	 * drop (frame was for another device).
	 * Other errors: *out_len = 0 → don't reply; caller should log. */
	modbus::Error process(const std::uint8_t* frame, std::size_t frame_len,
	                      const struct device* eeprom,
	                      std::uint8_t* scratch,
	                      std::uint8_t* response_buf, std::size_t cap,
	                      std::size_t* out_len);

private:
	modbus::Codec codec_;

	modbus::Error handle_read_holding(const modbus::Request& req,
	                                  std::uint8_t* out, std::size_t cap,
	                                  std::size_t* out_len);
	modbus::Error handle_read_input(const modbus::Request& req,
	                                std::uint8_t* out, std::size_t cap,
	                                std::size_t* out_len);
	modbus::Error handle_write_single(const modbus::Request& req,
	                                  const struct device* eeprom,
	                                  std::uint8_t* scratch,
	                                  std::uint8_t* out, std::size_t cap,
	                                  std::size_t* out_len);
	modbus::Error handle_write_multiple(const modbus::Request& req,
	                                    const struct device* eeprom,
	                                    std::uint8_t* scratch,
	                                    std::uint8_t* out, std::size_t cap,
	                                    std::size_t* out_len);

	/* Helper: build a CRCed exception frame in `out`. */
	modbus::Error emit_exception(std::uint8_t slave, std::uint8_t fc,
	                             modbus::ExceptionCode ex,
	                             std::uint8_t* out, std::size_t cap,
	                             std::size_t* out_len);
};

/* Process-wide singleton — paired with the transport thread in 3/3. */
Handler& instance();

} /* namespace uflow::modbus_handler */
