/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * USART1 transport implementation. See uart.hpp for the contract and
 * the power trade-off commentary.
 *
 * Pipeline:
 *   IRQ (RX_RDY) → uart_fifo_read into a small staging buffer →
 *   k_msgq_put per byte → worker thread k_msgq_get with K_USEC(1750)
 *   timeout → accumulate into frame_buf[256] → on timeout, hand
 *   frame to handler → write response via uart_poll_out.
 *
 * Why per-byte msgq instead of bulk DMA: STM32L1 USART1 supports DMA
 * but Zephyr's stm32-serial driver only exposes the bulk async API
 * (uart_rx_enable + RX_RDY callback). The async API doesn't give us
 * the per-byte-with-timeout semantics needed for inter-frame silence
 * detection without significant framing logic on the callback side.
 * Single-byte IRQ + worker is the simplest path that matches the
 * embassy "select on read+timeout+TX" structure.
 *
 * Outbound: uart_poll_out is blocking but at 115200 baud, the max
 * Modbus response (252 bytes) takes ~22 ms. Acceptable for a worker
 * thread that's already in "active processing" mode.
 */

#include "uart.hpp"

#include <cstddef>
#include <cstdint>

#include <zephyr/device.h>
#include <zephyr/drivers/uart.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "../modbus.hpp"
#include "../modbus_handler.hpp"
#include "../options.hpp"

LOG_MODULE_REGISTER(uart_transport, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers::uart {

namespace {

/* zephyr,console / zephyr,shell-uart are both USART1 (DT chosen).
 * Same device, just one driver instance. */
#define UART_NODE DT_CHOSEN(zephyr_console)

static_assert(DT_NODE_EXISTS(UART_NODE),
	"DT chosen `zephyr,console` (USART1) is missing — check board DTS");

const struct device* uart_dev_ = nullptr;

/* Inter-frame silence per Modbus RTU spec. At baudrates ≥ 19200 the
 * minimum is 1.75 ms regardless of actual char time. At 115200 the
 * real 3.5-char time is ~305 µs but the spec floor wins. */
constexpr k_timeout_t FRAME_GAP = K_USEC(1750);

/* Session idle: how long the worker stays in the active loop with no
 * bytes before going back to K_FOREVER block. Matches embassy's
 * 800 ms timeout — long enough for a retry round-trip after the
 * peer notices a missed frame. */
constexpr k_timeout_t SESSION_IDLE = K_MSEC(800);

constexpr std::size_t MAX_FRAME = modbus::MAX_FRAME;

K_MSGQ_DEFINE(rx_byte_msgq, sizeof(std::uint8_t), 512, 1);

/* Worker thread stack + scratch buffer for Options save during
 * write-register handling. Lives in BSS — too big for the 2 KB
 * worker stack. */
K_THREAD_STACK_DEFINE(uart_stack, 2048);
struct k_thread uart_thread_data;
k_tid_t uart_tid = nullptr;

std::uint8_t options_save_scratch[options::OPTIONS_PAGE_SIZE];

void uart_isr(const struct device* dev, void* /*user*/)
{
	if (!uart_irq_update(dev)) {
		return;
	}
	if (!uart_irq_rx_ready(dev)) {
		return;
	}
	std::uint8_t byte;
	while (uart_fifo_read(dev, &byte, 1) > 0) {
		(void)k_msgq_put(&rx_byte_msgq, &byte, K_NO_WAIT);
	}
}

void uart_write_blocking(const std::uint8_t* data, std::size_t len)
{
	for (std::size_t i = 0; i < len; i++) {
		uart_poll_out(uart_dev_, data[i]);
	}
}

void uart_worker(void*, void*, void*)
{
	LOG_INF("uart worker up");

	const struct device* eeprom = DEVICE_DT_GET(DT_CHOSEN(uflowmeter_eeprom));
	std::uint8_t frame[MAX_FRAME];
	std::size_t  frame_len = 0;
	std::uint8_t response[MAX_FRAME];

	for (;;) {
		std::uint8_t byte;
		k_timeout_t timeout;
		if (frame_len == 0) {
			/* No frame in progress — wait forever. The chip
			 * may enter STOP between sessions; UART IRQ will
			 * wake us (if USART clock was alive) or the next
			 * keypress/RTC tick will (if STOP gated USART). */
			timeout = K_FOREVER;
		} else {
			/* Mid-frame — race FRAME_GAP for boundary. */
			timeout = FRAME_GAP;
		}

		int rc = k_msgq_get(&rx_byte_msgq, &byte, timeout);
		if (rc == 0) {
			/* New byte — append to frame. Overflow → drop and
			 * reset; peer will retry. */
			if (frame_len >= MAX_FRAME) {
				LOG_WRN("frame overflow > %zu B, dropping", MAX_FRAME);
				frame_len = 0;
				continue;
			}
			frame[frame_len++] = byte;
			continue;
		}

		/* Timeout. If we have a frame in flight, dispatch it. */
		if (frame_len == 0) {
			continue;  /* spurious K_FOREVER timeout — shouldn't happen */
		}

		std::size_t resp_len = 0;
		modbus::Error err = modbus_handler::instance().process(
			frame, frame_len,
			eeprom, options_save_scratch,
			response, sizeof(response), &resp_len);
		frame_len = 0;

		if (err == modbus::Error::Ok && resp_len > 0) {
			uart_write_blocking(response, resp_len);
			LOG_DBG("modbus: rx %zu B → tx %zu B",
				static_cast<std::size_t>(0), resp_len);
		} else if (err == modbus::Error::InvalidSlaveAddr) {
			/* Silent drop — frame was for a different slave. */
		} else if (err != modbus::Error::Ok) {
			/* Codec returned an exception frame even on
			 * error paths (e.g. IllegalFunction). resp_len > 0
			 * means there's something to send. */
			if (resp_len > 0) {
				uart_write_blocking(response, resp_len);
			}
			LOG_DBG("modbus error %d", static_cast<int>(err));
		}

		/* After handling a frame, idle out the session window
		 * before going back to K_FOREVER block. Mirrors embassy's
		 * SESSION_IDLE_TIMEOUT — gives the peer time to send the
		 * next request without us deep-sleeping between bytes. */
		const std::int64_t deadline = k_uptime_get() + 800;
		while (k_uptime_get() < deadline) {
			rc = k_msgq_get(&rx_byte_msgq, &byte, K_MSEC(50));
			if (rc == 0) {
				frame[frame_len++] = byte;
				goto next_frame;  /* re-enter outer loop mid-frame */
			}
		}
		continue;
	next_frame:
		continue;
	}
}

} /* namespace */

int start()
{
	if (uart_tid != nullptr) {
		return 0;
	}
	uart_dev_ = DEVICE_DT_GET(UART_NODE);
	if (!device_is_ready(uart_dev_)) {
		LOG_ERR("USART1 device not ready");
		return -ENODEV;
	}
	int rc = uart_irq_callback_user_data_set(uart_dev_, uart_isr, nullptr);
	if (rc < 0) {
		LOG_ERR("uart_irq_callback_set failed: %d", rc);
		return rc;
	}
	uart_irq_rx_enable(uart_dev_);

	uart_tid = k_thread_create(
		&uart_thread_data,
		uart_stack,
		K_THREAD_STACK_SIZEOF(uart_stack),
		uart_worker,
		nullptr, nullptr, nullptr,
		K_PRIO_PREEMPT(6),
		0,
		K_NO_WAIT);
	k_thread_name_set(uart_tid, "uart_modbus");
	LOG_INF("uart transport: USART1 @ 115200, modbus on top");
	return 0;
}

} /* namespace uflow::drivers::uart */
