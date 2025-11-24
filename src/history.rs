#![allow(dead_code)]

use embedded_storage::Storage;
use modular_bitfield::prelude::*;

#[derive(Debug)]
pub enum Error {
    NoRecords,
    Unitialized,
    Unimplented,
    Storage,
    WrongCrc,
}

type Result<T> = core::result::Result<T, Error>;

#[bitfield]
#[derive(Default, Debug, Clone, Copy)]
pub struct ServiceData {
    pub size: u32,
    pub offset_of_last: u32,
    pub time_of_last: u32,
    crc: u16,
}

pub struct RingStorage<const OFFSET: usize, const SIZE: i32, const ELEMENT_SIZE: i32> {
    pub data: ServiceData,
}
impl<const OFFSET: usize, const SIZE: i32, const ELEMENT_SIZE: i32>
    RingStorage<OFFSET, SIZE, ELEMENT_SIZE>
{
    const OFFSET_OF_STAT_PAGE: usize = 4096;
    const OFFSET: usize = Self::OFFSET_OF_STAT_PAGE + OFFSET;
    pub const SIZE_ON_FLASH: usize =
        size_of::<u32>() + SIZE as usize + size_of::<ServiceData>() + size_of::<u16>();

    pub fn new<S: Storage>(storage: &mut S) -> Result<Self> {
        let mut buf = [0_u8; size_of::<ServiceData>()];
        storage
            .read(Self::OFFSET as u32, &mut buf)
            .map_err(|_| Error::Storage)?;
        let crc =
            crc16::State::<crc16::CCITT_FALSE>::calculate(&buf[..size_of::<ServiceData>() - 2]);
        let data = ServiceData { bytes: buf };
        if crc != data.crc() {
            return Ok(Self {
                data: ServiceData::default(),
            });
        }
        Ok(Self { data })
    }
    fn empty(&mut self) -> bool {
        self.data.size() == 0
    }
    fn size(&mut self) -> u32 {
        self.data.size()
    }
    fn offset(&mut self, index: usize) -> u32 {
        let mut offset = Self::OFFSET + size_of::<ServiceData>() + size_of::<u16>(); // first element offset
        offset += size_of::<u32>() * index;
        offset as u32
    }
    pub fn find<S: Storage>(&mut self, storage: &mut S, time: u32) -> Result<Option<i32>> {
        if self.data.size() == 0 {
            return Ok(None);
        }
        let time = time - time % 60;
        let mut index = self.data.offset_of_last() as usize;
        for _ in 0..self.data.size() {
            let offset = self.offset(index);
            let mut buf = [0_u8; size_of::<i32>()];
            storage.read(offset, &mut buf).map_err(|_| Error::Storage)?;
            let value = i32::from_le_bytes(buf);

            let expected_time = self.data.time_of_last()
                - (self.data.size() - 1 - index as u32) * ELEMENT_SIZE as u32;
            if expected_time == time {
                return Ok(Some(value));
            }

            if index == 0 {
                index = SIZE as usize - 1;
            } else {
                index -= 1;
            }
        }

        Ok(None)
    }
    fn last_value<S: Storage>(&mut self, storage: &mut S) -> Result<Option<i32>> {
        if self.data.size() > 0 {
            return Ok(self.find(storage, self.data.time_of_last()).unwrap());
        }
        Ok(None)
    }

    fn advance_offset_by_one(&mut self) {
        let offset_of_last = self.data.offset_of_last() + 1;
        self.data.set_offset_of_last(offset_of_last);
        if self.data.offset_of_last() == SIZE as u32 {
            self.data.set_offset_of_last(0);
        }
    }
    fn write_value<S: Storage>(&mut self, storage: &mut S, val: i32, time: u32) -> Result<()> {
        if self.data.size() < SIZE as u32 {
            let tmp = self.data.size() + 1;
            self.data.set_size(tmp);
        }
        self.data.set_time_of_last(time);
        storage
            .write(
                self.offset(self.data.offset_of_last() as usize),
                &val.to_le_bytes(),
            )
            .map_err(|_| Error::Storage)?;
        Ok(())
    }
    fn write_service_data<S: Storage>(&mut self, storage: &mut S) -> Result<()> {
        self.advance_offset_by_one();
        let mut buff = self.data.into_bytes();
        self.data
            .set_crc(crc16::State::<crc16::CCITT_FALSE>::calculate(
                &buff[..size_of::<ServiceData>() - 2],
            ));
        buff = self.data.into_bytes();
        storage
            .write(Self::OFFSET as u32, &buff)
            .map_err(|_| Error::Storage)?;
        Ok(())
    }
    pub fn last_stored_timestamp(&mut self) -> u32 {
        self.data.time_of_last()
    }

    pub fn first_stored_timestamp(&mut self) -> u32 {
        if self.data.size() > 0 {
            return self.data.time_of_last() - ELEMENT_SIZE as u32 * (self.data.size() - 1);
        }
        self.data.time_of_last()
    }

    pub fn add<S: Storage>(&mut self, storage: &mut S, val: i32, time: u32) -> Result<()> {
        let mut time = time;
        time -= time % 60;
        if self.empty() {
            self.write_value(storage, val, time)?;
            self.write_service_data(storage)?;
        } else {
            let mut delta = (time - self.data.time_of_last()) as i32;
            if delta > 0 {
                if delta / ELEMENT_SIZE >= SIZE {
                    self.data.set_size(0);
                    self.data.set_offset_of_last(0);
                    self.write_value(storage, val, time)?;
                    self.write_service_data(storage)?;
                } else {
                    // Fill gaps with zero values but correct timestamps
                    while delta > ELEMENT_SIZE {
                        let gap_time = self.data.time_of_last() + ELEMENT_SIZE as u32;
                        self.write_value(storage, 0, gap_time)?;
                        self.write_service_data(storage)?;
                        delta -= ELEMENT_SIZE;
                    }
                    self.write_value(storage, val, time)?;
                    self.write_service_data(storage)?;
                    return Ok(());
                }
            } else if delta.abs() / ELEMENT_SIZE >= self.data.size() as i32 {
                self.data.set_size(0);
                self.data.set_offset_of_last(0);
                self.write_value(storage, val, time)?;
                self.write_service_data(storage)?;
            } else {
                // Handle negative delta (going back in time)
                delta = delta.abs();
                while delta >= ELEMENT_SIZE {
                    storage
                        .write(
                            self.offset(self.data.offset_of_last() as usize),
                            &0_i32.to_le_bytes(),
                        )
                        .map_err(|_| Error::Storage)?;
                    if self.data.offset_of_last() == self.data.size() - 1 {
                        let size = self.data.size() - 1;
                        self.data.set_size(size);
                    }
                    // Handle underflow correctly
                    if self.data.offset_of_last() == 0 {
                        self.data.set_offset_of_last(SIZE as u32 - 1);
                    } else {
                        let tmp = self.data.offset_of_last() - 1;
                        self.data.set_offset_of_last(tmp);
                    }
                    delta -= ELEMENT_SIZE;
                }
                self.write_value(storage, val, time)?;
                self.write_service_data(storage)?;
            }
        }
        Ok(())
    }
}
