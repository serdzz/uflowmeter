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

/// TDC1000 SPI command byte: bit 6 = write, bits 5:0 = address.
/// Reads send the bare address. Matches TI's `TDC1000_WRITE_BIT` and
/// the C++ driver; the first-pass port used bit 7 instead.
const WRITE_BIT: u8 = 0x40;

/// Config register block, 0x00..0x09.
pub const CONFIG_REG_COUNT: usize = 10;

pub struct Tdc1000<'d, D> {
    spi: D,
    en: Output<'d>,
    res: Output<'d>,
}

impl<'d, D: SpiDevice> Tdc1000<'d, D> {
    /// `en` is the chip enable (held HIGH for "on"). `res` is the RESET
    /// line — see `reset()` for its (counter-intuitive) idle level.
    pub fn new(spi: D, en: Output<'d>, res: Output<'d>) -> Self {
        Self { spi, en, res }
    }

    /// Pulse RESET high, then leave it **low**, which is the level the
    /// chip runs at. Straight from the C++ (`tdc1000.hpp:61-64`):
    ///
    /// ```cpp
    /// void reset(){ RESET::Set(); RESET::Clear(); }
    /// ```
    ///
    /// The first-pass port read the line as active-low and parked it
    /// HIGH — i.e. it held the TDC1000 in reset for its whole life,
    /// which is why every register read came back 0x00 while the
    /// TDC7200 (no reset line) answered normally.
    pub fn reset(&mut self) {
        self.res.set_high();
        self.res.set_low();
    }

    /// Power the chip on (EN HIGH). Caller must wait ~1 ms for the
    /// internal regulator to settle before issuing SPI commands.
    pub fn power_on(&mut self) {
        self.en.set_high();
    }

    /// Power the chip off (EN LOW). Saves ~0.5 mA. All register state
    /// is lost — caller must `load_config()` again after the next
    /// `power_on()`. Currently unused: the C++ only drops EN on a full
    /// meter shutdown, and the measurement loop mirrors that.
    #[allow(dead_code)]
    pub fn power_off(&mut self) {
        self.en.set_low();
    }

    /// Read: bare address byte (C++ `read_register`, tdc1000.hpp:68).
    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error> {
        let cmd = [address & 0x3F];
        let mut rx = [0u8; 1];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut rx)])?;
        Ok(rx[0])
    }

    /// Write: `addr | 0x40`. TI's reference design spells this out as
    /// `#define TDC1000_WRITE_BIT 0x40`, and the C++ `write_register`
    /// (tdc1000.hpp:77) does the same.
    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error> {
        let cmd = [(address & 0x3F) | WRITE_BIT, value];
        self.spi.write(&cmd)
    }

    /// Load the 10-byte config block. The C++ writes these one register
    /// at a time rather than using auto-increment (tdc1000.hpp:30-35).
    pub fn load_config(&mut self, regs: &[u8; CONFIG_REG_COUNT]) -> Result<(), D::Error> {
        for (addr, value) in regs.iter().enumerate() {
            self.write_register(addr as u8, *value)?;
        }
        Ok(())
    }

    /// Select TDC1000 channel — bit 2 (0x04) of CONFIG_2 (reg 0x02),
    /// per the C++ `set_channel` (tdc1000.hpp:44-50). The first-pass
    /// port toggled bit 0 instead.
    pub fn set_channel(&mut self, ch2: bool) -> Result<(), D::Error> {
        let mut v = self.read_register(0x02)?;
        if ch2 {
            v |= 0x04;
        } else {
            v &= !0x04;
        }
        self.write_register(0x02, v)
    }

    /// Clear ERROR_FLAGS (reg 0x07). The C++ writes 0x03 here, not a
    /// blanket 0xFF (tdc1000.hpp:55).
    pub fn clear_error_flags(&mut self) -> Result<(), D::Error> {
        self.write_register(0x07, 0x03)
    }

    /// Read ERROR_FLAGS (reg 0x07) — non-zero means a bad signal on
    /// the last measurement.
    pub fn error_flags(&mut self) -> Result<u8, D::Error> {
        self.read_register(0x07)
    }
}
