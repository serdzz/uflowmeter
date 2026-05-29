/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * 4-button keypad bound to PB6 (Config), PB7 (Enter), PB8 (Down/Left),
 * PB9 (Up/Right). All pins are pull-up; pressed reads LOW.
 *
 * This first milestone is interrupt-driven and single-press only — no
 * software repeat. The Rust embassy driver's REPEAT_DELAY (1 s) +
 * REPEAT_INTERVAL (150 ms) logic moves over in milestone 2 alongside
 * the UI state machine. See git show rework/embassy:src/drivers/keypad.rs
 * for the reference implementation.
 *
 * Pin bindings come from the DT chosen node `uflowmeter,keypad`
 * (compatible "gpio-keys"). The four child nodes carry `zephyr,code`
 * values from <zephyr/dt-bindings/input/input-event-codes.h>; this
 * driver surfaces them through KeyEvent::code unchanged so the
 * consumer can switch on INPUT_KEY_MENU / INPUT_KEY_ENTER /
 * INPUT_KEY_LEFT / INPUT_KEY_RIGHT.
 */

#pragma once

#include <cstdint>
#include <zephyr/kernel.h>

namespace uflow::drivers {

struct KeyEvent {
	std::uint16_t code;  /* INPUT_KEY_* from input-event-codes.h */
};

/* Initialize all four keypad pins as pull-up inputs with falling-edge
 * interrupts. Idempotent — safe to call once at boot. Returns 0 on
 * success, negative errno on failure. */
int keypad_init();

/* Block-wait for the next key event. `timeout` may be K_FOREVER. */
int keypad_recv(KeyEvent& out, k_timeout_t timeout = K_FOREVER);

} /* namespace uflow::drivers */
