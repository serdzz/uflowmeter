/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * TDC7200 driver — time-to-digital converter. Latches the time
 * between TDC1000 TX and RX comparator events at sub-nanosecond
 * resolution. Reports completion by pulling INT (PB0) LOW.
 *
 * Pin context (from DT chosen uflowmeter,tdc7200):
 *   SPI   — &spi2, CS=PB12 (cs-gpios[2])
 *   EN    — PB1,  active-HIGH
 *   INT   — PB0,  active-LOW, EXTI0
 *
 * Register format (SPI):
 *   write: cmd = (0x40 | (addr & 0x1F)), then data byte(s)
 *   read:  cmd = (addr & 0x1F), then read data byte(s)
 *   The auto-increment bit (0x40) on writes is REQUIRED for
 *   multi-byte transfers; without it every byte after the first
 *   is silently discarded. Single-byte writes work either way;
 *   we set it for safety.
 *
 * The 24-bit TIME / CLOCK_COUNT registers are read as a single 3-byte
 * burst (big-endian on the wire).
 *
 * Single-threaded usage assumed; ISR-driven INT signals the
 * measurement thread via a static k_sem. Driver init wires up the
 * GPIO callback once at boot. wait_for_completion() may only be
 * called from a thread context.
 */

#pragma once

#include <cstddef>
#include <cstdint>

#include <zephyr/kernel.h>

namespace uflow::drivers {

class Tdc7200 {
public:
	/* Wires up EN as output (starts LOW), INT as falling-edge
	 * interrupt source. Idempotent — safe to call once at boot. */
	int init();

	void power_on();
	void power_off();

	int read_register(std::uint8_t address, std::uint8_t& out_value);
	int write_register(std::uint8_t address, std::uint8_t value);

	/* Read a 24-bit register (big-endian on the wire — bit 23 first).
	 * TDC7200 TIME1..TIME5 and CLOCK_COUNT1..5 all use this format. */
	int read_u24(std::uint8_t address, std::uint32_t& out_value);

	int load_config(const std::uint8_t* regs, std::size_t len);

	/* Start a single ToF measurement (writes CONFIG1.START_MEAS=1).
	 * INT will fall once the chip has captured the timestamp. */
	int start_measurement();

	/* Convenience: read TIME1 (reg 0x10). */
	int read_time1(std::uint32_t& out_value);

	/* Block until INT falls or timeout. Returns 0 if INT seen, -EAGAIN
	 * on timeout, -errno otherwise. Must be called from thread
	 * context. The semaphore is reset at start_measurement() time so
	 * stale events from prior cycles don't fire this immediately. */
	int wait_for_completion(k_timeout_t timeout);
};

/* Singleton — one TDC7200 on the board. */
Tdc7200& tdc7200();

} /* namespace uflow::drivers */
