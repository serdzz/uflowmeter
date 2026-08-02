//! Tests for the uploaded configuration blob layouts.

use crate::options::Options;
use crate::upload_lib::*;

/// Build a calibration blob from 14 floats in wire order.
fn calib(values: [f32; 14]) -> [u8; CALIBRATION_BLOCK] {
    let mut out = [0u8; CALIBRATION_BLOCK];
    for (i, v) in values.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

#[test]
fn calibration_lands_in_the_right_fields() {
    // Distinct values so any swapped pair shows up.
    let block = calib([
        1.0, // sensor 0 dTOF0
        2.0, 3.0, // V0, K0
        4.0, 5.0, // V1, K1
        6.0, 7.0, // V2, K2
        8.0, // sensor 1 dTOF0
        9.0, 10.0, 11.0, 12.0, 13.0, 14.0,
    ]);

    let mut o = Options::default();
    apply_calibration_block(&mut o, &block);

    assert_eq!(f32::from_bits(o.zero1()), 1.0);
    assert_eq!(f32::from_bits(o.v11()), 2.0);
    assert_eq!(f32::from_bits(o.k11()), 3.0);
    assert_eq!(f32::from_bits(o.v12()), 4.0);
    assert_eq!(f32::from_bits(o.k12()), 5.0);
    assert_eq!(f32::from_bits(o.v13()), 6.0);
    assert_eq!(f32::from_bits(o.k13()), 7.0);

    assert_eq!(f32::from_bits(o.zero2()), 8.0);
    assert_eq!(f32::from_bits(o.v21()), 9.0);
    assert_eq!(f32::from_bits(o.k21()), 10.0);
    assert_eq!(f32::from_bits(o.v22()), 11.0);
    assert_eq!(f32::from_bits(o.k22()), 12.0);
    assert_eq!(f32::from_bits(o.v23()), 13.0);
    assert_eq!(f32::from_bits(o.k23()), 14.0);
}

#[test]
fn calibration_v_and_k_are_interleaved_not_grouped() {
    // The C++ struct alternates V,K per point. If a port grouped all
    // Vs then all Ks, this blob would put 0.5 into v12 instead of k11.
    let mut values = [0.0f32; 14];
    values[2] = 0.5; // third float = K of the first point
    let mut o = Options::default();
    apply_calibration_block(&mut o, &calib(values));

    assert_eq!(f32::from_bits(o.k11()), 0.5);
    assert_eq!(f32::from_bits(o.v12()), 0.0);
}

#[test]
fn calibration_round_trips_realistic_values() {
    let values = [
        -1.25e-3, 0.05, 1.001, 0.5, 0.998, 5.0, 1.002, 2.5e-4, 0.06, 0.999, 0.6, 1.0, 6.0, 1.003,
    ];
    let mut o = Options::default();
    apply_calibration_block(&mut o, &calib(values));
    assert_eq!(f32::from_bits(o.zero1()), values[0]);
    assert_eq!(f32::from_bits(o.k23()), values[13]);
}

#[test]
fn tdc_block_splits_ten_and_ten() {
    let mut block = [0u8; TDC_BLOCK];
    for (i, b) in block.iter_mut().enumerate() {
        *b = i as u8;
    }
    let mut o = Options::default();
    apply_tdc_block(&mut o, &block);

    let a = o.tdc1000_regs().to_le_bytes();
    let b = o.tdc7200_regs().to_le_bytes();
    assert_eq!(&a[..10], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(&b[..10], &[10, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
}

#[test]
fn tdc_block_accepts_the_shipped_defaults() {
    // The C++ def_regs, which is what a fresh meter gets uploaded.
    let block: [u8; TDC_BLOCK] = [
        0x48, 0x45, 0x01, 0x01, 0x07, 0xA0, 0x1E, 0x00, 0x6A, 0x03, 0x02, 0x44, 0x06, 0x07, 0xFF,
        0xFF, 0xFF, 0xFF, 0x00, 0x00,
    ];
    let mut o = Options::default();
    apply_tdc_block(&mut o, &block);
    assert_eq!(o.tdc1000_regs().to_le_bytes()[0], 0x48);
    assert_eq!(o.tdc7200_regs().to_le_bytes()[1], 0x44);
}

#[test]
fn tdc_block_leaves_no_stale_high_bytes() {
    // Options stores each blob in a B80 slot fed from a u128; the six
    // unused bytes must be zero, not whatever was there before.
    let mut o = Options::default();
    apply_tdc_block(&mut o, &[0xFF; TDC_BLOCK]);
    apply_tdc_block(&mut o, &[0x00; TDC_BLOCK]);
    assert_eq!(o.tdc1000_regs(), 0);
    assert_eq!(o.tdc7200_regs(), 0);
}

#[test]
fn upload_kind_lengths_match_the_blocks() {
    assert_eq!(UploadKind::Calibration.block_len(), 56);
    assert_eq!(UploadKind::TdcRegs.block_len(), 20);
}
