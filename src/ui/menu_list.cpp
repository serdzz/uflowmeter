/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "menu_list.hpp"

namespace uflow::ui {

void MenuList::add(ScreenId screen)
{
	if (count_ >= CAPACITY) {
		return;
	}
	items_[count_++] = screen;
}

ScreenId MenuList::current() const
{
	if (count_ == 0) {
		return ScreenId::_Count;
	}
	return items_[index_];
}

void MenuList::next_enabled(IsEnabledFn is_enabled, void* ctx)
{
	if (count_ == 0) {
		return;
	}
	for (std::size_t step = 0; step < count_; step++) {
		std::size_t candidate = (index_ + step + 1) % count_;
		if (is_enabled == nullptr || is_enabled(items_[candidate], ctx)) {
			index_ = candidate;
			return;
		}
	}
	/* All disabled — leave index alone. */
}

void MenuList::prev_enabled(IsEnabledFn is_enabled, void* ctx)
{
	if (count_ == 0) {
		return;
	}
	for (std::size_t step = 0; step < count_; step++) {
		std::size_t candidate = (index_ + count_ - step - 1) % count_;
		if (is_enabled == nullptr || is_enabled(items_[candidate], ctx)) {
			index_ = candidate;
			return;
		}
	}
}

} /* namespace uflow::ui */
