/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Shell command parser. See shell.hpp for the supported commands.
 * Mostly mechanical translation of rework/embassy:src/shell.rs —
 * tokenizer, parsers, and per-command response builders.
 */

#include "shell.hpp"

#include <cstring>
#include <cstdio>

namespace uflow::shell {

namespace {

/* Optional datetime provider (set by uart.cpp at boot). */
DateTimeProvider g_dt_provider = nullptr;

/* Maximum tokens accepted per line. Anything more is "too many
 * tokens" and produces a parse error. */
constexpr std::size_t MAX_TOKENS = 8;

struct Token {
	const std::uint8_t* data;
	std::size_t len;
};

std::size_t trim_trailing_nl(const std::uint8_t* in, std::size_t len)
{
	while (len > 0 && (in[len - 1] == '\r' || in[len - 1] == '\n')) {
		len--;
	}
	return len;
}

/* Split on spaces / tabs. Returns count, populates `out`. Returns
 * MAX_TOKENS + 1 on overflow (caller treats as "too many tokens"). */
std::size_t split_tokens(const std::uint8_t* in, std::size_t len,
                         Token (&out)[MAX_TOKENS])
{
	std::size_t count = 0;
	std::size_t start = 0;
	bool in_token = false;
	for (std::size_t i = 0; i < len; i++) {
		const bool sep = (in[i] == ' ' || in[i] == '\t');
		if (sep) {
			if (in_token) {
				if (count >= MAX_TOKENS) return MAX_TOKENS + 1;
				out[count++] = {in + start, i - start};
				in_token = false;
			}
		} else {
			if (!in_token) {
				start = i;
				in_token = true;
			}
		}
	}
	if (in_token) {
		if (count >= MAX_TOKENS) return MAX_TOKENS + 1;
		out[count++] = {in + start, len - start};
	}
	return count;
}

bool eq(const Token& t, const char* s)
{
	const std::size_t s_len = std::strlen(s);
	if (t.len != s_len) return false;
	return std::memcmp(t.data, s, s_len) == 0;
}

bool parse_u32(const Token& t, std::uint32_t& out)
{
	if (t.len == 0) return false;
	std::uint32_t v = 0;
	for (std::size_t i = 0; i < t.len; i++) {
		const std::uint8_t b = t.data[i];
		if (b < '0' || b > '9') return false;
		const std::uint32_t prev = v;
		v = v * 10u + static_cast<std::uint32_t>(b - '0');
		if (v < prev) return false;  /* overflow */
	}
	out = v;
	return true;
}

bool parse_u8(const Token& t, std::uint8_t& out)
{
	std::uint32_t v = 0;
	if (!parse_u32(t, v)) return false;
	if (v > 255) return false;
	out = static_cast<std::uint8_t>(v);
	return true;
}

Result ok_lit(const char* text)
{
	Result r{};
	r.kind = ResultKind::Ok;
	const std::size_t n = std::strlen(text);
	const std::size_t copy = (n < REPLY_CAP) ? n : REPLY_CAP;
	std::memcpy(r.text, text, copy);
	r.text_len = copy;
	return r;
}

Result err_lit(const char* text)
{
	Result r{};
	r.kind = ResultKind::Error;
	const std::size_t prefix_len = 5;
	std::memcpy(r.text, "ERR: ", prefix_len);
	std::size_t n = std::strlen(text);
	if (prefix_len + n > REPLY_CAP - 2) {
		n = REPLY_CAP - 2 - prefix_len;
	}
	std::memcpy(r.text + prefix_len, text, n);
	r.text[prefix_len + n + 0] = '\r';
	r.text[prefix_len + n + 1] = '\n';
	r.text_len = prefix_len + n + 2;
	return r;
}

Result drop()
{
	Result r{};
	r.kind = ResultKind::NotAShellCommand;
	r.text_len = 0;
	return r;
}

Result cmd_help()
{
	return ok_lit(
		"Commands:\r\n"
		"  help\r\n"
		"  date get\r\n"
		"  date set <unix_ts>\r\n"
		"  zero\r\n"
		"  calibrate <1-3> <lph>\r\n"
		"  set_serial <N>\r\n"
		"  set_verbose <0|1>\r\n"
		"  get_settings\r\n"
		"  get_calibration\r\n");
}

Result cmd_date(const Token* args, std::size_t argc)
{
	if (argc == 0) return err_lit("Usage: date get | date set <N>");
	if (eq(args[0], "get")) {
		Result r{};
		r.kind = ResultKind::Ok;
		if (g_dt_provider != nullptr) {
			/* Provider writes formatted timestamp; we append the
			 * trailing \r\n. Leave room for "\r\n\0" (3 bytes). */
			constexpr std::size_t TAIL = 3;
			const std::size_t n = g_dt_provider(r.text,
				REPLY_CAP - TAIL);
			r.text[n + 0] = '\r';
			r.text[n + 1] = '\n';
			r.text_len = n + 2;
		} else {
			/* Pre-provider fallback — kept so unit tests that
			 * don't install a provider still see a parseable
			 * reply. */
			return ok_lit("date get: provider not installed\r\n");
		}
		return r;
	}
	if (eq(args[0], "set")) {
		if (argc < 2) return err_lit("Usage: date set <unix_ts>");
		std::uint32_t ts = 0;
		if (!parse_u32(args[1], ts)) return err_lit("invalid timestamp");
		Result r{};
		r.kind = ResultKind::Ok;
		const int n = snprintf(r.text, REPLY_CAP, "date set %u\r\n",
			static_cast<unsigned>(ts));
		r.text_len = (n > 0) ? static_cast<std::size_t>(n) : 0;
		return r;
	}
	return err_lit("Usage: date get | date set <N>");
}

Result cmd_zero(std::size_t argc)
{
	if (argc != 0) return err_lit("Usage: zero");
	return ok_lit("Zero autocalibration in progress\r\n");
}

Result cmd_calibrate(const Token* args, std::size_t argc)
{
	if (argc < 2) return err_lit("Usage: calibrate <1-3> <lph>");
	std::uint8_t coef = 0;
	if (!parse_u8(args[0], coef) || coef < 1 || coef > 3) {
		return err_lit("coef must be 1, 2, or 3");
	}
	std::uint32_t lph = 0;
	if (!parse_u32(args[1], lph)) {
		return err_lit("invalid lph value");
	}
	Result r{};
	r.kind = ResultKind::Ok;
	const int n = snprintf(r.text, REPLY_CAP, "Calibration K%u in progress\r\n",
		static_cast<unsigned>(coef));
	r.text_len = (n > 0) ? static_cast<std::size_t>(n) : 0;
	return r;
}

Result cmd_set_serial(const Token* args, std::size_t argc)
{
	if (argc == 0) return err_lit("Usage: set_serial <N>");
	std::uint32_t s = 0;
	if (!parse_u32(args[0], s)) return err_lit("invalid serial number");
	Result r{};
	r.kind = ResultKind::Ok;
	const int n = snprintf(r.text, REPLY_CAP, "Writing serial %u\r\n",
		static_cast<unsigned>(s));
	r.text_len = (n > 0) ? static_cast<std::size_t>(n) : 0;
	return r;
}

Result cmd_set_verbose(const Token* args, std::size_t argc)
{
	if (argc == 0) return err_lit("Usage: set_verbose <0|1>");
	std::uint8_t v = 0;
	if (!parse_u8(args[0], v)) return err_lit("value must be 0 or 1");
	if (v == 0) return ok_lit("Verbose disabled\r\n");
	if (v == 1) return ok_lit("Verbose enabled\r\n");
	return err_lit("value must be 0 or 1");
}

} /* namespace */

void set_datetime_provider(DateTimeProvider fn)
{
	g_dt_provider = fn;
}

Result process_line(const std::uint8_t* line, std::size_t len)
{
	len = trim_trailing_nl(line, len);
	if (len == 0) return drop();

	Token tokens[MAX_TOKENS];
	const std::size_t count = split_tokens(line, len, tokens);
	if (count > MAX_TOKENS) return err_lit("too many tokens");
	if (count == 0) return drop();

	const Token& cmd = tokens[0];

	if (eq(cmd, "help"))            return cmd_help();
	if (eq(cmd, "date"))            return cmd_date(&tokens[1], count - 1);
	if (eq(cmd, "zero"))            return cmd_zero(count - 1);
	if (eq(cmd, "calibrate"))       return cmd_calibrate(&tokens[1], count - 1);
	if (eq(cmd, "set_serial"))      return cmd_set_serial(&tokens[1], count - 1);
	if (eq(cmd, "set_verbose"))     return cmd_set_verbose(&tokens[1], count - 1);
	if (eq(cmd, "get_settings"))    return ok_lit("Dump TDC registers via Modbus\r\n");
	if (eq(cmd, "get_calibration")) return ok_lit("Dump calibration via Modbus\r\n");

	return drop();
}

Action parse_action(const std::uint8_t* line, std::size_t len)
{
	Action a{};
	a.kind = ActionKind::None;

	len = trim_trailing_nl(line, len);
	if (len == 0) return a;

	Token tokens[MAX_TOKENS];
	const std::size_t count = split_tokens(line, len, tokens);
	if (count == 0 || count > MAX_TOKENS) return a;

	const Token& cmd = tokens[0];

	if (eq(cmd, "date") && count >= 3 && eq(tokens[1], "set")) {
		std::uint32_t ts = 0;
		if (parse_u32(tokens[2], ts)) {
			a.kind = ActionKind::SetDateUnix;
			a.value = ts;
		}
		return a;
	}
	if (eq(cmd, "set_serial") && count >= 2) {
		std::uint32_t s = 0;
		if (parse_u32(tokens[1], s)) {
			a.kind = ActionKind::SetSerial;
			a.value = s;
		}
		return a;
	}
	if (eq(cmd, "set_verbose") && count >= 2) {
		std::uint8_t v = 0;
		if (parse_u8(tokens[1], v) && (v == 0 || v == 1)) {
			a.kind = ActionKind::SetVerbose;
			a.value = v;
		}
		return a;
	}

	return a;
}

} /* namespace uflow::shell */
