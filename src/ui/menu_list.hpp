/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ring buffer of ScreenIds. Constant-size (16-slot upper bound covers
 * the biggest menu — Main has 14 entries — without dynamic allocation).
 * Wraps modulo count. O(1) navigation.
 *
 * Source: git show rework/embassy:src/ui.rs MenuList.
 */

#pragma once

#include <cstddef>
#include <cstdint>

#include "screen.hpp"

namespace uflow::ui {

class MenuList {
public:
	static constexpr std::size_t CAPACITY = 16;

	/* Predicate: should this screen be visible during navigation?
	 * Implemented by the controller (e.g. SlaveAddress is hidden
	 * when CommType=Off). Function pointer keeps this header free of
	 * std::function overhead. */
	using IsEnabledFn = bool (*)(ScreenId, void* ctx);

	/* Push a screen into the ring (in declaration order). Silently
	 * caps at CAPACITY — debug check elsewhere if you care. */
	void add(ScreenId screen);

	/* Current focused screen. Returns ScreenId::_Count if the ring
	 * is empty (caller should defensively check). */
	ScreenId current() const;

	/* Advance / retreat the cursor to the next enabled screen,
	 * skipping disabled entries. No-op if no screen is enabled
	 * (avoids infinite loop). */
	void next_enabled(IsEnabledFn is_enabled, void* ctx);
	void prev_enabled(IsEnabledFn is_enabled, void* ctx);

	/* Reset cursor to the first entry. */
	void reset() { index_ = 0; }

	std::size_t count() const { return count_; }

private:
	ScreenId    items_[CAPACITY]{};
	std::size_t count_{0};
	std::size_t index_{0};
};

} /* namespace uflow::ui */
