/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * UI rendering — paints the current screen to the HD44780. Sync, no
 * blink animation, Latin labels only this commit.
 *
 * The Rust version uses Cyrillic glyphs from CGRAM (8 custom-character
 * slots) — porting that means tracking glyph allocations across
 * frames, which is a separate concern. For now we render ASCII labels
 * that map to the same screens.
 *
 * Source-of-data for live values:
 *   options::g_options       — serial number, slave address, comm_type
 *   measurement::latest_flow_m3h — current flow rate
 *
 * Source: git show rework/embassy:src/ui.rs MenuController::render +
 *         format_value + title.
 */

#pragma once

#include "menu_controller.hpp"

namespace uflow::drivers { class Hd44780; }

namespace uflow::ui {

/* Paint the current state to the LCD. Caller MUST hold lcd_mutex.
 * Always re-writes both rows + pads to 16 chars so leftover state
 * from prior screens doesn't bleed through.
 *
 * Takes the controller by mutable reference because rendering advances
 * the blink phase (mc.tick_blink()) and resets CGRAM bookkeeping per
 * frame. */
void render(MenuController& mc, drivers::Hd44780& lcd);

} /* namespace uflow::ui */
