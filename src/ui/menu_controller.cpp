/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "menu_controller.hpp"

#include "../datetime.hpp"
#include "../options.hpp"

namespace uflow::ui {

namespace {

constexpr std::uint8_t COMM_TYPE_COUNT   = 4;   /* Off / M-BUS / ModBus / 4-20mA */
constexpr std::uint8_t SENSOR_TYPE_COUNT = 5;   /* DN40 / DN50 / DN65 / DN80 / DN100 */
constexpr std::uint8_t SLAVE_ADDR_MIN    = 1;
constexpr std::uint8_t SLAVE_ADDR_MAX    = 250;

} /* namespace */

MenuController::MenuController()
{
	/* Main menu — 14 screens in display order. */
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

	/* Edit-state bounds. Values get refreshed from g_options on
	 * each enter-edit transition; these are the rails. */
	comm_type_.max     = COMM_TYPE_COUNT;
	sensor_type_.max   = SENSOR_TYPE_COUNT;
	muster_.max        = 2;   /* 0 / 1 */
	negative_.max      = 2;

	slave_address_.min = SLAVE_ADDR_MIN;
	slave_address_.max = SLAVE_ADDR_MAX;
	slave_address_.step = 1;
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
	if (s == ScreenId::SlaveAddress) {
		return options::g_options.comm_type != 0;
	}
	return true;
}

EditBoxState* MenuController::editbox_for(ScreenId s)
{
	switch (s) {
	case ScreenId::CommType:    return &comm_type_;
	case ScreenId::Muster:      return &muster_;
	case ScreenId::Negative:    return &negative_;
	case ScreenId::SensorType:  return &sensor_type_;
	default:                    return nullptr;
	}
}

EditNumberState* MenuController::editnumber_for(ScreenId s)
{
	switch (s) {
	case ScreenId::SlaveAddress: return &slave_address_;
	default:                     return nullptr;
	}
}

void MenuController::load_from_options(ScreenId s)
{
	const auto& opts = options::g_options;
	switch (s) {
	case ScreenId::CommType:
		comm_type_.cursor = (opts.comm_type < COMM_TYPE_COUNT)
			? opts.comm_type : 0;
		break;
	case ScreenId::SlaveAddress: {
		std::uint8_t v = opts.slave_address;
		if (v < SLAVE_ADDR_MIN) v = SLAVE_ADDR_MIN;
		if (v > SLAVE_ADDR_MAX) v = SLAVE_ADDR_MAX;
		slave_address_.value = v;
		break;
	}
	case ScreenId::Negative:
		negative_.cursor = (opts.enable_negative != 0) ? 1 : 0;
		break;
	case ScreenId::SensorType:
		sensor_type_.cursor = (opts.sensor_type < SENSOR_TYPE_COUNT)
			? opts.sensor_type : 0;
		break;
	case ScreenId::Muster:
		/* No Options home — cursor keeps its previous value across
		 * edit sessions (still resets on reboot). */
		break;
	default:
		break;
	}
}

AppRequest MenuController::commit_request_for(ScreenId s) const
{
	switch (s) {
	case ScreenId::CommType:     return AppRequest::SetCommType;
	case ScreenId::SlaveAddress: return AppRequest::SetSlaveAddress;
	case ScreenId::Muster:       return AppRequest::SetMuster;
	case ScreenId::Negative:     return AppRequest::SetNegative;
	case ScreenId::SensorType:   return AppRequest::SetSensorType;
	default:                     return AppRequest::None;
	}
}

bool MenuController::is_editing_current() const
{
	const auto s = current_screen();
	switch (s) {
	case ScreenId::CommType:     return comm_type_.editable;
	case ScreenId::SlaveAddress: return slave_address_.editable;
	case ScreenId::Muster:       return muster_.editable;
	case ScreenId::Negative:     return negative_.editable;
	case ScreenId::SensorType:   return sensor_type_.editable;
	default:                     return false;
	}
}

std::uint8_t MenuController::edit_cursor_for_current() const
{
	const auto s = current_screen();
	switch (s) {
	case ScreenId::CommType:     return comm_type_.cursor;
	case ScreenId::SlaveAddress: return slave_address_.value;
	case ScreenId::Muster:       return muster_.cursor;
	case ScreenId::Negative:     return negative_.cursor;
	case ScreenId::SensorType:   return sensor_type_.cursor;
	default:                     return 0;
	}
}

AppRequest MenuController::editbox_event(EditBoxState& st, ScreenId s, UiEvent ev)
{
	switch (ev) {
	case UiEvent::Enter:
		if (!st.editable) {
			load_from_options(s);
			st.editable = true;
			return AppRequest::None;
		}
		st.editable = false;
		last_edit_value_ = st.cursor;
		return commit_request_for(s);

	case UiEvent::Back:
		/* Cancel edit without committing. */
		st.editable = false;
		return AppRequest::None;

	case UiEvent::Left:
	case UiEvent::Down:
		if (st.max == 0) return AppRequest::None;
		st.cursor = static_cast<std::uint8_t>((st.cursor + st.max - 1) % st.max);
		return AppRequest::None;

	case UiEvent::Right:
	case UiEvent::Up:
		if (st.max == 0) return AppRequest::None;
		st.cursor = static_cast<std::uint8_t>((st.cursor + 1) % st.max);
		return AppRequest::None;
	}
	return AppRequest::None;
}

AppRequest MenuController::editnumber_event(EditNumberState& st, ScreenId s, UiEvent ev)
{
	switch (ev) {
	case UiEvent::Enter:
		if (!st.editable) {
			load_from_options(s);
			st.editable = true;
			return AppRequest::None;
		}
		st.editable = false;
		last_edit_value_ = st.value;
		return commit_request_for(s);

	case UiEvent::Back:
		st.editable = false;
		return AppRequest::None;

	case UiEvent::Up:
	case UiEvent::Right:
		if (st.value <= static_cast<std::uint8_t>(st.max - st.step)) {
			st.value = static_cast<std::uint8_t>(st.value + st.step);
		} else {
			st.value = st.max;
		}
		return AppRequest::None;

	case UiEvent::Down:
	case UiEvent::Left:
		if (st.value >= static_cast<std::uint8_t>(st.min + st.step)) {
			st.value = static_cast<std::uint8_t>(st.value - st.step);
		} else {
			st.value = st.min;
		}
		return AppRequest::None;
	}
	return AppRequest::None;
}

void MenuController::datetime_advance_field()
{
	switch (datetime_field_) {
	case DateTimeEditField::None:    datetime_field_ = DateTimeEditField::Seconds; break;
	case DateTimeEditField::Seconds: datetime_field_ = DateTimeEditField::Minutes; break;
	case DateTimeEditField::Minutes: datetime_field_ = DateTimeEditField::Hours;   break;
	case DateTimeEditField::Hours:   datetime_field_ = DateTimeEditField::Day;     break;
	case DateTimeEditField::Day:     datetime_field_ = DateTimeEditField::Month;   break;
	case DateTimeEditField::Month:   datetime_field_ = DateTimeEditField::Year;    break;
	case DateTimeEditField::Year:    datetime_field_ = DateTimeEditField::None;    break;
	}
}

AppRequest MenuController::datetime_event(UiEvent ev)
{
	auto inc = [&]() {
		switch (datetime_field_) {
		case DateTimeEditField::Seconds: datetime::inc_second(edited_datetime_); break;
		case DateTimeEditField::Minutes: datetime::inc_minute(edited_datetime_); break;
		case DateTimeEditField::Hours:   datetime::inc_hour(edited_datetime_);   break;
		case DateTimeEditField::Day:     datetime::inc_day(edited_datetime_);    break;
		case DateTimeEditField::Month:   datetime::inc_month(edited_datetime_);  break;
		case DateTimeEditField::Year:    datetime::inc_year(edited_datetime_);   break;
		case DateTimeEditField::None:    break;
		}
	};
	auto dec = [&]() {
		switch (datetime_field_) {
		case DateTimeEditField::Seconds: datetime::dec_second(edited_datetime_); break;
		case DateTimeEditField::Minutes: datetime::dec_minute(edited_datetime_); break;
		case DateTimeEditField::Hours:   datetime::dec_hour(edited_datetime_);   break;
		case DateTimeEditField::Day:     datetime::dec_day(edited_datetime_);    break;
		case DateTimeEditField::Month:   datetime::dec_month(edited_datetime_);  break;
		case DateTimeEditField::Year:    datetime::dec_year(edited_datetime_);   break;
		case DateTimeEditField::None:    break;
		}
	};

	switch (ev) {
	case UiEvent::Enter:
		datetime_advance_field();
		if (datetime_field_ == DateTimeEditField::None) {
			/* Just completed Year → exit. Commit. */
			last_committed_datetime_ = edited_datetime_;
			return AppRequest::SetDateTime;
		}
		return AppRequest::None;
	case UiEvent::Back:
		datetime_field_ = DateTimeEditField::None;
		return AppRequest::None;
	case UiEvent::Up:
	case UiEvent::Right:
		inc();
		return AppRequest::None;
	case UiEvent::Down:
	case UiEvent::Left:
		dec();
		return AppRequest::None;
	}
	return AppRequest::None;
}

AppRequest MenuController::event(UiEvent ev)
{
	/* Edit mode swallows everything for the current screen first.
	 * Back during edit cancels (handled inside editbox/editnumber/
	 * datetime helpers); Back outside edit deselects the menu. */
	const ScreenId screen_before = current_screen();
	if (screen_before == ScreenId::DateTime &&
	    datetime_field_ != DateTimeEditField::None) {
		return datetime_event(ev);
	}
	if (auto* box = editbox_for(screen_before)) {
		if (box->editable) {
			return editbox_event(*box, screen_before, ev);
		}
	}
	if (auto* num = editnumber_for(screen_before)) {
		if (num->editable) {
			return editnumber_event(*num, screen_before, ev);
		}
	}

	if (ev == UiEvent::Back) {
		deselect();
		return AppRequest::DeepSleep;
	}

	if (active_ == MenuId::None) {
		select(MenuId::Main);
		return AppRequest::None;
	}

	auto* list = current_list();
	if (list == nullptr) {
		return AppRequest::None;
	}

	const ScreenId screen = list->current();

	if (ev == UiEvent::Enter) {
		switch (screen) {
		case ScreenId::Uptime:
			select(MenuId::User);
			return AppRequest::None;
		case ScreenId::Bootloader:
			return AppRequest::SystemReset;
		case ScreenId::Calibration:
			return AppRequest::EnterCalibration;
		case ScreenId::CommType:
		case ScreenId::Muster:
		case ScreenId::Negative:
		case ScreenId::SensorType: {
			auto* box = editbox_for(screen);
			return box ? editbox_event(*box, screen, ev) : AppRequest::None;
		}
		case ScreenId::SlaveAddress: {
			auto* num = editnumber_for(screen);
			return num ? editnumber_event(*num, screen, ev) : AppRequest::None;
		}
		case ScreenId::DateTime:
			/* Enter from non-edit on DateTime: capture live datetime
			 * into the working buffer + step into Seconds. */
			edited_datetime_ = datetime::now();
			datetime_advance_field();
			return AppRequest::None;
		default:
			return AppRequest::None;
		}
	}

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
