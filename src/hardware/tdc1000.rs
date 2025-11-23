#![allow(dead_code)]
extern crate embedded_hal;

use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::digital::v2::OutputPin;
use modular_bitfield::prelude::*;

#[allow(dead_code)]
#[derive(Debug)]
pub enum Error<SpiError, PinError> {
    SpiError(SpiError),
    PinError(PinError),
}

#[allow(dead_code)]
pub struct TDC1000<SPI, CS, RESET, EN> {
    spi: SPI,
    cs: CS,
    reset: RESET,
    en: EN,
}

impl<SPI, CS, RESET, EN, SpiError, PinError> TDC1000<SPI, CS, RESET, EN>
where
    SPI: Transfer<u8, Error = SpiError> + Write<u8, Error = SpiError>,
    CS: OutputPin<Error = PinError>,
    RESET: OutputPin<Error = PinError>,
    EN: OutputPin<Error = PinError>,
{
    const REG_COUNT: usize = 10;

    pub fn new(spi: SPI, cs: CS, reset: RESET, en: EN) -> Self {
        TDC1000 { spi, cs, reset, en }
    }

    pub fn release(self) -> SPI {
        self.spi
    }

    pub fn set_config(&mut self, bytes: &[u8]) -> Result<(), Error<SpiError, PinError>> {
        for i in bytes.iter().enumerate() {
            self.write_register(i.0.try_into().unwrap(), *i.1)?;
        }
        Ok(())
    }

    pub fn read_all_registers(&mut self, data: &mut [u8]) -> Result<(), Error<SpiError, PinError>> {
        for tmp in data.iter_mut().enumerate() {
            if tmp.0 < Self::REG_COUNT {
                *tmp.1 = self.read_register(tmp.0.try_into().unwrap())?;
            }
        }
        Ok(())
    }

    pub fn set_channel(&mut self, ch: bool) -> Result<(), Error<SpiError, PinError>> {
        let mut val = self.read_register(0x02)?;
        if ch {
            val |= 0x04;
        } else {
            val &= !0x04;
        }
        self.write_register(0x02, val)?;
        Ok(())
    }

    pub fn clear_error_flags(&mut self) -> Result<(), Error<SpiError, PinError>> {
        self.write_register(0x07, 0x03)?;
        Ok(())
    }

    pub fn get_error_flags(&mut self) -> Result<u8, Error<SpiError, PinError>> {
        let val = self.read_register(0x07)?;
        Ok(val)
    }

    pub fn reset(&mut self) -> Result<(), Error<SpiError, PinError>> {
        self.reset.set_low().map_err(Error::PinError)?;
        self.reset.set_high().map_err(Error::PinError)?;
        Ok(())
    }

    pub fn get_config0(&mut self) -> Result<Config0, Error<SpiError, PinError>> {
        Config0::read(&mut self.spi, &mut self.cs)
    }

    pub fn set_config0(&mut self, cfg: Config0) -> Result<(), Error<SpiError, PinError>> {
        cfg.write(&mut self.spi, &mut self.cs)
    }

    fn read_register(&mut self, address: u8) -> Result<u8, Error<SpiError, PinError>> {
        let address = [address | 0x40];
        let mut data = [0u8];
        self.cs.set_low().map_err(Error::PinError)?;
        self.spi.write(&address).map_err(Error::SpiError)?;
        self.spi.transfer(&mut data).map_err(Error::SpiError)?;
        self.cs.set_high().map_err(Error::PinError)?;
        Ok(data[0])
    }

    fn write_register(&mut self, address: u8, value: u8) -> Result<(), Error<SpiError, PinError>> {
        let address = [address | 0x40];
        let value = [value];
        self.cs.set_low().map_err(Error::PinError)?;
        self.spi.write(&address).map_err(Error::SpiError)?;
        self.spi.write(&value).map_err(Error::SpiError)?;
        self.cs.set_high().map_err(Error::PinError)?;
        Ok(())
    }
}

pub(crate) trait ReadOnlyRegister: From<u8> {
    const ADDR: u8;

    fn read<
        SpiError,
        PinError,
        CS: OutputPin<Error = PinError>,
        SPI: Transfer<u8, Error = SpiError>,
    >(
        spi: &mut SPI,
        cs: &mut CS,
    ) -> Result<Self, Error<SpiError, PinError>> {
        let buf = &mut [Self::ADDR, 0u8];
        cs.set_low().map_err(Error::PinError)?;
        let data = spi
            .transfer(buf)
            .map(|buf| buf[1].into())
            .map_err(Error::SpiError);
        cs.set_high().map_err(Error::PinError)?;
        data
    }
}

impl<RWR: ReadWriteRegister> ReadOnlyRegister for RWR {
    const ADDR: u8 = RWR::ADDR;
}

pub(crate) trait ReadWriteRegister: From<u8> + Into<u8> {
    const ADDR: u8;

    fn write<
        SpiError,
        PinError,
        CS: OutputPin<Error = PinError>,
        SPI: Write<u8, Error = SpiError>,
    >(
        self,
        spi: &mut SPI,
        cs: &mut CS,
    ) -> Result<(), Error<SpiError, PinError>> {
        cs.set_low().map_err(Error::PinError)?;
        let ret: Result<(), Error<SpiError, PinError>> = spi
            .write(&[Self::ADDR, self.into()])
            .map_err(Error::SpiError);
        cs.set_high().map_err(Error::PinError)?;
        ret
    }
}

macro_rules! register {
    ($Reg:ident, $addr:literal, RO) => {
        impl ReadOnlyRegister for $Reg {
            const ADDR: u8 = $addr;
        }

        impl From<u8> for $Reg {
            fn from(raw: u8) -> Self {
                Self::from_bytes([raw])
            }
        }
    };
    ($Reg:ident, $addr:literal, RW) => {
        impl ReadWriteRegister for $Reg {
            const ADDR: u8 = $addr;
        }

        impl From<u8> for $Reg {
            fn from(raw: u8) -> Self {
                Self::from_bytes([raw])
            }
        }

        impl From<$Reg> for u8 {
            fn from(reg: $Reg) -> Self {
                reg.into_bytes()[0]
            }
        }
    };
}

macro_rules! register_map {
    ($($Reg:ident: $addr:literal, $rw:tt,)+) => {
        $(
            register!($Reg, $addr, $rw);
        )+
    };
}

register_map!(
    Config0: 0x00, RW,      // Device Configuration Register 0
    Config1: 0x01, RW,      // Device Configuration Register 1  
    Config2: 0x02, RW,      // Device Configuration Register 2
    Config3: 0x03, RW,      // Device Configuration Register 3
    Config4: 0x04, RW,      // Device Configuration Register 4
    TOF1: 0x05, RW,         // Time of Flight Register 1
    TOF0: 0x06, RW,         // Time of Flight Register 0
    ErrorFlags: 0x07, RW,   // Error Flags Register
    Timeout: 0x08, RW,      // Timeout Register
    ClockRate: 0x09, RW,    // Clock Rate Register
);

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 3]
pub enum FrequencyDividerForTx {
    Div2,
    Div4,
    Div8,
    Div16,
    Div32,
    Div64,
    Div128,
    Div256,
}

// ============================================================================
// Register Structures with Bitfield Definitions
// ============================================================================

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config0 {
    /// TX frequency divider (bits 0-2)
    pub tx_freq_div: FrequencyDividerForTx,
    /// Number of TX pulses (bits 3-7)
    pub num_tx: B5,
}

impl Default for Config0 {
    fn default() -> Self {
        Self { bytes: [0x45] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config1 {
    /// Measurement mode selection (bit 0)
    pub measurement_mode: bool,
    /// Enable continuous mode (bit 1)
    pub continuous_mode: bool,
    /// Channel selection (bit 2)
    pub channel_select: bool,
    /// Reserved (bits 3-7)
    #[skip]
    __: B5,
}

impl Default for Config1 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config2 {
    /// Transmit pulse mask (bits 0-4)
    pub tx_pulse_mask: B5,
    /// Clock selection (bits 5-6)
    pub clock_select: B2,
    /// Reserved (bit 7)
    #[skip]
    __: B1,
}

impl Default for Config2 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config3 {
    /// Transducer frequency selection (bits 0-2)
    pub transducer_freq: B3,
    /// Amplifier gain (bits 3-5)
    pub amplifier_gain: B3,
    /// Reserved (bits 6-7)
    #[skip]
    __: B2,
}

impl Default for Config3 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config4 {
    /// Time-to-digital conversion resolution (bits 0-2)
    pub tdc_resolution: B3,
    /// Measurement range (bits 3-4)
    pub measurement_range: B2,
    /// Enable auto-calibration (bit 5)
    pub auto_calibration: bool,
    /// Reserved (bits 6-7)
    #[skip]
    __: B2,
}

impl Default for Config4 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TOF1 {
    /// Time of Flight high byte (bits 0-7)
    pub tof_high: u8,
}

impl Default for TOF1 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TOF0 {
    /// Time of Flight low byte (bits 0-7)
    pub tof_low: u8,
}

impl Default for TOF0 {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ErrorFlags {
    /// Time of flight error flag (bit 0)
    pub tof_error: bool,
    /// Calibration error flag (bit 1)
    pub cal_error: bool,
    /// Range overflow flag (bit 2)
    pub range_overflow: bool,
    /// ADC overflow flag (bit 3)
    pub adc_overflow: bool,
    /// Reserved (bits 4-7)
    #[skip]
    __: B4,
}

impl Default for ErrorFlags {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Timeout {
    /// Timeout value in clock cycles (bits 0-7)
    pub timeout_value: u8,
}

impl Default for Timeout {
    fn default() -> Self {
        Self { bytes: [0xFF] }
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ClockRate {
    /// Clock divider ratio (bits 0-3)
    pub clock_divider: B4,
    /// Clock enable (bit 4)
    pub clock_enable: bool,
    /// Reserved (bits 5-7)
    #[skip]
    __: B3,
}

impl Default for ClockRate {
    fn default() -> Self {
        Self { bytes: [0x00] }
    }
}
