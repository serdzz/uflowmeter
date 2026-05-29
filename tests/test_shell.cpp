/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Shell parser tests.
 */

#include "framework.hpp"

#include "../src/shell.hpp"

#include <cstring>

using uflow::shell::Action;
using uflow::shell::ActionKind;
using uflow::shell::parse_action;
using uflow::shell::process_line;
using uflow::shell::Result;
using uflow::shell::ResultKind;

namespace {

bool starts_with(const Result& r, const char* prefix)
{
	const std::size_t n = std::strlen(prefix);
	return r.text_len >= n && std::memcmp(r.text, prefix, n) == 0;
}

bool contains(const Result& r, const char* needle)
{
	const std::size_t n = std::strlen(needle);
	if (r.text_len < n) return false;
	for (std::size_t i = 0; i + n <= r.text_len; i++) {
		if (std::memcmp(&r.text[i], needle, n) == 0) return true;
	}
	return false;
}

Result run(const char* s)
{
	return process_line(reinterpret_cast<const std::uint8_t*>(s), std::strlen(s));
}

Action act(const char* s)
{
	return parse_action(reinterpret_cast<const std::uint8_t*>(s), std::strlen(s));
}

} /* namespace */

TEST(help_lists_commands)
{
	const Result r = run("help");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(starts_with(r, "Commands:"));
	ASSERT_TRUE(contains(r, "set_serial"));
	ASSERT_TRUE(contains(r, "date set"));
}

TEST(unknown_command_dropped)
{
	const Result r = run("zorglub");
	ASSERT_TRUE(r.kind == ResultKind::NotAShellCommand);
	ASSERT_EQ(r.text_len, std::size_t{0});
}

TEST(empty_line_dropped)
{
	const Result r = run("");
	ASSERT_TRUE(r.kind == ResultKind::NotAShellCommand);
}

TEST(trim_trailing_newline)
{
	const Result r = run("help\r\n");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(starts_with(r, "Commands:"));
}

TEST(date_get)
{
	const Result r = run("date get");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "date get"));
}

TEST(date_set_valid)
{
	const Result r = run("date set 1735689600");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "1735689600"));
}

TEST(date_set_missing_arg)
{
	const Result r = run("date set");
	ASSERT_TRUE(r.kind == ResultKind::Error);
	ASSERT_TRUE(contains(r, "Usage"));
}

TEST(date_set_invalid_value)
{
	const Result r = run("date set abc");
	ASSERT_TRUE(r.kind == ResultKind::Error);
	ASSERT_TRUE(contains(r, "invalid"));
}

TEST(zero_no_args)
{
	const Result r = run("zero");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "Zero"));
}

TEST(zero_with_args_rejected)
{
	const Result r = run("zero extra");
	ASSERT_TRUE(r.kind == ResultKind::Error);
}

TEST(calibrate_valid)
{
	const Result r = run("calibrate 2 5000");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "K2"));
}

TEST(calibrate_bad_coef)
{
	const Result r = run("calibrate 7 1000");
	ASSERT_TRUE(r.kind == ResultKind::Error);
}

TEST(set_serial_valid)
{
	const Result r = run("set_serial 12345");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "12345"));
}

TEST(set_verbose_on)
{
	const Result r = run("set_verbose 1");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "enabled"));
}

TEST(set_verbose_off)
{
	const Result r = run("set_verbose 0");
	ASSERT_TRUE(r.kind == ResultKind::Ok);
	ASSERT_TRUE(contains(r, "disabled"));
}

TEST(set_verbose_bad_arg)
{
	const Result r = run("set_verbose 42");
	ASSERT_TRUE(r.kind == ResultKind::Error);
}

/* ─── parse_action side-effect extraction ─────────────────────── */

TEST(action_date_set_emits)
{
	const Action a = act("date set 1700000000");
	ASSERT_TRUE(a.kind == ActionKind::SetDateUnix);
	ASSERT_EQ(a.value, 1700000000u);
}

TEST(action_date_get_emits_none)
{
	const Action a = act("date get");
	ASSERT_TRUE(a.kind == ActionKind::None);
}

TEST(action_set_serial_emits)
{
	const Action a = act("set_serial 99");
	ASSERT_TRUE(a.kind == ActionKind::SetSerial);
	ASSERT_EQ(a.value, 99u);
}

TEST(action_set_verbose_emits)
{
	const Action a = act("set_verbose 1");
	ASSERT_TRUE(a.kind == ActionKind::SetVerbose);
	ASSERT_EQ(a.value, 1u);
}

TEST(action_help_emits_none)
{
	const Action a = act("help");
	ASSERT_TRUE(a.kind == ActionKind::None);
}

TEST(action_invalid_arg_emits_none)
{
	const Action a = act("set_serial xyz");
	ASSERT_TRUE(a.kind == ActionKind::None);
}
