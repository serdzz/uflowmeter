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

// ============================================================================
// Enumerations for Configuration Options
// ============================================================================

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 3]
pub enum FrequencyDividerForTx {
    /// Divide by 2
    Div2,
    /// Divide by 4
    Div4,
    /// Divide by 8
    Div8,
    /// Divide by 16
    Div16,
    /// Divide by 32
    Div32,
    /// Divide by 64
    Div64,
    /// Divide by 128
    Div128,
    /// Divide by 256
    Div256,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 2]
pub enum ClockSelection {
    /// Internal oscillator (default)
    Internal = 0,
    /// External clock input 1
    ExternalClock1 = 1,
    /// External clock input 2
    ExternalClock2 = 2,
    /// External clock input 3
    ExternalClock3 = 3,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 3]
pub enum TransducerFrequency {
    /// 500 kHz
    Freq500kHz = 0,
    /// 1 MHz
    Freq1MHz = 1,
    /// 2 MHz
    Freq2MHz = 2,
    /// 4 MHz
    Freq4MHz = 3,
    /// Reserved
    Reserved4 = 4,
    /// Reserved
    Reserved5 = 5,
    /// Reserved
    Reserved6 = 6,
    /// Reserved
    Reserved7 = 7,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 3]
pub enum AmplifierGain {
    /// 0 dB gain
    Gain0dB = 0,
    /// 6 dB gain
    Gain6dB = 1,
    /// 12 dB gain
    Gain12dB = 2,
    /// 18 dB gain
    Gain18dB = 3,
    /// 24 dB gain
    Gain24dB = 4,
    /// 30 dB gain
    Gain30dB = 5,
    /// 36 dB gain
    Gain36dB = 6,
    /// 42 dB gain
    Gain42dB = 7,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 3]
pub enum TDCResolution {
    /// 250 picoseconds
    Ps250 = 0,
    /// 500 picoseconds
    Ps500 = 1,
    /// 1 nanosecond
    Ns1 = 2,
    /// 2 nanoseconds
    Ns2 = 3,
    /// 4 nanoseconds
    Ns4 = 4,
    /// 8 nanoseconds
    Ns8 = 5,
    /// 16 nanoseconds
    Ns16 = 6,
    /// 32 nanoseconds
    Ns32 = 7,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 2]
pub enum MeasurementRange {
    /// 0-100 nanoseconds
    Range100ns = 0,
    /// 0-200 nanoseconds
    Range200ns = 1,
    /// 0-400 nanoseconds
    Range400ns = 2,
    /// 0-800 nanoseconds
    Range800ns = 3,
}

#[derive(Specifier, Debug, Clone, Copy, Eq, PartialEq)]
#[bits = 4]
pub enum ClockDivider {
    /// Divide by 1 (no division)
    Div1 = 0,
    /// Divide by 2
    Div2 = 1,
    /// Divide by 4
    Div4 = 2,
    /// Divide by 8
    Div8 = 3,
    /// Divide by 16
    Div16 = 4,
    /// Divide by 32
    Div32 = 5,
    /// Divide by 64
    Div64 = 6,
    /// Divide by 128
    Div128 = 7,
    /// Divide by 256
    Div256 = 8,
    /// Divide by 512
    Div512 = 9,
    /// Divide by 1024
    Div1024 = 10,
    /// Divide by 2048
    Div2048 = 11,
    /// Divide by 4096
    Div4096 = 12,
    /// Divide by 8192
    Div8192 = 13,
    /// Divide by 16384
    Div16384 = 14,
    /// Divide by 32768
    Div32768 = 15,
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
    pub clock_select: ClockSelection,
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
    pub transducer_freq: TransducerFrequency,
    /// Amplifier gain (bits 3-5)
    pub amplifier_gain: AmplifierGain,
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
    pub tdc_resolution: TDCResolution,
    /// Measurement range (bits 3-4)
    pub measurement_range: MeasurementRange,
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
    pub clock_divider: ClockDivider,
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
