//! Pure TDC7200 result decoding — no HAL, no SPI, host-testable.
//!
//! Ported byte-for-byte from the C++ `TDC7200::get_tof`
//! (`UFlowMeter_c++/UFlowMeter/hardware/tdc7200.hpp:134`), including
//! its fixed-point rounding, so results stay bit-identical to the
//! firmware this board was calibrated against.
//!
//! The chip returns a contiguous block starting at register 0x10:
//!
//! ```text
//! offset  0.. 2  TIME1
//! offset  3.. 5  CLOCK_COUNT1
//! offset  6.. 8  TIME2
//! offset  9..11  CLOCK_COUNT2
//! ...            6 bytes per additional stop
//! offset 33..35  CALIBRATION1
//! offset 36..38  CALIBRATION2
//! ```
//!
//! All fields are 24-bit big-endian.

/// Bytes the TDC7200 result block occupies (TIME1 .. CALIBRATION2).
pub const RESULT_BLOCK_LEN: usize = 39;

/// The chip supports at most 5 stops.
pub const MAX_STOPS: usize = 5;

/// Read a 24-bit big-endian field at `off`.
fn be24(data: &[u8], off: usize) -> u32 {
    ((data[off] as u32) << 16) | ((data[off + 1] as u32) << 8) | (data[off + 2] as u32)
}

/// Decode per-stop time-of-flight values from a result block.
///
/// Returns `None` when `n_stops` is out of range or the block is
/// short — the same guard the C++ applies before touching the buffer.
///
/// The constants come from the calibrated configuration this meter
/// ships with (`CONFIG2 = 0x44`: CALIBRATION2_PERIODS = 10, so the
/// calibration count spans 9 clock periods, and an 8 MHz reference
/// gives a 125 000 ps period). `140625 = 1_125_000 / 8`; the `* 8`
/// steps below recover the fractional bits that integer division
/// would otherwise drop.
pub fn decode_tof(data: &[u8], n_stops: usize) -> Option<[i32; MAX_STOPS]> {
    if n_stops == 0 || n_stops > MAX_STOPS || data.len() < RESULT_BLOCK_LEN {
        return None;
    }

    let cal0 = be24(data, 33) as i32;
    let cal1 = be24(data, 36) as i32;
    let dn = cal1 - cal0;
    if dn == 0 {
        // Calibration never ran (both registers identical) — every
        // division below would trap. The C++ runs on a core where
        // this silently produced garbage; refuse instead.
        return None;
    }

    let time0 = be24(data, 0) as i32;
    let mut out = [0i32; MAX_STOPS];

    for c in 1..=n_stops {
        let time1 = be24(data, 6 * c) as i32;
        let clk = be24(data, 6 * (c - 1) + 3) as i32;

        let dmy = (time0 - time1).wrapping_mul(140_625);
        let s = dmy / dn;
        let r = dmy - s * dn;
        let half = dn / 2;
        let r = (r * 8 + half) / dn;
        let s = 8 * s + r;

        out[c - 1] = s + clk.wrapping_mul(125_000);
    }

    Some(out)
}

/// Mean of the first `n_stops` entries — the C++ `get_tof()` averages
/// the per-stop values before handing them to the flow calculator.
pub fn average_stops(tof: &[i32; MAX_STOPS], n_stops: usize) -> Option<i32> {
    if n_stops == 0 || n_stops > MAX_STOPS {
        return None;
    }
    let sum: i64 = tof[..n_stops].iter().map(|v| *v as i64).sum();
    Some((sum / n_stops as i64) as i32)
}

/// Number of stops the chip is configured for: `CONFIG2.NUM_STOP + 1`
/// (bits 2:0), per the C++ `stop_numbers()`.
pub fn stop_numbers(config2: u8) -> usize {
    (config2 & 0x07) as usize + 1
}
