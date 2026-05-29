/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * MenuController — the UI state machine.
 *
 * Owns four MenuLists (one per top-level menu) + the currently active
 * MenuId. Consumes UiEvents, returns AppRequests when the user takes
 * an action that the system should act on (today: deselect → DeepSleep,
 * Bootloader Enter → SystemReset, Calibration Enter → EnterCalibration;
 * the data-bearing actions land in the edit-mode commit).
 *
 * This is commit 1 of the 6-commit UI port — see CLAUDE.md "UI port
 * status" for the remaining scope. Notably absent:
 *   - Edit-mode flags + cursors on CommType / SlaveAddress / Muster /
 *     Negative / SensorType.
 *   - DateTime field-stepping.
 *   - HistoryWidget date picker.
 *   - Version easter-egg pattern.
 *
 * Source: git show rework/embassy:src/ui.rs MenuController.
 */

#pragma once

#include "app_request.hpp"
#include "events.hpp"
#include "menu_list.hpp"
#include "screen.hpp"

namespace uflow::ui {

class MenuController {
public:
	MenuController();

	/* Process a key event. Returns AppRequest::None when no system-
	 * level action is needed. The caller should re-render whether
	 * or not None is returned — every key potentially advances the
	 * cursor. */
	AppRequest event(UiEvent ev);

	/* Currently-active menu (None means "no menu shown — press any
	 * key to wake"). */
	MenuId active_menu() const { return active_; }

	/* Currently-focused screen, or ScreenId::_Count when no menu is
	 * active. */
	ScreenId current_screen() const;

	/* Switch into a specific menu (e.g., Main on first wake, User
	 * on Enter-from-Uptime). Public so unit tests can drive the
	 * controller into specific shapes. */
	void select(MenuId menu);
	void deselect() { active_ = MenuId::None; }

private:
	/* SlaveAddress is hidden when CommType=Off — encoded here so
	 * MenuList::next/prev_enabled can call it. Static so it can be
	 * passed as a C function pointer; closes over `this` via the
	 * void* ctx slot. */
	static bool is_screen_enabled(ScreenId s, void* ctx);

	MenuList* current_list();

	MenuId   active_{MenuId::None};
	MenuList main_;
	MenuList user_;
	MenuList calibration_;
	MenuList configuration_;
};

} /* namespace uflow::ui */
