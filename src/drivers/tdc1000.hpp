/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * TDC1000 driver — ultrasonic analog front-end. Generates the TX
 * burst, switches between channel 0 (downstream) and channel 1
 * (upstream), feeds the receive comparator that latches the
 * timestamp into the TDC7200.
 *
 * Pin context (from DT chosen uflowmeter,tdc1000):
 *   SPI    — &spi2, CS=PB11 (cs-gpios[1] of &spi2)
 *   EN     — PB10, active-HIGH (LOW between measurements)
 *   RST    — PC6,  active-LOW  (HIGH for normal operation)
 *
 * Register format (SPI):
 *   write: cmd = (0x80 | (addr & 0x7F)), then data byte
 *   read:  cmd = (0x00 | (addr & 0x7F)), then read data byte back
 *
 * The driver does NOT cache register state — the chip loses all
 * registers when EN drops, so callers re-issue load_config() after
 * every power_on(). See rework/embassy:src/main.rs measurement_task
 * for the reference cycle.
 *
 * Single-threaded usage assumed (the measurement thread is the only
 * caller). Thread-safety can be layered on if a second consumer
 * shows up. SPI access is serialized at the controller level by
 * Zephyr's bus mutex.
 */

#pragma once

#include <cstddef>
#include <cstdint>

namespace uflow::drivers {

class Tdc1000 {
public:
	/* Configure EN and RST as outputs (EN LOW, RST HIGH at boot —
	 * chip stays off until power_on()). Verifies SPI controller +
	 * GPIO ports are ready. Returns 0 on success. */
	int init();

	/* Drive EN HIGH. Caller must k_msleep(1) before any SPI access
	 * — the internal LDO needs time to settle, ~500 µs typ. All
	 * register state was lost at the previous power_off(); call
	 * load_config() before kicking measurements. */
	void power_on();

	/* Drive EN LOW. ~0.5 mA drops out of the system supply. */
	void power_off();

	int read_register(std::uint8_t address, std::uint8_t& out_value);
	int write_register(std::uint8_t address, std::uint8_t value);

	/* Bulk-load a register blob, one byte per register starting at
	 * 0x00. The Options struct stores this as a fixed 10-byte slot
	 * (tdc1000_regs[10]). */
	int load_config(const std::uint8_t* regs, std::size_t len);

	/* Toggle CONFIG_2.CH2 (reg 0x02 bit 0). false = downstream, true
	 * = upstream. */
	int set_channel(bool ch2);

	/* Write 0xFF to ERROR_FLAGS (reg 0x07) to clear all latched
	 * comparator/sampling errors before a new measurement. */
	int clear_error_flags();
};

/* Singleton — there's exactly one TDC1000 on this board. */
Tdc1000& tdc1000();

} /* namespace uflow::drivers */
