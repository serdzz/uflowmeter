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
#include "../history.hpp"
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

/* Field-stepping sequence on HourHistory / DayHistory / MonthHistory.
 * Per-kind starting field (matches embassy HistoryWidget::next_item):
 *   HourHistory  starts at Hour, walks Hour → Day → Month → Year → exit
 *   DayHistory   starts at Day,  walks Day  → Month → Year → exit
 *   MonthHistory starts at Month, walks Month → Year → exit
 *
 * Unlike DateTime, the "Year → None" transition does NOT emit a commit
 * AppRequest — SetHistory fires on every Left/Right press while
 * editable, so the backing data stays live with the cursor. */
enum class HistoryEditField : std::uint8_t {
	None = 0,
	Hour,
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

	/* History edit introspection — render() reads these to bracket
	 * the active field and source the date row from the working
	 * buffer instead of the live wall clock. */
	bool is_editing_history() const { return history_field_ != HistoryEditField::None; }
	HistoryEditField current_history_field() const { return history_field_; }
	const datetime::DateTimeFields& edited_history_datetime() const { return edited_history_datetime_; }

	/* Last history query — kind + ts the caller should send to
	 * history::query() after a SetHistory return. */
	history::HistoryType last_history_kind() const { return last_history_kind_; }
	std::uint32_t last_history_timestamp() const { return last_history_timestamp_; }

	/* True when a multi-field edit is open (DateTime or History) —
	 * main loop uses this to drop the keypad_recv timeout from 2 s
	 * to 150 ms so blink animation runs at a perceptible cadence. */
	bool has_active_field_edit() const {
		return datetime_field_ != DateTimeEditField::None ||
		       history_field_  != HistoryEditField::None;
	}

	/* Blink: render() bumps this counter on every paint; the value
	 * is the on/off half-cycle indicator for the active field.
	 * Embassy uses 6 frames per cycle (300 ms on / 300 ms off at
	 * 10 Hz); we match the frame count, the period depends on the
	 * main loop's current refresh tick (150 ms while editing →
	 * ~450/450 ms split, perceptible). */
	void tick_blink() { blink_frame_ = (blink_frame_ + 1) % BLINK_PERIOD; }
	bool is_blink_visible() const { return blink_frame_ < BLINK_PERIOD / 2; }

	static constexpr std::uint8_t BLINK_PERIOD = 6;

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

	/* History edit state + helpers — one set, the active history-
	 * kind is derived from the current screen (HourHistory →
	 * HistoryType::Hour etc.). edited_history_datetime_ is the
	 * working buffer; last_history_* is the snapshot for SetHistory. */
	HistoryEditField history_field_{HistoryEditField::None};
	datetime::DateTimeFields edited_history_datetime_{};
	history::HistoryType last_history_kind_{history::HistoryType::Hour};
	std::uint32_t last_history_timestamp_{0};

	AppRequest history_event(UiEvent ev, ScreenId screen);
	void history_advance_field(history::HistoryType kind);
	AppRequest history_emit_set(history::HistoryType kind);
	static history::HistoryType kind_for_screen(ScreenId s);

	/* Version screen easter-egg — Enter, Enter, Enter, Up, Up, Down,
	 * Down → AppRequest::EnterCalibration. Wrong key OR screen change
	 * resets the matcher. */
	AppRequest version_event(UiEvent ev);
	std::uint8_t pattern_matched_{0};
	ScreenId last_screen_for_pattern_{ScreenId::_Count};

	std::uint8_t blink_frame_{0};
};

} /* namespace uflow::ui */
