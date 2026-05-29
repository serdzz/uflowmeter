/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * USART1 Modbus RTU transport — layer 3 of 3 in the RTU port.
 *
 * IRQ-driven RX into a static byte queue. A worker thread drains the
 * queue with a 1.75 ms timeout (Modbus RTU 3.5-char silence floor at
 * baudrates ≥ 19200; we use it unconditionally). On timeout with a
 * non-empty frame buffer, the buffer goes to
 * modbus_handler::instance().process() and the response (if any) gets
 * written back via uart_poll_out.
 *
 * Power trade-off in this commit
 * ------------------------------
 * USART1 is left always enabled (Zephyr's stm32 serial driver
 * configures it at boot). When the kernel enters STOP via the PM
 * policy, the USART clock is gated and bytes during STOP are LOST
 * until something else wakes the chip (RTC WUT, keypad EXTI).
 *
 * Embassy's port had EXTI-on-PA10 + per-session USART init/deinit so
 * the first start bit during STOP woke the chip. Porting that to
 * Zephyr requires shared ownership of PA10 between the USART driver
 * and our EXTI configuration — a non-trivial pinctrl + SYSCFG dance
 * that's its own commit. Until then:
 *
 *   - For SCADA installations that need reliable Modbus polling,
 *     build with CONFIG_PM=n (chip stays running, ~2 mA continuous).
 *   - For low-power deployments where Modbus is rare, the default
 *     CONFIG_PM=y means peer must retry on missed frames — standard
 *     Modbus retry semantics handle this.
 *   - A future commit will add USART STOP-wake (PA10 EXTI + USART
 *     CR1.UESM) so both can coexist.
 */

#pragma once

namespace uflow::drivers::uart {

/* Initialize USART1 IRQ + spawn the transport worker thread.
 * Idempotent — only the first call starts the thread. Returns 0 on
 * success, negative errno on device-not-ready. */
int start();

} /* namespace uflow::drivers::uart */
