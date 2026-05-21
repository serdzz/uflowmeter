//! Minimal TDC7200 driver — Time-to-Digital Converter (ToF measurement).
//!
//! First-pass skeleton — register read/write over a shared SPI bus
//! plus EN (PB1) pin. The full measurement orchestration (start /
//! calibration / multi-stop sampling) is in the legacy
//! `src/hardware/tdc7200.rs` and gets ported later.

use embassy_stm32::gpio::Output;
use embedded_hal::spi::{Operation, SpiDevice};

pub struct Tdc7200<'d, D> {
    spi: D,
    _en: Output<'d>,
}

impl<'d, D: SpiDevice> Tdc7200<'d, D> {
    pub fn new(spi: D, en: Output<'d>) -> Self {
        Self { spi, _en: en }
    }

    /// 8-bit register space. Command byte is `(addr & 0x1F) | 0x00`
    /// for reads (TDC7200: bit 6 = auto-increment, bit 7 = R/W with
    /// 1 = write).
    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error> {
        let cmd = [address & 0x1F];
        let mut rx = [0u8; 1];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut rx)])?;
        Ok(rx[0])
    }
}
