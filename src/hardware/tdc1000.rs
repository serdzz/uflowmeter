#![allow(dead_code)]
extern crate embedded_hal;

use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::serial::Read;
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
    Config0 : 0x00, RW,
    // Config1: 0x01, RW,
    // Config2: 0x02, RW,
    // Config3: 0x03, RW,
    // Config4: 0x04, RW,
    // TOF1: 0x05, RW,
    // TOF0: 0x06, RW,
    // ErrorFlags: 0x07, RW,
    // Timeout: 0x08, RW,
    // ClockRate: 0x09, RW,
);

#[derive(BitfieldSpecifier, Debug, Clone, Copy, Eq, PartialEq)]
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

#[bitfield]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Config0 {
    pub tx_freq_div: FrequencyDividerForTx,
    pub num_tx: B5,
}

impl Default for Config0 {
    fn default() -> Self {
        Self { bytes: [0x45] }
    }
}
