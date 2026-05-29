/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "history.hpp"

namespace uflow::history {

namespace {
QueryResult g_last_result{};
}

std::optional<float> query(HistoryType /*type*/, std::uint32_t /*timestamp*/)
{
	/* Stub — rings not ported yet. Always returns "no data". The
	 * HistoryWidget UI still exercises every code path under this
	 * (date navigation, kind switch, render), and the swap to the
	 * real ring lookup is a single function replacement. */
	return std::nullopt;
}

const QueryResult& last_result()
{
	return g_last_result;
}

void set_last_result(const QueryResult& r)
{
	g_last_result = r;
}

} /* namespace uflow::history */
