//! 25LC1024 SPI EEPROM driver (128 KB, 256-byte pages).
//!
//! Takes an `embedded_hal::spi::SpiDevice` so it shares the SPI2 bus
//! with TDC1000 / TDC7200 via `embassy_embedded_hal::shared_bus`.
//! HOLD (PC11) and WP (PC12) must be held HIGH externally — see
//! `src/main.rs` for the pin setup.

use embedded_hal::spi::{Operation, SpiDevice};

const CMD_READ: u8 = 0x03;
const CMD_WRITE: u8 = 0x02;
const CMD_WREN: u8 = 0x06;
const CMD_RDSR: u8 = 0x05;
const STATUS_WIP: u8 = 0x01;

pub struct Eeprom25Lc1024<D> {
    spi: D,
}

impl<D: SpiDevice> Eeprom25Lc1024<D> {
    pub fn new(spi: D) -> Self {
        Self { spi }
    }

    /// Read `buf.len()` bytes starting at `addr`. 24-bit address.
    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), D::Error> {
        let cmd = [
            CMD_READ,
            ((addr >> 16) & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            (addr & 0xFF) as u8,
        ];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(buf)])
    }

    /// Write up to a single 256-byte page. Caller is responsible for
    /// not crossing a page boundary.
    pub fn write_page(&mut self, addr: u32, data: &[u8]) -> Result<(), D::Error> {
        self.spi.write(&[CMD_WREN])?;
        let cmd = [
            CMD_WRITE,
            ((addr >> 16) & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            (addr & 0xFF) as u8,
        ];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Write(data)])?;
        self.wait_idle()
    }

    fn wait_idle(&mut self) -> Result<(), D::Error> {
        for _ in 0..10_000 {
            let mut sr = [0u8; 1];
            self.spi
                .transaction(&mut [Operation::Write(&[CMD_RDSR]), Operation::Read(&mut sr)])?;
            if sr[0] & STATUS_WIP == 0 {
                return Ok(());
            }
        }
        Ok(())
    }
}
