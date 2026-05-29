/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * AppRequest — system-level actions the UI emits in response to
 * specific key events on specific screens. The embassy version
 * carries data (datetime payload, history kind+timestamp, etc) but
 * this commit only handles display-only screens, so the variants
 * that need data are stubbed without payload. We'll grow them in
 * the follow-up commits as the edit-mode logic lands.
 *
 * Source: git show rework/embassy:src/apps.rs.
 */

#pragma once

#include <cstdint>

namespace uflow::ui {

enum class AppRequest : std::uint8_t {
	None = 0,        /* no action — keep this so callers can default-init */
	DeepSleep,       /* user pressed Back at root */
	EnterCalibration,
	SystemReset,     /* Bootloader screen Enter */
	/* Data-bearing requests (SetDateTime, SetHistory, SetCommType,
	 * SetAddress, SetMuster, SetNegative) come in the edit-mode
	 * commit alongside their consumers. */
};

} /* namespace uflow::ui */
