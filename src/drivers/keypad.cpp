/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Keypad driver implementation. See keypad.hpp for the API contract and
 * the parity statement vs. the embassy-era Rust driver.
 *
 * Architecture:
 *   1. One gpio_callback per pin, all firing a single ISR that
 *      distinguishes press vs release by reading the pin level
 *      (GPIO_INT_EDGE_BOTH — was EDGE_TO_ACTIVE in the single-press
 *      variant; both-edge needed to detect release for repeat cancel).
 *   2. On press: post a KeyEvent + schedule a per-button delayable
 *      work for INITIAL_REPEAT_DELAY_MS (1 s). Apply DEBOUNCE_MS
 *      gate to reject contact bounce.
 *   3. The repeat_work handler re-reads the pin: if still pressed,
 *      post another KeyEvent + reschedule for REPEAT_INTERVAL_MS
 *      (150 ms); if released, do nothing.
 *   4. On release: cancel any pending repeat_work for that button.
 *
 * Repeat timing (1 s initial, 150 ms interval) matches the embassy
 * keypad_task constants exactly — see rework/embassy:src/drivers/keypad.rs
 * REPEAT_DELAY / REPEAT_INTERVAL.
 */

#include "keypad.hpp"

#include <cstddef>

#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/dt-bindings/input/input-event-codes.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(keypad, CONFIG_LOG_DEFAULT_LEVEL);

namespace uflow::drivers {

namespace {

#define KEYPAD_NODE DT_CHOSEN(uflowmeter_keypad)

static_assert(DT_NODE_EXISTS(KEYPAD_NODE),
	"DT chosen `uflowmeter,keypad` is missing — check your board DTS");

constexpr std::uint32_t DEBOUNCE_MS              = 20;
constexpr std::uint32_t INITIAL_REPEAT_DELAY_MS  = 1000;
constexpr std::uint32_t REPEAT_INTERVAL_MS       = 150;

struct Button {
	struct gpio_dt_spec spec;
	struct gpio_callback cb;
	std::uint16_t code;
	std::uint32_t last_event_ms;
	struct k_work_delayable repeat_work;
};

#define BUTTON_ENTRY(node_id) {                                   \
	.spec = GPIO_DT_SPEC_GET(node_id, gpios),                 \
	.cb = {},                                                 \
	.code = DT_PROP(node_id, zephyr_code),                    \
	.last_event_ms = 0,                                       \
	.repeat_work = {},                                        \
},

Button buttons[] = {
	DT_FOREACH_CHILD_STATUS_OKAY(KEYPAD_NODE, BUTTON_ENTRY)
};

constexpr std::size_t button_count = sizeof(buttons) / sizeof(buttons[0]);

K_MSGQ_DEFINE(key_msgq, sizeof(KeyEvent), 8, 4);

void post_key(Button* btn)
{
	KeyEvent ev{btn->code};
	(void)k_msgq_put(&key_msgq, &ev, K_NO_WAIT);
}

void repeat_work_handler(struct k_work* w)
{
	struct k_work_delayable* dwork = k_work_delayable_from_work(w);
	Button* btn = CONTAINER_OF(dwork, Button, repeat_work);

	/* Re-read pin level. If released (electrical HIGH on active-LOW
	 * line), gpio_pin_get_dt returns 0 → stop repeating. */
	if (gpio_pin_get_dt(&btn->spec) == 0) {
		return;
	}
	post_key(btn);
	k_work_schedule(&btn->repeat_work, K_MSEC(REPEAT_INTERVAL_MS));
}

void on_button_isr(const struct device* port, struct gpio_callback* cb, gpio_port_pins_t pins)
{
	ARG_UNUSED(port);
	ARG_UNUSED(pins);

	Button* btn = nullptr;
	for (auto& candidate : buttons) {
		if (&candidate.cb == cb) {
			btn = &candidate;
			break;
		}
	}
	if (btn == nullptr) {
		return;
	}

	/* Read pin level to distinguish press vs release (both-edge IRQ).
	 * Active-LOW: gpio_pin_get_dt returns 1 when pressed (line LOW). */
	const bool pressed = gpio_pin_get_dt(&btn->spec) != 0;

	if (!pressed) {
		/* Release — cancel any pending repeat. */
		k_work_cancel_delayable(&btn->repeat_work);
		return;
	}

	const std::uint32_t now = k_uptime_get_32();
	if (now - btn->last_event_ms < DEBOUNCE_MS) {
		return;
	}
	btn->last_event_ms = now;

	post_key(btn);
	/* Arm initial repeat. Cancels any prior schedule first
	 * (idempotent if no work was pending). */
	k_work_schedule(&btn->repeat_work, K_MSEC(INITIAL_REPEAT_DELAY_MS));
}

} /* namespace */

int keypad_init()
{
	for (auto& btn : buttons) {
		if (!gpio_is_ready_dt(&btn.spec)) {
			LOG_ERR("keypad pin %u port not ready", btn.spec.pin);
			return -ENODEV;
		}
		int rc = gpio_pin_configure_dt(&btn.spec, GPIO_INPUT);
		if (rc < 0) {
			LOG_ERR("gpio_pin_configure_dt failed (%d) for pin %u", rc, btn.spec.pin);
			return rc;
		}
		/* Both edges — need release edge to cancel repeat. */
		rc = gpio_pin_interrupt_configure_dt(&btn.spec, GPIO_INT_EDGE_BOTH);
		if (rc < 0) {
			LOG_ERR("gpio_pin_interrupt_configure_dt failed (%d) for pin %u",
				rc, btn.spec.pin);
			return rc;
		}
		gpio_init_callback(&btn.cb, on_button_isr, BIT(btn.spec.pin));
		rc = gpio_add_callback(btn.spec.port, &btn.cb);
		if (rc < 0) {
			LOG_ERR("gpio_add_callback failed (%d) for pin %u", rc, btn.spec.pin);
			return rc;
		}
		k_work_init_delayable(&btn.repeat_work, repeat_work_handler);
	}
	LOG_INF("keypad: %zu buttons armed (repeat %u/%u ms)", button_count,
		static_cast<unsigned>(INITIAL_REPEAT_DELAY_MS),
		static_cast<unsigned>(REPEAT_INTERVAL_MS));
	return 0;
}

int keypad_recv(KeyEvent& out, k_timeout_t timeout)
{
	return k_msgq_get(&key_msgq, &out, timeout);
}

} /* namespace uflow::drivers */
