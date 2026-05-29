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

history::HistoryType MenuController::kind_for_screen(ScreenId s)
{
	switch (s) {
	case ScreenId::HourHistory:  return history::HistoryType::Hour;
	case ScreenId::DayHistory:   return history::HistoryType::Day;
	case ScreenId::MonthHistory: return history::HistoryType::Month;
	default:                     return history::HistoryType::Hour;
	}
}

void MenuController::history_advance_field(history::HistoryType kind)
{
	switch (history_field_) {
	case HistoryEditField::None:
		/* Kind-specific entry point — Hour starts at Hour, Day at
		 * Day, Month at Month. Mirrors embassy's per-kind
		 * next_item() in the None branch. */
		switch (kind) {
		case history::HistoryType::Hour:  history_field_ = HistoryEditField::Hour; break;
		case history::HistoryType::Day:   history_field_ = HistoryEditField::Day; break;
		case history::HistoryType::Month: history_field_ = HistoryEditField::Month; break;
		}
		break;
	case HistoryEditField::Hour:  history_field_ = HistoryEditField::Day; break;
	case HistoryEditField::Day:   history_field_ = HistoryEditField::Month; break;
	case HistoryEditField::Month: history_field_ = HistoryEditField::Year; break;
	case HistoryEditField::Year:  history_field_ = HistoryEditField::None; break;
	}
}

AppRequest MenuController::history_emit_set(history::HistoryType kind)
{
	last_history_kind_ = kind;
	last_history_timestamp_ = datetime::to_timestamp(edited_history_datetime_);
	return AppRequest::SetHistory;
}

AppRequest MenuController::history_event(UiEvent ev, ScreenId screen)
{
	const auto kind = kind_for_screen(screen);
	auto inc = [&]() {
		switch (history_field_) {
		case HistoryEditField::Hour:  datetime::inc_hour(edited_history_datetime_); break;
		case HistoryEditField::Day:   datetime::inc_day(edited_history_datetime_);  break;
		case HistoryEditField::Month: datetime::inc_month(edited_history_datetime_); break;
		case HistoryEditField::Year:  datetime::inc_year(edited_history_datetime_); break;
		case HistoryEditField::None:  break;
		}
	};
	auto dec = [&]() {
		switch (history_field_) {
		case HistoryEditField::Hour:  datetime::dec_hour(edited_history_datetime_); break;
		case HistoryEditField::Day:   datetime::dec_day(edited_history_datetime_);  break;
		case HistoryEditField::Month: datetime::dec_month(edited_history_datetime_); break;
		case HistoryEditField::Year:  datetime::dec_year(edited_history_datetime_); break;
		case HistoryEditField::None:  break;
		}
	};

	switch (ev) {
	case UiEvent::Enter:
		history_advance_field(kind);
		/* No commit on Year → None — SetHistory has fired on every
		 * Left/Right press during edit, so backing data is already
		 * up to date. Exit edit silently. */
		return AppRequest::None;
	case UiEvent::Back:
		history_field_ = HistoryEditField::None;
		return AppRequest::None;
	case UiEvent::Up:
	case UiEvent::Right:
		inc();
		return history_emit_set(kind);
	case UiEvent::Down:
	case UiEvent::Left:
		dec();
		return history_emit_set(kind);
	}
	return AppRequest::None;
}

AppRequest MenuController::version_event(UiEvent ev)
{
	/* Pattern: Enter (3x) → Up (2x) → Down (2x). On full match, emit
	 * EnterCalibration; on wrong key, reset and let the event fall
	 * through to normal navigation. Source: rework/embassy:src/ui.rs
	 * version_key_event (with the "Process" return on partial matches
	 * dropped — we just continue dispatch). */
	static constexpr UiEvent PATTERN[7] = {
		UiEvent::Enter, UiEvent::Enter, UiEvent::Enter,
		UiEvent::Up, UiEvent::Up,
		UiEvent::Down, UiEvent::Down,
	};
	const std::uint8_t idx = pattern_matched_;
	if (idx < 7 && ev == PATTERN[idx]) {
		pattern_matched_++;
		if (pattern_matched_ == 7) {
			pattern_matched_ = 0;
			return AppRequest::EnterCalibration;
		}
		/* Consume — don't fall through to menu navigation while the
		 * pattern's in flight, otherwise Up/Down would walk away
		 * from Version screen. */
		return AppRequest::None;
	}
	pattern_matched_ = 0;
	return AppRequest::None;
}

AppRequest MenuController::event(UiEvent ev)
{
	/* Reset pattern matcher on screen change — pattern only counts
	 * keys delivered to a single Version-screen session. */
	const ScreenId current = current_screen();
	if (current != last_screen_for_pattern_) {
		pattern_matched_ = 0;
		last_screen_for_pattern_ = current;
	}

	/* Edit mode swallows everything for the current screen first.
	 * Back during edit cancels (handled inside editbox/editnumber/
	 * datetime/history helpers); Back outside edit deselects. */
	const ScreenId screen_before = current_screen();
	if (screen_before == ScreenId::DateTime &&
	    datetime_field_ != DateTimeEditField::None) {
		return datetime_event(ev);
	}
	if ((screen_before == ScreenId::HourHistory ||
	     screen_before == ScreenId::DayHistory ||
	     screen_before == ScreenId::MonthHistory) &&
	    history_field_ != HistoryEditField::None) {
		return history_event(ev, screen_before);
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

	/* Version pattern matcher runs ahead of regular Enter dispatch
	 * so partial matches don't leak into screen navigation. */
	if (screen == ScreenId::Version) {
		auto req = version_event(ev);
		if (req != AppRequest::None) {
			return req;
		}
		/* For Up/Down on Version that DIDN'T advance the pattern
		 * (pattern got reset), fall through to normal nav. */
	}

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
		case ScreenId::HourHistory:
		case ScreenId::DayHistory:
		case ScreenId::MonthHistory: {
			/* Enter from non-edit on a History screen: capture
			 * current wall clock as the starting date, step into
			 * the kind-specific first field, immediately fire a
			 * SetHistory so the backend can populate the value. */
			edited_history_datetime_ = datetime::now();
			const auto kind = kind_for_screen(screen);
			history_advance_field(kind);
			return history_emit_set(kind);
		}
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
