/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * MenuController — the UI state machine.
 *
 * Owns four MenuLists (one per top-level menu), the currently active
 * MenuId, AND the per-screen edit state for the five editable screens
 * (CommType, SlaveAddress, Muster, Negative, SensorType).
 *
 * Edit cycle:
 *   1. User navigates to an editable screen via Up/Down.
 *   2. Press Enter — capture current value from options::g_options
 *      into the per-screen edit state, set editable=true.
 *   3. While editable: Left/Right cycle the cursor (EditBox) or
 *      Up/Down increment the value (EditNumber). Up/Down also work
 *      on EditBox screens for keypads without distinct Left/Right.
 *   4. Press Enter again — emit Set* AppRequest, set editable=false.
 *      Caller reads last_edit_value() to recover the committed u8.
 *   5. Press Back while editable — cancel edit (no AppRequest).
 *      Back at the menu-navigation level deselects the menu.
 *
 * This is UI port commit 2/6. Still missing: DateTime field-stepping,
 * HistoryWidget date picker, Version easter-egg, Cyrillic CGRAM.
 *
 * Source: git show rework/embassy:src/ui.rs MenuController.
 */

#pragma once

#include <cstdint>

#include "../datetime.hpp"
#include "app_request.hpp"
#include "events.hpp"
#include "menu_list.hpp"
#include "screen.hpp"

namespace uflow::ui {

/* Field-stepping sequence on the DateTime screen — matches the embassy
 * DateTimeWidget::next_item order: None → Seconds → Minutes → Hours →
 * Day → Month → Year → None (commit on the last → None transition). */
enum class DateTimeEditField : std::uint8_t {
	None = 0,
	Seconds,
	Minutes,
	Hours,
	Day,
	Month,
	Year,
};

struct EditBoxState {
	std::uint8_t cursor{0};
	std::uint8_t max{1};      /* upper exclusive bound — cursor wraps 0..max-1 */
	bool editable{false};
};

struct EditNumberState {
	std::uint8_t value{0};
	std::uint8_t min{0};
	std::uint8_t max{255};
	std::uint8_t step{1};
	bool editable{false};
};

class MenuController {
public:
	MenuController();

	/* Process a key event. Returns an AppRequest when the user
	 * confirms an edit or triggers a system action. Display always
	 * needs a refresh — caller should re-render unconditionally
	 * after every event(). */
	AppRequest event(UiEvent ev);

	MenuId active_menu() const { return active_; }
	ScreenId current_screen() const;

	void select(MenuId menu);
	void deselect() { active_ = MenuId::None; }

	/* Edit-state introspection — render() uses these to decide
	 * whether to show the saved value or the in-progress cursor,
	 * and to wrap the value in [brackets] when editable. */
	bool is_editing_current() const;
	std::uint8_t edit_cursor_for_current() const;

	/* Value most recently committed by an Enter on an editable
	 * screen. Read on the same tick as a Set* AppRequest. */
	std::uint8_t last_edit_value() const { return last_edit_value_; }

	/* Memory-only Muster flag — no Options field exists for the
	 * verification mode; storing it here means it resets on reboot.
	 * Documented limitation; fix when Options layout is bumped. */
	bool muster_active() const { return muster_.cursor != 0; }

	/* DateTime edit introspection — render() reads these to decide
	 * which field gets [brackets] on the DateTime screen, and to
	 * show the in-progress edit buffer instead of live datetime. */
	bool is_editing_datetime() const { return datetime_field_ != DateTimeEditField::None; }
	DateTimeEditField current_datetime_field() const { return datetime_field_; }
	const datetime::DateTimeFields& edited_datetime() const { return edited_datetime_; }

	/* DateTime committed on the last Year→None transition. Read on
	 * the tick AppRequest::SetDateTime is returned. */
	const datetime::DateTimeFields& last_committed_datetime() const { return last_committed_datetime_; }

private:
	static bool is_screen_enabled(ScreenId s, void* ctx);
	MenuList* current_list();

	/* Returns nullptr for non-editable screens. The four editable
	 * cycle-selectors share an EditBoxState; SlaveAddress uses
	 * EditNumberState. */
	EditBoxState*    editbox_for(ScreenId s);
	EditNumberState* editnumber_for(ScreenId s);

	/* Per-edit-mode tick handlers. Return AppRequest::None when no
	 * commit happened. */
	AppRequest editbox_event(EditBoxState& st, ScreenId s, UiEvent ev);
	AppRequest editnumber_event(EditNumberState& st, ScreenId s, UiEvent ev);

	/* Snapshot current options::g_options into the edit cursor when
	 * entering edit mode, so the in-progress value starts from the
	 * persisted state. */
	void load_from_options(ScreenId s);

	/* Map a screen to the AppRequest variant its Enter commits. */
	AppRequest commit_request_for(ScreenId s) const;

	MenuId   active_{MenuId::None};
	MenuList main_;
	MenuList user_;
	MenuList calibration_;
	MenuList configuration_;

	EditBoxState    comm_type_;     /* cursor 0..3 */
	EditNumberState slave_address_; /* 1..250 */
	EditBoxState    muster_;        /* 0..1 */
	EditBoxState    negative_;      /* 0..1 */
	EditBoxState    sensor_type_;   /* 0..4 */

	std::uint8_t last_edit_value_{0};

	/* DateTime edit state. `datetime_field_ != None` means we're
	 * mid-stepping through Year/Month/Day/Hours/Minutes/Seconds —
	 * edited_datetime_ is the working copy that increments / decrements
	 * with Right / Left and snapshots into last_committed_datetime_
	 * on the final Enter. */
	DateTimeEditField datetime_field_{DateTimeEditField::None};
	datetime::DateTimeFields edited_datetime_{};
	datetime::DateTimeFields last_committed_datetime_{};

	/* DateTime field-stepping helpers. */
	AppRequest datetime_event(UiEvent ev);
	void datetime_advance_field();   /* None → Sec → Min → Hr → Day → Mo → Yr → None */
};

} /* namespace uflow::ui */
