/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * UI screen + menu identifiers. Mirrors the embassy ScreenId / MenuId
 * enums verbatim — the wire-level UX (which screen comes after which,
 * which menu owns which screen) needs to match what existing units
 * train their users on.
 *
 * Source: git show rework/embassy:src/ui.rs (top of file).
 */

#pragma once

#include <cstdint>

namespace uflow::ui {

/* Which of the four top-level menus is active. None = no menu selected,
 * the LCD shows a static "press any key" hint. */
enum class MenuId : std::uint8_t {
	None = 0,
	Main,
	User,
	Calibration,
	Configuration,
};

/* All 19 screens. Order within each menu mirrors the embassy
 * MenuController constructor's `add()` sequence — keep stable. */
enum class ScreenId : std::uint8_t {
	/* Main menu — 14 screens, navigated with Up/Down */
	HourConsumption = 0,
	DayConsumption,
	TotalVolume,
	Uptime,
	HourHistory,
	DayHistory,
	MonthHistory,
	DateTime,
	Version,
	Bootloader,
	CommType,
	SlaveAddress,
	Muster,
	Negative,

	/* User menu — 2 screens */
	Channel1,
	Channel2,

	/* Configuration menu — 2 screens */
	SensorType,
	SerialNumber,

	/* Calibration menu — 1 screen */
	Calibration,

	/* Sentinel — equals count of real screens. Don't add screens
	 * after this. */
	_Count,
};

} /* namespace uflow::ui */
