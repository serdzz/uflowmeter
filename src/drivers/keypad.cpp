/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Keypad driver implementation. See keypad.hpp for the API contract and
 * the parity statement vs. the embassy-era Rust driver.
 *
 * Architecture: one gpio_callback per pin (Zephyr requires a separate
 * callback per port-pin combo to demultiplex efficiently), all firing
 * a single ISR handler that:
 *   1. Reads back the level to confirm the line is actually LOW
 *      (rejects spurious wake from the EXTI shared line).
 *   2. Applies a coarse software debounce by gating events to once per
 *      DEBOUNCE_INTERVAL per button via k_uptime_get_32().
 *   3. Posts a KeyEvent into the static k_msgq the consumer reads.
 *
 * No zpp wrapper here — zpp covers k_fifo (linked-list queues that
 * require pointer ownership) but not k_msgq (value-copy queues). For a
 * tiny POD event a k_msgq is the right tool; we'll layer zpp::thread on
 * top later when the UI consumer thread lands.
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

constexpr std::uint32_t DEBOUNCE_MS = 20;

/* Expand one DT child node into a {spec, callback, code} triple. Each
 * keypad child node in the DTS has its own slot. */
struct Button {
	struct gpio_dt_spec spec;
	struct gpio_callback cb;
	std::uint16_t code;
	std::uint32_t last_event_ms;
};

#define BUTTON_ENTRY(node_id) {                                   \
	.spec = GPIO_DT_SPEC_GET(node_id, gpios),                 \
	.cb = {},                                                 \
	.code = DT_PROP(node_id, zephyr_code),                    \
	.last_event_ms = 0,                                       \
},

Button buttons[] = {
	DT_FOREACH_CHILD_STATUS_OKAY(KEYPAD_NODE, BUTTON_ENTRY)
};

constexpr std::size_t button_count = sizeof(buttons) / sizeof(buttons[0]);

K_MSGQ_DEFINE(key_msgq, sizeof(KeyEvent), 8, 4);

void on_button_isr(const struct device* port, struct gpio_callback* cb, gpio_port_pins_t pins)
{
	ARG_UNUSED(port);
	ARG_UNUSED(pins);

	/* Locate the button whose callback fired. CONTAINER_OF would be
	 * cleaner but C++ aggregate access keeps the macro at bay. */
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

	/* Confirm the line is still LOW (rejects shared-EXTI ghosts). */
	if (gpio_pin_get_dt(&btn->spec) == 0) {
		return;
	}

	const std::uint32_t now = k_uptime_get_32();
	if (now - btn->last_event_ms < DEBOUNCE_MS) {
		return;
	}
	btn->last_event_ms = now;

	KeyEvent ev{btn->code};
	(void)k_msgq_put(&key_msgq, &ev, K_NO_WAIT);
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
		rc = gpio_pin_interrupt_configure_dt(&btn.spec, GPIO_INT_EDGE_TO_ACTIVE);
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
	}
	LOG_INF("keypad: %zu buttons armed", button_count);
	return 0;
}

int keypad_recv(KeyEvent& out, k_timeout_t timeout)
{
	return k_msgq_get(&key_msgq, &out, timeout);
}

} /* namespace uflow::drivers */
