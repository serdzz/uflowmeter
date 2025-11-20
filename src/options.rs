#![allow(dead_code)]

use core::convert::Infallible;

use super::*;
use embedded_storage::Storage;
use modular_bitfield::prelude::*;

#[bitfield]
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub crc: u16,
    pub serial_number: u32,
    pub sensor_type: u8,
    pub tdc1000_regs: B80,
    pub tdc7200_regs: B80,
    pub zero1: B32,
    pub zero2: B32,
    pub v11: B32,
    pub v12: B32,
    pub v13: B32,
    pub v21: B32,
    pub v22: B32,
    pub v23: B32,
    pub k11: B32,
    pub k12: B32,
    pub k13: B32,
    pub k21: B32,
    pub k22: B32,
    pub k23: B32,
    pub uptime: B32,
    pub total: B32,
    pub hour_total: B32,
    pub day_total: B32,
    pub month_total: B32,
    pub rest: B32,
    pub enable_negative: B8,
    pub slave_address: B8,
    pub comm_type: B8,
    pub modbus_mode: B8,
}

#[derive(Debug)]
pub enum Error {
    Storage,
    WrongCrc,
    Spi(spi::Error),
}

impl From<microchip_eeprom_25lcxx::Error<hal::spi::Error, Infallible>> for Error {
    fn from(err: microchip_eeprom_25lcxx::Error<hal::spi::Error, Infallible>) -> Self {
        match err {
            microchip_eeprom_25lcxx::Error::SpiError(e) => Error::Spi(e),
            microchip_eeprom_25lcxx::Error::PinError(_) => Error::Storage,
            _ => Error::Storage,
        }
    }
}

impl Options {
    const SIZE: usize = 1024;
    const OFFSET_PRIMARY: u32 = 0;
    const OFFSET_SECONDARY: u32 = 1024;

    pub fn load(storage: &mut MyStorage) -> Result<Self, Error> {
        assert!(core::mem::size_of::<Options>() < Self::SIZE);
        let mut data = [0; Self::SIZE];
        storage.read(Self::OFFSET_PRIMARY, &mut data)?;
        defmt::info!("data: {:x}", data);
        let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
        let mut bytes = [0u8; core::mem::size_of::<Options>()];
        bytes.copy_from_slice(&data[0..core::mem::size_of::<Options>()]);
        let mut opt = Self { bytes };
        if crc != opt.crc() {
            defmt::warn!("Wrong CRC on primary page {:x} != {:x}", crc, opt.crc());
            storage.read(Self::OFFSET_SECONDARY, &mut data)?;
            let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
            let mut bytes = [0u8; core::mem::size_of::<Options>()];
            bytes.copy_from_slice(&data[0..core::mem::size_of::<Options>()]);
            opt = Self { bytes };
            if crc != opt.crc() {
                defmt::error!("Wrong CRC on secondary page {:x} != {:x}", crc, opt.crc());
                return Err(Error::WrongCrc);
            }
        }
        Ok(opt)
    }

    pub fn save(&mut self, storage: &mut MyStorage) -> Result<(), Error> {
        assert!(core::mem::size_of::<Options>() < Self::SIZE);
        let mut data = [0_u8; Self::SIZE];
        let src = self.into_bytes();
        data[..src.len()].copy_from_slice(&src);
        let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
        self.set_crc(crc);
        let src = self.into_bytes();
        data[..src.len()].copy_from_slice(&src);
        storage.write(Self::OFFSET_PRIMARY, &data)?;
        storage.write(Self::OFFSET_SECONDARY, &data)?;
        defmt::info!("data: {:x}", data);
        Ok(())
    }
}
