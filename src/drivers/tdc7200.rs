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
    en: Output<'d>,
}

impl<'d, D: SpiDevice> Tdc7200<'d, D> {
    pub fn new(spi: D, en: Output<'d>) -> Self {
        Self { spi, en }
    }

    /// Power the chip on (EN HIGH). Caller must wait ~1 ms for the
    /// internal regulator to settle before issuing SPI commands.
    pub fn power_on(&mut self) {
        self.en.set_high();
    }

    /// Power the chip off (EN LOW). All register state is lost.
    pub fn power_off(&mut self) {
        self.en.set_low();
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

    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error> {
        let cmd = [(address & 0x1F) | 0x40, value];
        self.spi.write(&cmd)
    }

    /// Bulk-load a config blob — one byte per register starting at 0x00.
    /// Matches the legacy `Options::tdc7200_regs` (10 bytes).
    pub fn load_config(&mut self, regs: &[u8]) -> Result<(), D::Error> {
        for (i, b) in regs.iter().enumerate() {
            self.write_register(i as u8, *b)?;
        }
        Ok(())
    }

    /// Read a 24-bit big-endian time value (TIME1/CLOCK_COUNT1/etc.).
    pub fn read_u24(&mut self, address: u8) -> Result<u32, D::Error> {
        let cmd = [address & 0x1F];
        let mut rx = [0u8; 3];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut rx)])?;
        Ok(((rx[0] as u32) << 16) | ((rx[1] as u32) << 8) | (rx[2] as u32))
    }

    /// Kick off a single ToF measurement (sets CONFIG1.START_MEAS).
    /// INT pin (PB0) goes low on completion — wait via ExtiInput.
    pub fn start_measurement(&mut self) -> Result<(), D::Error> {
        // CONFIG1 (0x00): START_MEAS=1.
        self.write_register(0x00, 0x01)
    }

    /// Read TIME1 (0x10) — 24-bit raw ToF value.
    pub fn read_time1(&mut self) -> Result<u32, D::Error> {
        self.read_u24(0x10)
    }
}
