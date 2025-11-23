#![allow(dead_code)]

use core::mem::size_of;
use modular_bitfield::prelude::*;

#[cfg(test)]
use std::default::Default;

type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Clone, Copy)]
pub enum Error {
    NoRecords,
    Unitialized,
    Unimplented,
    Storage,
    WrongCrc,
}

#[bitfield]
#[derive(Debug, Clone, Copy)]
pub struct ServiceData {
    pub size: u32,
    pub offset_of_last: u32,
    pub time_of_last: u32,
    crc: u16,
}

impl Default for ServiceData {
    fn default() -> Self {
        Self::new()
    }
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

    pub fn new_empty() -> Self {
        Self {
            data: ServiceData::default(),
        }
    }

    fn empty(&mut self) -> bool {
        self.data.size() == 0
    }

    pub fn size(&mut self) -> u32 {
        self.data.size()
    }

    pub fn offset(&mut self, index: usize) -> u32 {
        let mut offset = Self::OFFSET + size_of::<ServiceData>() + size_of::<u16>(); // first element offset
        offset += size_of::<u32>() * index;
        offset as u32
    }

    pub fn advance_offset_by_one(&mut self) {
        let offset_of_last = self.data.offset_of_last() + 1;
        self.data.set_offset_of_last(offset_of_last);
        if self.data.offset_of_last() == SIZE as u32 {
            self.data.set_offset_of_last(0);
        }
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
}
