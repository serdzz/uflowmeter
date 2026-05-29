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
#include <zephyr/drivers/interrupt_controller/gpio_intc_stm32.h>
#include <zephyr/dt-bindings/pinctrl/stm32-pinctrl-common.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "../datetime.hpp"
#include "../modbus.hpp"
#include "../modbus_handler.hpp"
#include "../options.hpp"
#include "../shell.hpp"
#include "eeprom_power.hpp"

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

/* STOP-mode wake callback. STM32L1 USART has no UESM (that's L0/L4+
 * only), so the chip's USART clock dies when we enter STOP via the
 * PM hook in src/power.cpp. PA10 (USART1_RX) is still a live GPIO
 * input regardless — EXTI line 10 samples the pad voltage AND
 * triggers wake from STOP on any enabled edge. We arm the falling
 * edge: peer's start bit pulls PA10 low, EXTI wakes the chip,
 * pm_state_exit_post_ops restores HSE+PLL+USART clock (~50 ms),
 * and subsequent bytes are captured by the standard uart_isr.
 *
 * The wake-byte is lost (USART clock was off when the start edge
 * arrived). Modbus master notices the missing/garbled byte via CRC
 * and retries on its own timeout. Shell users see the first
 * character of the wake-up command swallowed; terminal echo helps
 * the operator notice and re-type.
 *
 * The callback itself does nothing — the EXTI dispatcher clears
 * the pending bit, the USART driver picks up bytes via its own IRQ.
 * We register a no-op so Zephyr's intc dispatcher has something to
 * call (otherwise the line would be unclaimed). */
void pa10_wake_cb(gpio_port_pins_t, void*)
{
	/* Intentionally empty. See comment above. */
}

void uart_isr(const struct device* dev, void* /*user*/)
{
	/* Zephyr 4.4 uart_irq_update returns void — it just refreshes
	 * the cached IRQ status that the *_ready / *_pending queries
	 * below consume. */
	uart_irq_update(dev);
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

void dispatch_shell_action(shell::Action action, const struct device* eeprom)
{
	switch (action.kind) {
	case shell::ActionKind::None:
		return;
	case shell::ActionKind::SetDateUnix: {
		/* Embassy used the Unix epoch directly; our datetime
		 * utility uses seconds-since-2000-01-01 internally.
		 * Convert: 2000-01-01T00:00:00Z = 946684800 (Unix). */
		constexpr std::uint32_t UNIX_TO_2000 = 946684800u;
		std::uint32_t ts_2000 = (action.value > UNIX_TO_2000)
			? (action.value - UNIX_TO_2000) : 0u;
		datetime::set(datetime::from_timestamp(ts_2000));
		LOG_INF("shell: SetDateUnix %u", action.value);
		return;
	}
	case shell::ActionKind::SetSerial:
		options::g_options.serial_number = action.value;
		(void)options::save_through_dp(eeprom, options_save_scratch);
		LOG_INF("shell: SetSerial %u", action.value);
		return;
	case shell::ActionKind::SetVerbose:
		LOG_INF("shell: SetVerbose %u (log-only)", action.value);
		return;
	}
}

/* Hand a completed line to the shell. Writes reply (if any) back over
 * USART and dispatches the side-effect action. */
void handle_shell_line(const std::uint8_t* line, std::size_t len,
                       const struct device* eeprom)
{
	shell::Result r = shell::process_line(line, len);
	if (r.kind != shell::ResultKind::NotAShellCommand && r.text_len > 0) {
		uart_write_blocking(reinterpret_cast<const std::uint8_t*>(r.text),
			r.text_len);
	}
	shell::Action a = shell::parse_action(line, len);
	dispatch_shell_action(a, eeprom);
}

/* Hand a completed Modbus frame to the handler. Writes response over
 * USART when one is generated. */
void handle_modbus_frame(const std::uint8_t* frame, std::size_t len,
                         const struct device* eeprom)
{
	std::uint8_t response[MAX_FRAME];
	std::size_t resp_len = 0;
	modbus::Error err = modbus_handler::instance().process(
		frame, len,
		eeprom, options_save_scratch,
		response, sizeof(response), &resp_len);
	if (resp_len > 0) {
		uart_write_blocking(response, resp_len);
		LOG_DBG("modbus: rx %zu B → tx %zu B", len, resp_len);
	} else if (err == modbus::Error::InvalidSlaveAddr) {
		/* Silent drop — not for us. */
	} else if (err != modbus::Error::Ok) {
		LOG_DBG("modbus drop (err %d)", static_cast<int>(err));
	}
}

void uart_worker(void*, void*, void*)
{
	LOG_INF("uart worker up");

	const struct device* eeprom = DEVICE_DT_GET(DT_CHOSEN(uflowmeter_eeprom));

	/* Dual buffers per embassy's heuristic. Bytes are appended to
	 * both; \r\n flushes the line buffer (shell path), FRAME_GAP
	 * silence flushes the frame buffer (Modbus path). Shell input
	 * with inter-byte pauses still works because the Modbus framer's
	 * partial flushes get rejected by CRC and silently dropped. */
	std::uint8_t frame[MAX_FRAME];
	std::size_t  frame_len = 0;
	std::uint8_t line[shell::MAX_LINE];
	std::size_t  line_len = 0;

	for (;;) {
		std::uint8_t byte;
		const k_timeout_t timeout =
			(frame_len == 0) ? K_FOREVER : FRAME_GAP;
		int rc = k_msgq_get(&rx_byte_msgq, &byte, timeout);

		if (rc == 0) {
			/* New byte — append to frame buffer (overflow →
			 * drop frame, reset). */
			if (frame_len < MAX_FRAME) {
				frame[frame_len++] = byte;
			} else {
				LOG_WRN("frame overflow > %zu B, dropping",
					MAX_FRAME);
				frame_len = 0;
			}

			/* Line buffer dispatch on \r or \n. */
			if (byte == '\r' || byte == '\n') {
				if (line_len > 0) {
					handle_shell_line(line, line_len, eeprom);
					line_len = 0;
				}
			} else if (line_len < shell::MAX_LINE) {
				line[line_len++] = byte;
			} else {
				/* Line overflow — reset; user re-types. */
				static const char overflow_msg[] = "ERR: line too long\r\n";
				uart_write_blocking(
					reinterpret_cast<const std::uint8_t*>(overflow_msg),
					sizeof(overflow_msg) - 1);
				line_len = 0;
			}
			continue;
		}

		/* Timeout — emit Modbus frame if non-empty. */
		if (frame_len > 0) {
			handle_modbus_frame(frame, frame_len, eeprom);
			frame_len = 0;
		}

		/* Note: line buffer is NOT cleared on FRAME_GAP timeout.
		 * Human typing produces inter-byte pauses > 1.75 ms; the
		 * line keeps accumulating until \r\n. The Modbus partial
		 * frame gets dispatched + CRC-rejected silently each time. */
	}
	(void)SESSION_IDLE;  /* reserved for future STOP-wake integration */
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

	/* Arm PA10 (USART1 RX) as a STOP-mode wake source. PA10 is in
	 * USART AF — Zephyr's USART driver owns the pin config — but
	 * EXTI samples the pad voltage independent of GPIO mode, so
	 * the wake works without taking the pin away from USART. */
	const stm32_gpio_irq_line_t pa10_line =
		stm32_gpio_intc_get_pin_irq_line(STM32_PORTA, 10);
	stm32_exti_set_line_src_port(10, STM32_PORTA);
	stm32_gpio_intc_select_line_trigger(pa10_line,
		STM32_GPIO_IRQ_TRIG_FALLING);
	int wake_rc = stm32_gpio_intc_set_irq_callback(pa10_line,
		pa10_wake_cb, nullptr);
	if (wake_rc < 0) {
		LOG_WRN("PA10 wake callback registration failed (%d) — "
		        "UART bytes during STOP will be lost without "
		        "wake from other sources", wake_rc);
	} else {
		stm32_gpio_intc_enable_line(pa10_line);
		LOG_INF("uart STOP-wake armed on PA10 falling edge");
	}

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
