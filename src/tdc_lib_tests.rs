//! Tests for the pure TDC7200 result decoding.
//!
//! Expected values are computed by hand from the same integer
//! arithmetic the C++ `get_tof` performs, so a regression here means
//! we have drifted from the firmware the meter was calibrated with.

use crate::tdc_lib::*;

/// Build a result block: `times[i]` / `clocks[i]` are the TIMEn+1 and
/// CLOCK_COUNTn fields, `cal0` / `cal1` the two calibration counters.
fn block(time0: u32, times: &[u32], clocks: &[u32], cal0: u32, cal1: u32) -> [u8; 39] {
    let mut b = [0u8; 39];
    let put = |b: &mut [u8; 39], off: usize, v: u32| {
        b[off] = (v >> 16) as u8;
        b[off + 1] = (v >> 8) as u8;
        b[off + 2] = v as u8;
    };
    put(&mut b, 0, time0);
    for (i, t) in times.iter().enumerate() {
        put(&mut b, 6 * (i + 1), *t);
    }
    for (i, c) in clocks.iter().enumerate() {
        put(&mut b, 6 * i + 3, *c);
    }
    put(&mut b, 33, cal0);
    put(&mut b, 36, cal1);
    b
}

#[test]
fn stop_numbers_reads_low_three_bits() {
    // CONFIG2 = 0x44 is this meter's shipped value: NUM_STOP = 4.
    assert_eq!(stop_numbers(0x44), 5);
    assert_eq!(stop_numbers(0x00), 1);
    assert_eq!(stop_numbers(0x07), 8); // encoding allows it; chip caps at 5
                                       // Bits above 2:0 must not leak in.
    assert_eq!(stop_numbers(0xF8), 1);
}

#[test]
fn decode_single_stop_matches_reference_arithmetic() {
    // dn = cal1 - cal0 = 9000, one stop.
    let b = block(2000, &[1000], &[3], 1000, 10_000);
    let tof = decode_tof(&b, 1).expect("decodes");

    // dmy = (2000 - 1000) * 140625 = 140_625_000
    // s = 140_625_000 / 9000 = 15625, r = 0
    // r = (0 * 8 + 4500) / 9000 = 0
    // s = 8 * 15625 + 0 = 125_000
    // + clk * 125_000 = 3 * 125_000 = 375_000
    assert_eq!(tof[0], 125_000 + 375_000);
}

#[test]
fn decode_uses_per_stop_time_and_clock_fields() {
    // Two stops with distinct TIME/CLOCK so a mixed-up offset shows up.
    let b = block(5000, &[4000, 3000], &[1, 2], 0, 9000);
    let tof = decode_tof(&b, 2).expect("decodes");

    // stop 1: dmy = 1000 * 140625 = 140_625_000 -> s = 125_000
    //         clk = 1 -> + 125_000
    assert_eq!(tof[0], 250_000);
    // stop 2: dmy = 2000 * 140625 = 281_250_000 -> s = 250_000
    //         clk = 2 -> + 250_000
    assert_eq!(tof[1], 500_000);
    // Untouched slots stay zero.
    assert_eq!(tof[2..], [0, 0, 0]);
}

#[test]
fn decode_rounds_the_fractional_remainder() {
    // dn = 7 forces a non-zero remainder through the *8 refinement.
    let b = block(1, &[0], &[0], 0, 7);
    let tof = decode_tof(&b, 1).expect("decodes");

    // dmy = 1 * 140625; s = 140625 / 7 = 20089, r = 140625 - 140623 = 2
    // r = (2 * 8 + 3) / 7 = 19 / 7 = 2
    // s = 8 * 20089 + 2 = 160714
    assert_eq!(tof[0], 160_714);
}

#[test]
fn decode_rejects_bad_inputs() {
    let b = block(2000, &[1000], &[3], 1000, 10_000);
    assert!(decode_tof(&b, 0).is_none(), "zero stops");
    assert!(decode_tof(&b, MAX_STOPS + 1).is_none(), "too many stops");
    assert!(decode_tof(&b[..38], 1).is_none(), "short block");
}

#[test]
fn decode_rejects_uncalibrated_block() {
    // cal0 == cal1 means calibration never ran; dividing by dn would
    // panic on a division by zero.
    let b = block(2000, &[1000], &[3], 4242, 4242);
    assert!(decode_tof(&b, 1).is_none());
}

#[test]
fn average_stops_takes_the_mean_of_the_active_slots() {
    let tof = [10, 20, 30, 999, 999];
    assert_eq!(average_stops(&tof, 3), Some(20));
    // Trailing garbage must not be averaged in.
    assert_eq!(average_stops(&tof, 2), Some(15));
    assert_eq!(average_stops(&tof, 0), None);
    assert_eq!(average_stops(&tof, MAX_STOPS + 1), None);
}

#[test]
fn average_stops_does_not_overflow_on_large_values() {
    // Five near-i32::MAX stops would wrap a 32-bit accumulator.
    let tof = [i32::MAX; MAX_STOPS];
    assert_eq!(average_stops(&tof, MAX_STOPS), Some(i32::MAX));
}
