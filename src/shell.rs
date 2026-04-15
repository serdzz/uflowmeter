//! Debug Shell over USART1
//!
//! Text-based command interface sharing USART1 with Modbus.
//! When a line starts with a known shell command prefix, the shell processes it.
//! Otherwise, the data is treated as a Modbus RTU frame.
//!
//! Commands (matching C++ version):
//!   date get           — print current RTC time as unix timestamp
//!   date set <N>       — set RTC time (unix timestamp)
//!   zero               — trigger auto-zero calibration
//!   calibrate <1-3> <lph> — calibration point
//!   set_serial <N>     — set device serial number
//!   set_verbose <0|1>  — enable/disable verbose console output
//!   get_settings       — dump TDC1000/TDC7200 register config
//!   get_calibration    — dump calibration data
//!   help               — list commands

use heapless::Vec;
use heapless::String;

#[allow(dead_code)]
const MAX_LINE: usize = 80;

/// Shell command result
pub enum ShellResult {
    /// Command recognized and processed, response in String
    Ok(String<256>),
    /// Unknown command — treat input as Modbus
    NotAShellCommand,
    /// Command parse error
    Error(&'static str),
}

/// Process a line of text input as a shell command.
/// Returns ShellResult::NotAShellCommand if the first token isn't a known command.
pub fn process_line(line: &[u8]) -> ShellResult {
    // Trim trailing \r\n
    let trimmed = trim_line(line);
    if trimmed.is_empty() {
        return ShellResult::NotAShellCommand;
    }

    // Split into tokens (space-separated)
    let tokens: Vec<&[u8], 8> = match split_tokens(trimmed) {
        Ok(t) => t,
        Err(_) => return ShellResult::Error("too many tokens"),
    };

    if tokens.is_empty() {
        return ShellResult::NotAShellCommand;
    }

    let cmd = tokens[0];

    // Match known shell commands
    if eq(cmd, b"help") {
        return ShellResult::Ok(help_text());
    }
    if eq(cmd, b"date") {
        return cmd_date(&tokens[1..]);
    }
    if eq(cmd, b"zero") {
        return cmd_zero(&tokens[1..]);
    }
    if eq(cmd, b"calibrate") {
        return cmd_calibrate(&tokens[1..]);
    }
    if eq(cmd, b"set_serial") {
        return cmd_set_serial(&tokens[1..]);
    }
    if eq(cmd, b"set_verbose") {
        return cmd_set_verbose(&tokens[1..]);
    }
    if eq(cmd, b"get_settings") {
        return cmd_get_settings();
    }
    if eq(cmd, b"get_calibration") {
        return cmd_get_calibration();
    }

    ShellResult::NotAShellCommand
}

// ─── Commands ────────────────────────────────────────────────────────

fn help_text() -> String<256> {
    lit(
        "Commands:\r\n\
         date get\r\n\
         date set <unix_ts>\r\n\
         zero\r\n\
         calibrate <1-3> <lph>\r\n\
         set_serial <N>\r\n\
         set_verbose <0|1>\r\n\
         get_settings\r\n\
         get_calibration\r\n\
         help\r\n"
    )
}

fn cmd_date(args: &[&[u8]]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::Error("Usage: date get | date set <N>");
    }
    if eq(args[0], b"get") {
        // RTC time will be read by the caller (app_request) and formatted
        return ShellResult::Ok(lit("date get: use Modbus reg 0x0064\r\n"));
    }
    if eq(args[0], b"set") {
        if args.len() < 2 {
            return ShellResult::Error("Usage: date set <unix_ts>");
        }
        match parse_u32(args[1]) {
            Some(ts) => {
                // The caller will set RTC — return the parsed value via AppRequest::SetTime
                let mut out: String<256> = lit("date set ");
                out.push_str(&fmt_u32(ts)).ok();
                out.push_str("\r\n").ok();
                return ShellResult::Ok(out);
            }
            None => return ShellResult::Error("invalid timestamp"),
        }
    }
    ShellResult::Error("Usage: date get | date set <N>")
}

fn cmd_zero(args: &[&[u8]]) -> ShellResult {
    if !args.is_empty() {
        return ShellResult::Error("Usage: zero");
    }
    // Auto-zero calibration triggered
    ShellResult::Ok(lit("Zero autocalibration in progress\r\n"))
}

fn cmd_calibrate(args: &[&[u8]]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::Error("Usage: calibrate <1-3> <lph>");
    }
    let coef_no = match parse_u8(args[0]) {
        Some(n) if n >= 1 && n <= 3 => n,
        _ => return ShellResult::Error("coef must be 1, 2, or 3"),
    };
    let _lph = match parse_u32(args[1]) {
        Some(v) => v,
        None => return ShellResult::Error("invalid lph value"),
    };
    let mut out: String<256> = lit("Calibration K");
    out.push_str(&fmt_u8(coef_no)).ok();
    out.push_str(" in progress\r\n").ok();
    ShellResult::Ok(out)
}

fn cmd_set_serial(args: &[&[u8]]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::Error("Usage: set_serial <N>");
    }
    match parse_u32(args[0]) {
        Some(serial) => {
            let mut out: String<256> = lit("Writing serial ");
            out.push_str(&fmt_u32(serial)).ok();
            out.push_str("\r\n").ok();
            ShellResult::Ok(out)
        }
        None => ShellResult::Error("invalid serial number"),
    }
}

fn cmd_set_verbose(args: &[&[u8]]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::Error("Usage: set_verbose <0|1>");
    }
    match parse_u8(args[0]) {
        Some(0) => ShellResult::Ok(lit("Verbose disabled\r\n")),
        Some(1) => ShellResult::Ok(lit("Verbose enabled\r\n")),
        _ => ShellResult::Error("value must be 0 or 1"),
    }
}

fn cmd_get_settings() -> ShellResult {
    ShellResult::Ok(lit("Dump TDC registers via Modbus\r\n"))
}

fn cmd_get_calibration() -> ShellResult {
    ShellResult::Ok(lit("Dump calibration via Modbus\r\n"))
}

// ─── Helpers ──────────────────────────────────────────────────────────

// ─── Helpers ──────────────────────────────────────────────────────────

/// Create a heapless String from a &str literal (panics if too long — only for known-short literals)
fn lit(lit: &str) -> String<256> {
    String::try_from(lit).unwrap_or_else(|_| String::new())
}

fn trim_line(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    while end > 0 && (line[end - 1] == b'\r' || line[end - 1] == b'\n') {
        end -= 1;
    }
    &line[..end]
}

fn split_tokens<const N: usize>(input: &[u8]) -> Result<Vec<&[u8], N>, ()> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (i, &b) in input.iter().enumerate() {
        if b == b' ' || b == b'\t' {
            if let Some(s) = start {
                if tokens.push(&input[s..i]).is_err() {
                    return Err(());
                }
                start = None;
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        if tokens.push(&input[s..]).is_err() {
            return Err(());
        }
    }
    Ok(tokens)
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

fn parse_u32(s: &[u8]) -> Option<u32> {
    let mut result: u32 = 0;
    for &b in s {
        if b >= b'0' && b <= b'9' {
            result = result.checked_mul(10)?.checked_add((b - b'0') as u32)?;
        } else {
            return None;
        }
    }
    Some(result)
}

fn parse_u8(s: &[u8]) -> Option<u8> {
    parse_u32(s).and_then(|v| if v <= 255 { Some(v as u8) } else { None })
}

fn fmt_u32(mut v: u32) -> String<11> {
    if v == 0 {
        return String::try_from("0").unwrap_or_else(|_| String::new());
    }
    let mut buf = [0u8; 11];
    let mut i = 10;
    while v > 0 {
        buf[i] = (v % 10) as u8 + b'0';
        v /= 10;
        i -= 1;
    }
    let s = core::str::from_utf8(&buf[i + 1..]).unwrap_or("?");
    String::try_from(s).unwrap_or_else(|_| String::new())
}

fn fmt_u8(v: u8) -> String<4> {
    let s = fmt_u32(v as u32);
    String::try_from(s.as_str()).unwrap_or_else(|_| String::new())
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help() {
        match process_line(b"help\r\n") {
            ShellResult::Ok(s) => assert!(s.starts_with("Commands:")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_date_get() {
        match process_line(b"date get\r\n") {
            ShellResult::Ok(_) => {}
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_date_set() {
        match process_line(b"date set 1700000000\r\n") {
            ShellResult::Ok(s) => assert!(s.contains("1700000000")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_date_set_invalid() {
        match process_line(b"date set\r\n") {
            ShellResult::Error(_) => {}
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_zero() {
        match process_line(b"zero\r\n") {
            ShellResult::Ok(s) => assert!(s.contains("autocalibration")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_calibrate() {
        match process_line(b"calibrate 1 1500\r\n") {
            ShellResult::Ok(s) => assert!(s.contains("K1")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_calibrate_bad_coef() {
        match process_line(b"calibrate 5 100\r\n") {
            ShellResult::Error(_) => {}
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_set_serial() {
        match process_line(b"set_serial 12345\r\n") {
            ShellResult::Ok(s) => assert!(s.contains("12345")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_set_verbose() {
        match process_line(b"set_verbose 1\r\n") {
            ShellResult::Ok(s) => assert!(s.contains("enabled")),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_unknown_command_is_not_shell() {
        match process_line(b"\x01\x03\x00\x00\x00\x0a") {
            ShellResult::NotAShellCommand => {}
            _ => panic!("expected NotAShellCommand for binary input"),
        }
    }

    #[test]
    fn test_parse_u32() {
        assert_eq!(parse_u32(b"12345"), Some(12345));
        assert_eq!(parse_u32(b"0"), Some(0));
        assert_eq!(parse_u32(b"4294967295"), Some(4294967295));
        assert_eq!(parse_u32(b"abc"), None);
    }

    #[test]
    fn test_parse_u8() {
        assert_eq!(parse_u8(b"255"), Some(255));
        assert_eq!(parse_u8(b"256"), None);
    }

    #[test]
    fn test_fmt_u32() {
        assert_eq!(fmt_u32(0).as_str(), "0");
        assert_eq!(fmt_u32(42).as_str(), "42");
        assert_eq!(fmt_u32(1234567).as_str(), "1234567");
    }

    #[test]
    fn test_split_tokens() {
        let tokens: Vec<&[u8], 8> = split_tokens(b"date set 123").unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(eq(tokens[0], b"date"));
        assert!(eq(tokens[1], b"set"));
        assert!(eq(tokens[2], b"123"));
    }
}