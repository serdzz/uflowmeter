//! 25LC1024 SPI EEPROM driver (128 KB, 256-byte pages).
//!
//! Takes an `embedded_hal::spi::SpiDevice` so it shares the SPI2 bus
//! with TDC1000 / TDC7200 via `embassy_embedded_hal::shared_bus`.
//! HOLD (PC11) and WP (PC12) must be held HIGH externally — see
//! `src/main.rs` for the pin setup.

use embedded_hal::spi::{Operation, SpiDevice};
use embedded_storage::{ReadStorage, Storage};

const CMD_READ: u8 = 0x03;
const CMD_WRITE: u8 = 0x02;
const CMD_WREN: u8 = 0x06;
const CMD_RDSR: u8 = 0x05;
const STATUS_WIP: u8 = 0x01;

/// 25LC1024 page size — write_page must not cross.
const PAGE_SIZE: usize = 256;
/// Total capacity (128 KB).
const TOTAL_SIZE: u32 = 128 * 1024;

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

impl<D: SpiDevice> ReadStorage for Eeprom25Lc1024<D> {
    type Error = D::Error;
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), D::Error> {
        Self::read(self, offset, bytes)
    }
    fn capacity(&self) -> usize {
        TOTAL_SIZE as usize
    }
}

impl<D: SpiDevice> Storage for Eeprom25Lc1024<D> {
    /// Page-aware write — splits long buffers across `PAGE_SIZE`
    /// boundaries (the chip wraps writes within a page, so a multi-page
    /// write has to issue one WRITE command per page).
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), D::Error> {
        let mut written = 0;
        while written < bytes.len() {
            let addr = offset + written as u32;
            let in_page = PAGE_SIZE - (addr as usize & (PAGE_SIZE - 1));
            let chunk = (bytes.len() - written).min(in_page);
            self.write_page(addr, &bytes[written..written + chunk])?;
            written += chunk;
        }
        Ok(())
    }
}
