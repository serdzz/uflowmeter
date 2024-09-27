#![allow(dead_code)]
extern crate embedded_hal;

use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::serial::Read;

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
