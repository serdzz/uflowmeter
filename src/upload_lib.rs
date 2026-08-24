//! Decoding of the two configuration blobs the meter accepts over
//! XMODEM. Pure — no HAL, no transport — so the layout is testable.
//!
//! Both layouts come from the C++ shell commands
//! (`UFlowMeter_c++/UFlowMeter/Src/main.cpp:75-110`).

use crate::options::Options;

/// 14 little-endian f32: one `tCallibrationTable` per sensor pair.
pub const CALIBRATION_BLOCK: usize = 56;

/// 10 TDC1000 registers followed by 10 TDC7200 registers.
pub const TDC_BLOCK: usize = 20;

/// Which blob a transfer is carrying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadKind {
    Calibration,
    TdcRegs,
}

impl UploadKind {
    /// Payload size this kind of upload carries.
    pub fn block_len(&self) -> usize {
        match self {
            UploadKind::Calibration => CALIBRATION_BLOCK,
            UploadKind::TdcRegs => TDC_BLOCK,
        }
    }
}

fn f32_bits(block: &[u8], index: usize) -> u32 {
    let off = index * 4;
    u32::from_le_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]])
}

/// Apply a 56-byte calibration blob.
///
/// The C++ `tCallibrationTable` is `{ dTOF0, data[3] }` where each
/// entry is `{ V, K }` — so V and K alternate per point rather than
/// being grouped. Options stores them as separate fields, which is
/// exactly where a naive `memcpy`-shaped port would swap them.
pub fn apply_calibration_block(options: &mut Options, block: &[u8; CALIBRATION_BLOCK]) {
    options.set_zero1(f32_bits(block, 0));
    options.set_v11(f32_bits(block, 1));
    options.set_k11(f32_bits(block, 2));
    options.set_v12(f32_bits(block, 3));
    options.set_k12(f32_bits(block, 4));
    options.set_v13(f32_bits(block, 5));
    options.set_k13(f32_bits(block, 6));

    options.set_zero2(f32_bits(block, 7));
    options.set_v21(f32_bits(block, 8));
    options.set_k21(f32_bits(block, 9));
    options.set_v22(f32_bits(block, 10));
    options.set_k22(f32_bits(block, 11));
    options.set_v23(f32_bits(block, 12));
    options.set_k23(f32_bits(block, 13));
}

/// Apply a 20-byte TDC register blob: first ten bytes are TDC1000
/// registers 0x00..0x09, the rest TDC7200's.
pub fn apply_tdc_block(options: &mut Options, block: &[u8; TDC_BLOCK]) {
    let mut tdc1000 = [0u8; 16];
    tdc1000[..10].copy_from_slice(&block[..10]);
    options.set_tdc1000_regs(u128::from_le_bytes(tdc1000));

    let mut tdc7200 = [0u8; 16];
    tdc7200[..10].copy_from_slice(&block[10..20]);
    options.set_tdc7200_regs(u128::from_le_bytes(tdc7200));
}
