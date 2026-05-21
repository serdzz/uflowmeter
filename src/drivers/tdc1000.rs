//! Minimal TDC1000 driver — analog ultrasonic frontend.
//!
//! First-pass skeleton: register read / write over an
//! `embedded_hal::spi::SpiDevice` plus EN (PB10) and RES (PC6) pins.
//! Full measurement configuration (channel selection, pulse counts,
//! TX frequency divider) lives in the legacy `src/hardware/tdc1000.rs`
//! and will be ported once we need real ToF numbers — for now this
//! verifies bus + CS wiring with a register read.

use embassy_stm32::gpio::Output;
use embedded_hal::spi::{Operation, SpiDevice};

/// TDC1000 SPI command format: bit 7 = R/W (1 = write, 0 = read),
/// bits 6:0 = register address.
const READ_BIT: u8 = 0x00;

pub struct Tdc1000<'d, D> {
    spi: D,
    _en: Output<'d>,
    _res: Output<'d>,
}

impl<'d, D: SpiDevice> Tdc1000<'d, D> {
    /// `en` is the chip enable (held HIGH). `res` is the (active-low)
    /// reset — held HIGH for normal operation.
    pub fn new(spi: D, en: Output<'d>, res: Output<'d>) -> Self {
        Self { spi, _en: en, _res: res }
    }

    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error> {
        let cmd = [(address & 0x7F) | READ_BIT];
        let mut rx = [0u8; 1];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut rx)])?;
        Ok(rx[0])
    }

    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error> {
        let cmd = [(address & 0x7F) | 0x80, value];
        self.spi.write(&cmd)
    }

    /// Bulk-load a config blob — one byte per register starting at 0x00.
    /// Matches the legacy `Options::tdc1000_regs` (10 bytes).
    pub fn load_config(&mut self, regs: &[u8]) -> Result<(), D::Error> {
        for (i, b) in regs.iter().enumerate() {
            self.write_register(i as u8, *b)?;
        }
        Ok(())
    }

    /// Select TDC1000 channel. Toggles bit 0 of CONFIG_2 (reg 0x02).
    pub fn set_channel(&mut self, ch2: bool) -> Result<(), D::Error> {
        let mut v = self.read_register(0x02)?;
        if ch2 {
            v |= 0x01;
        } else {
            v &= !0x01;
        }
        self.write_register(0x02, v)
    }

    /// Clear ERROR_FLAGS (reg 0x07).
    pub fn clear_error_flags(&mut self) -> Result<(), D::Error> {
        self.write_register(0x07, 0xFF)
    }
}
