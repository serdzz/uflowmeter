#![allow(dead_code)]

use embedded_storage::Storage;
use modular_bitfield::prelude::*;

/// Communication type — determines which protocol runs on USART1
/// Matches C++ Configuration::CommType
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommType {
    /// No communication
    None = 0,
    /// M-Bus (periodic datagram transmission)
    MBus = 1,
    /// Modbus RTU slave
    ModBus = 2,
    /// Analog output (not UART-based)
    Analog = 3,
}

impl CommType {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => CommType::MBus,
            2 => CommType::ModBus,
            3 => CommType::Analog,
            _ => CommType::None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Line rate USART1 runs at for this communication type.
    ///
    /// One UART serves all of them, so the selected protocol decides
    /// the rate and everything else on the line follows — including the
    /// shell, which is reachable at 9600 while Modbus is selected
    /// rather than at its own 115200. There is only one pair of wires.
    ///
    /// `None` and `Analog` do not use the serial line for a protocol,
    /// so they leave it at the fast rate where the shell is comfortable
    /// and a firmware image does not take a quarter of an hour.
    pub fn baudrate(self) -> u32 {
        match self {
            CommType::ModBus => 9_600,
            CommType::MBus => 1_200,
            CommType::None | CommType::Analog => 115_200,
        }
    }
}

/// Modbus RTU inter-frame silence for `baud`, in microseconds.
///
/// The spec puts the frame boundary at 3.5 character times, and a
/// character is 11 bits — start, eight data, parity or its stand-in,
/// stop. Above 19200 baud it stops scaling and fixes the value at
/// 1750 µs instead, because the true figure gets short enough that
/// timing it reliably costs more than it is worth.
///
/// This has to follow the baud rate rather than sit at a constant: at
/// 9600 the real boundary is about 4 ms, and a framer using the 1750 µs
/// of the fast case would cut frames apart in the middle.
pub fn modbus_frame_gap_micros(baud: u32) -> u64 {
    const FIXED_ABOVE: u32 = 19_200;
    const FIXED_MICROS: u64 = 1_750;
    if baud > FIXED_ABOVE || baud == 0 {
        FIXED_MICROS
    } else {
        // 3.5 chars * 11 bits = 38.5 bit times; in microseconds that is
        // 38_500_000 / baud.
        38_500_000u64 / baud as u64
    }
}

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
    /// MeterConfig::const_val — speed-of-sound geometry constant
    /// L²/(2·cos α). Stored as f32 bits in a B32 slot to stay inside
    /// the modular_bitfield API. Appended to the layout so prior
    /// EEPROM data still CRC-matches and is read with const_val=0
    /// (which keeps the legacy "no flow until calibrated" behavior).
    pub const_val: B32,
    /// Padding to round the total bitfield up to a whole even number
    /// of bytes (116) so Modbus holding-register reads at the very
    /// end of Options (e.g. register 57, which covers const_val's
    /// high byte) don't trip the "end_byte > options_bytes.len()"
    /// OOB check in modbus_handler::handle_read_holding_registers.
    pub reserved: B8,
}

#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug)]
pub enum Error<E> {
    Storage,
    WrongCrc,
    Spi(E),
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Self {
        Error::Spi(e)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}

/// Static buffer wrapper that implements Sync, allowing use in static context.
/// SAFETY: Only safe when accessed from a single thread (e.g., RTIC init).
#[allow(unsafe_code)]
struct StaticBuf {
    buf: core::cell::UnsafeCell<[u8; 1024]>,
}

#[allow(unsafe_code)]
unsafe impl Sync for StaticBuf {}

impl StaticBuf {
    const fn new() -> Self {
        Self {
            buf: core::cell::UnsafeCell::new([0; 1024]),
        }
    }

    /// Get mutable reference to the buffer.
    /// SAFETY: Caller must ensure no concurrent access.
    #[allow(unsafe_code, clippy::mut_from_ref)]
    unsafe fn get_mut(&self) -> &mut [u8; 1024] {
        &mut *self.buf.get()
    }
}

impl Options {
    pub const SIZE: usize = 1024;
    const OFFSET_PRIMARY: u32 = 0;
    const OFFSET_SECONDARY: u32 = 1024;

    /// Load Options from storage using a caller-provided buffer.
    /// This avoids allocating 1KB+ on the stack, which caused stack overflow
    /// on STM32L151RC with limited stack in RTIC init.
    pub fn load_with_buf<S, E>(
        storage: &mut S,
        data: &mut [u8; Self::SIZE],
    ) -> Result<Self, Error<E>>
    where
        S: Storage,
        Error<E>: From<S::Error>,
    {
        assert!(core::mem::size_of::<Options>() < Self::SIZE);
        storage.read(Self::OFFSET_PRIMARY, data)?;
        #[cfg(not(test))]
        defmt::info!("data: {:x}", data);
        let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
        let mut bytes = [0u8; core::mem::size_of::<Options>()];
        bytes.copy_from_slice(&data[0..core::mem::size_of::<Options>()]);
        let mut opt = Self { bytes };
        if crc != opt.crc() {
            #[cfg(not(test))]
            defmt::warn!("Wrong CRC on primary page {:x} != {:x}", crc, opt.crc());
            storage.read(Self::OFFSET_SECONDARY, data)?;
            let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
            let mut bytes = [0u8; core::mem::size_of::<Options>()];
            bytes.copy_from_slice(&data[0..core::mem::size_of::<Options>()]);
            opt = Self { bytes };
            if crc != opt.crc() {
                #[cfg(not(test))]
                defmt::error!("Wrong CRC on secondary page {:x} != {:x}", crc, opt.crc());
                return Err(Error::WrongCrc);
            }
        }
        Ok(opt)
    }

    /// Save Options to storage using a caller-provided buffer.
    pub fn save_with_buf<S, E>(
        &mut self,
        storage: &mut S,
        data: &mut [u8; Self::SIZE],
    ) -> Result<(), Error<E>>
    where
        S: Storage,
        Error<E>: From<S::Error>,
    {
        assert!(core::mem::size_of::<Options>() < Self::SIZE);
        data.fill(0);
        let src = self.into_bytes();
        data[..src.len()].copy_from_slice(&src);
        let crc = crc16::State::<crc16::CCITT_FALSE>::calculate(&data[2..]);
        self.set_crc(crc);
        let src = self.into_bytes();
        data[..src.len()].copy_from_slice(&src);
        storage.write(Self::OFFSET_PRIMARY, data)?;
        storage.write(Self::OFFSET_SECONDARY, data)?;
        #[cfg(not(test))]
        defmt::info!("data: {:x}", data);
        Ok(())
    }

    /// Convenience wrappers that use a static buffer.
    /// Safe only in RTIC init (single-threaded, no reentrancy).
    /// For concurrent contexts, use load_with_buf/save_with_buf instead.
    #[allow(unsafe_code)]
    pub fn load<S, E>(storage: &mut S) -> Result<Self, Error<E>>
    where
        S: Storage,
        Error<E>: From<S::Error>,
    {
        static DATA_BUF: StaticBuf = StaticBuf::new();
        // SAFETY: Safe only in single-threaded context (RTIC init).
        // Do NOT call from concurrent tasks — use load_with_buf() instead.
        let data = unsafe { DATA_BUF.get_mut() };
        Self::load_with_buf(storage, data)
    }

    #[allow(unsafe_code)]
    pub fn save<S, E>(&mut self, storage: &mut S) -> Result<(), Error<E>>
    where
        S: Storage,
        Error<E>: From<S::Error>,
    {
        static DATA_BUF: StaticBuf = StaticBuf::new();
        // SAFETY: Same as load() — safe only in init, use save_with_buf() otherwise.
        let data = unsafe { DATA_BUF.get_mut() };
        Self::save_with_buf(self, storage, data)
    }
}
