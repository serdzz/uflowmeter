/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Text-based command shell sharing USART1 with Modbus. Layered on top
 * of the UART transport's existing per-byte msgq — the worker also
 * accumulates a line buffer (terminated on \r or \n) and routes
 * completed lines through process_line(). Unknown lines fall through
 * silently (the Modbus framer on the same byte stream has its own
 * boundary detection, so noise that isn't a shell command typically
 * still gets handled — or dropped as a CRC mismatch).
 *
 * Commands (matching rework/embassy:src/shell.rs verbatim):
 *   help                  — list commands
 *   date get              — print "use Modbus reg 0x0064" hint (RTC
 *                           formatting deferred)
 *   date set <unix_ts>    — set wall clock (calls datetime::set)
 *   zero                  — auto-zero calibration trigger (stub)
 *   calibrate <1-3> <lph> — calibration point (stub)
 *   set_serial <N>        — write Options.serial_number + save
 *   set_verbose <0|1>     — log-only (verbose flag not yet wired)
 *   get_settings          — TDC register dump pointer ("use Modbus")
 *   get_calibration       — calibration dump pointer ("use Modbus")
 *
 * Pure-logic: no Zephyr deps. The caller (UART worker) wires the
 * side-effect dispatch.
 */

#pragma once

#include <cstddef>
#include <cstdint>

namespace uflow::shell {

constexpr std::size_t REPLY_CAP = 256;
constexpr std::size_t MAX_LINE  = 80;

enum class ResultKind : std::uint8_t {
	Ok,                /* command recognized; reply text in `out` */
	NotAShellCommand,  /* first token unknown; caller should drop */
	Error,             /* parse error; reply text holds an error msg */
};

struct Result {
	ResultKind   kind;
	char         text[REPLY_CAP];
	std::size_t  text_len;
};

enum class ActionKind : std::uint8_t {
	None = 0,
	SetDateUnix,    /* value = seconds since 1970-01-01 */
	SetSerial,      /* value = u32 serial number */
	SetVerbose,     /* value = 0 or 1 */
};

struct Action {
	ActionKind   kind;
	std::uint32_t value;
};

/* Process a line of input (without trailing \r/\n). Returns a Result
 * with the reply text composed in `text` (text_len bytes). For
 * NotAShellCommand, text_len = 0 and caller should drop. */
Result process_line(const std::uint8_t* line, std::size_t len);

/* Re-parse the line for any side-effect the caller should perform.
 * Mirrors process_line but only emits actions for variants that have
 * effects beyond the text reply. Returns {None, 0} if no action. */
Action parse_action(const std::uint8_t* line, std::size_t len);

} /* namespace uflow::shell */
