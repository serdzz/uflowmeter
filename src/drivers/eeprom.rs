//! 25LC1024 SPI EEPROM driver (128 KB, 256-byte pages).
//!
//! Pin map per src/hardware/pins.rs:
//!   SCK  = PB13     SCK
//!   MISO = PB14
//!   MOSI = PB15
//!   CS   = PC10     (MemoryEn, active-LOW)
//!   HOLD = PC11     (held HIGH = not paused)
//!   WP   = PC12     (held HIGH = writes allowed)
//!
//! The bus is shared with TDC1000 (CS PB11) and TDC7200 (CS PB12) in
//! the legacy firmware. For the initial port we own SPI2 outright;
//! later moves wrap it in `embassy_embedded_hal::shared_bus`.

use embassy_stm32::gpio::Output;
use embassy_stm32::mode::Blocking;
use embassy_stm32::spi::Spi;

// `embassy_stm32::spi::Spi` has two generics: `M: PeriMode` (Blocking
// or Async) and `CM: CommunicationMode` (Master / Slave). Master is
// inside a private mode submodule with no pub re-export, so the
// driver is generic over CM to dodge naming it.

const CMD_READ: u8 = 0x03;
const CMD_WRITE: u8 = 0x02;
const CMD_WREN: u8 = 0x06;
const CMD_RDSR: u8 = 0x05;
const STATUS_WIP: u8 = 0x01;

pub struct Eeprom25Lc1024<'d, CM: embassy_stm32::spi::mode::CommunicationMode> {
    spi: Spi<'d, Blocking, CM>,
    cs: Output<'d>,
    /// HOLD / WP are tied HIGH at construction. Kept as fields so the
    /// pin handles aren't dropped (which would reconfigure them).
    _hold: Output<'d>,
    _wp: Output<'d>,
}

impl<'d, CM: embassy_stm32::spi::mode::CommunicationMode> Eeprom25Lc1024<'d, CM> {
    pub fn new(
        spi: Spi<'d, Blocking, CM>,
        cs: Output<'d>,
        hold: Output<'d>,
        wp: Output<'d>,
    ) -> Self {
        Self { spi, cs, _hold: hold, _wp: wp }
    }

    /// Read `buf.len()` bytes starting at `addr`. 24-bit address.
    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), embassy_stm32::spi::Error> {
        let cmd = [
            CMD_READ,
            ((addr >> 16) & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            (addr & 0xFF) as u8,
        ];
        self.cs.set_low();
        let r = self.spi.blocking_write(&cmd).and_then(|_| self.spi.blocking_read(buf));
        self.cs.set_high();
        r
    }

    /// Write up to a single 256-byte page. Caller is responsible for
    /// not crossing a page boundary.
    pub fn write_page(&mut self, addr: u32, data: &[u8]) -> Result<(), embassy_stm32::spi::Error> {
        // WREN.
        self.cs.set_low();
        let r = self.spi.blocking_write(&[CMD_WREN]);
        self.cs.set_high();
        r?;

        let cmd = [
            CMD_WRITE,
            ((addr >> 16) & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            (addr & 0xFF) as u8,
        ];
        self.cs.set_low();
        let r = self.spi.blocking_write(&cmd).and_then(|_| self.spi.blocking_write(data));
        self.cs.set_high();
        r?;

        // Poll WIP until clear (max ~5 ms per 25LC1024 datasheet).
        self.wait_idle()
    }

    fn wait_idle(&mut self) -> Result<(), embassy_stm32::spi::Error> {
        for _ in 0..10_000 {
            let mut sr = [0u8; 1];
            self.cs.set_low();
            let r = self.spi.blocking_write(&[CMD_RDSR]).and_then(|_| self.spi.blocking_read(&mut sr));
            self.cs.set_high();
            r?;
            if sr[0] & STATUS_WIP == 0 {
                return Ok(());
            }
        }
        Ok(())
    }
}
