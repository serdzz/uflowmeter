/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "menu_controller.hpp"

#include "../options.hpp"

namespace uflow::ui {

MenuController::MenuController()
{
	/* Main menu — 14 screens in display order. Mirrors the embassy
	 * MenuController::new() exactly. */
	main_.add(ScreenId::HourConsumption);
	main_.add(ScreenId::DayConsumption);
	main_.add(ScreenId::TotalVolume);
	main_.add(ScreenId::Uptime);
	main_.add(ScreenId::HourHistory);
	main_.add(ScreenId::DayHistory);
	main_.add(ScreenId::MonthHistory);
	main_.add(ScreenId::DateTime);
	main_.add(ScreenId::Version);
	main_.add(ScreenId::Bootloader);
	main_.add(ScreenId::CommType);
	main_.add(ScreenId::SlaveAddress);
	main_.add(ScreenId::Muster);
	main_.add(ScreenId::Negative);

	user_.add(ScreenId::Channel1);
	user_.add(ScreenId::Channel2);

	configuration_.add(ScreenId::SensorType);
	configuration_.add(ScreenId::SerialNumber);

	calibration_.add(ScreenId::Calibration);
}

MenuList* MenuController::current_list()
{
	switch (active_) {
	case MenuId::Main:          return &main_;
	case MenuId::User:          return &user_;
	case MenuId::Calibration:   return &calibration_;
	case MenuId::Configuration: return &configuration_;
	case MenuId::None:          return nullptr;
	}
	return nullptr;
}

ScreenId MenuController::current_screen() const
{
	switch (active_) {
	case MenuId::Main:          return main_.current();
	case MenuId::User:          return user_.current();
	case MenuId::Calibration:   return calibration_.current();
	case MenuId::Configuration: return configuration_.current();
	case MenuId::None:          return ScreenId::_Count;
	}
	return ScreenId::_Count;
}

void MenuController::select(MenuId menu)
{
	active_ = menu;
	if (auto* list = current_list()) {
		list->reset();
	}
}

bool MenuController::is_screen_enabled(ScreenId s, void* /*ctx*/)
{
	/* SlaveAddress is meaningless when no comm protocol is selected
	 * — hide it from navigation. comm_type == 0 = "off". */
	if (s == ScreenId::SlaveAddress) {
		return options::g_options.comm_type != 0;
	}
	return true;
}

AppRequest MenuController::event(UiEvent ev)
{
	/* Back at any time exits to the no-menu state and signals the
	 * system to suspend. */
	if (ev == UiEvent::Back) {
		deselect();
		return AppRequest::DeepSleep;
	}

	/* First press after wake — bring up the main menu, no further
	 * action this tick. */
	if (active_ == MenuId::None) {
		select(MenuId::Main);
		return AppRequest::None;
	}

	auto* list = current_list();
	if (list == nullptr) {
		return AppRequest::None;
	}

	const ScreenId screen = list->current();

	/* Screen-specific Enter handling. The data-bearing actions
	 * (SetDateTime, SetHistory, edit-mode toggles) land in the
	 * next commit; for now we handle the three Enter→system-action
	 * screens + the Uptime → User menu hop. */
	if (ev == UiEvent::Enter) {
		switch (screen) {
		case ScreenId::Uptime:
			select(MenuId::User);
			return AppRequest::None;
		case ScreenId::Bootloader:
			return AppRequest::SystemReset;
		case ScreenId::Calibration:
			return AppRequest::EnterCalibration;
		default:
			/* Editable screens (DateTime, History, CommType, etc.)
			 * will start their edit modes here in the next commit.
			 * For now Enter is a no-op on those — we still rebuild
			 * the display, but no AppRequest. */
			return AppRequest::None;
		}
	}

	/* Up / Down advance through the active menu's ring. Left and
	 * Right are reserved for edit-mode cycling in future commits;
	 * they're treated as Up/Down here so a 4-button keypad behaves
	 * sensibly today. */
	if (ev == UiEvent::Up || ev == UiEvent::Right) {
		list->next_enabled(&MenuController::is_screen_enabled, this);
		return AppRequest::None;
	}
	if (ev == UiEvent::Down || ev == UiEvent::Left) {
		list->prev_enabled(&MenuController::is_screen_enabled, this);
		return AppRequest::None;
	}

	return AppRequest::None;
}

} /* namespace uflow::ui */
