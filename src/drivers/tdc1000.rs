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
}
