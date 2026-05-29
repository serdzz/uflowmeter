/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * UI key events — abstraction layer above the keypad driver. The
 * keypad emits INPUT_KEY_* codes from <zephyr/dt-bindings/input/...>;
 * this enum is what MenuController + screen widgets consume.
 *
 * Mapping (preserves the embassy convention from
 * git show rework/embassy:src/main.rs, lines 365-380):
 *   INPUT_KEY_MENU   →  Back   (Config button = back / exit)
 *   INPUT_KEY_ENTER  →  Enter
 *   INPUT_KEY_LEFT   →  Down   (hardware "Down" maps to UI Left/Prev)
 *   INPUT_KEY_RIGHT  →  Up     (hardware "Up" maps to UI Right/Next)
 */

#pragma once

#include <cstdint>

namespace uflow::ui {

enum class UiEvent : std::uint8_t {
	Up,     /* hardware "Up" button   */
	Down,   /* hardware "Down" button */
	Left,   /* unused on a 4-key panel — reserved for richer keypads */
	Right,
	Enter,
	Back,
};

/* Convert a Zephyr INPUT_KEY_* code into a UiEvent. Returns true on a
 * known mapping. Implemented in events.cpp to keep this header
 * dependency-light. */
bool ui_event_from_input_code(std::uint16_t input_code, UiEvent& out);

} /* namespace uflow::ui */
