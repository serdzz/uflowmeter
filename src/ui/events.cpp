/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "events.hpp"

#include <zephyr/dt-bindings/input/input-event-codes.h>

namespace uflow::ui {

bool ui_event_from_input_code(std::uint16_t input_code, UiEvent& out)
{
	switch (input_code) {
	case INPUT_KEY_MENU:  out = UiEvent::Back;  return true;
	case INPUT_KEY_ENTER: out = UiEvent::Enter; return true;
	/* Hardware Down (PB8) → UI Down (prev screen). */
	case INPUT_KEY_LEFT:  out = UiEvent::Down;  return true;
	/* Hardware Up (PB9) → UI Up (next screen). */
	case INPUT_KEY_RIGHT: out = UiEvent::Up;    return true;
	default:              return false;
	}
}

} /* namespace uflow::ui */
