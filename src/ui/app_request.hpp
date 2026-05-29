/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * AppRequest — system-level actions the UI emits in response to
 * specific key events on specific screens. Carries no payload
 * directly; for the Set* variants, MenuController exposes the
 * committed value via last_edit_value() — read it on the same tick
 * as the AppRequest is returned.
 *
 * Source: git show rework/embassy:src/apps.rs.
 */

#pragma once

#include <cstdint>

namespace uflow::ui {

enum class AppRequest : std::uint8_t {
	None = 0,
	DeepSleep,             /* Back at root */
	EnterCalibration,      /* Calibration screen Enter */
	SystemReset,           /* Bootloader screen Enter */

	/* Set* — caller reads MenuController::last_edit_value() to
	 * recover the committed u8. The handler is responsible for
	 * type-checking the field (e.g. SetCommType expects 0..3). */
	SetCommType,           /* options.comm_type   ← cursor 0..3 */
	SetSlaveAddress,       /* options.slave_address ← 1..250 */
	SetMuster,             /* memory-only this commit (no Options home) */
	SetNegative,           /* options.enable_negative ← cursor 0..1 */
	SetSensorType,         /* options.sensor_type ← cursor 0..4 */

	/* Caller reads MenuController::last_committed_datetime() (full
	 * DateTimeFields struct, not a single u8). */
	SetDateTime,

	/* Fires on every Left/Right press in HistoryWidget edit mode —
	 * caller reads last_history_kind() + last_history_timestamp(),
	 * calls history::query(), feeds the result into
	 * history::set_last_result() for render to pick up. */
	SetHistory,
};

} /* namespace uflow::ui */
