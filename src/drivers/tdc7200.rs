//! TDC7200 driver — Time-to-Digital Converter (ToF measurement).
//!
//! SPI framing and the bring-up sequence follow the C++ firmware that
//! actually runs this board (`UFlowMeter_c++/UFlowMeter/hardware/
//! tdc7200.hpp`), cross-checked against TI's reference design in
//! `Docs/UltrasonicIC/TIDM-ULTRASONIC-TDC/Source/TDC_1000_7200_SPI.h`.
//!
//! Command byte: bit 6 = write, bit 7 = auto-increment, bits 5:0 =
//! address. So a single read is the bare address, a single write is
//! `addr | 0x40`, a bulk read is `addr | 0x80` and a bulk write is
//! `addr | 0xC0`.
//!
//! Note the legacy Rust driver in `src/hardware/tdc7200.rs` has read
//! and write inverted relative to this — do not use it as a reference.

use embassy_stm32::gpio::Output;
use embedded_hal::spi::{Operation, SpiDevice};
use uflowmeter::tdc_lib::{stop_numbers, RESULT_BLOCK_LEN};

const WRITE_BIT: u8 = 0x40;
const AUTO_INC_BIT: u8 = 0x80;

/// Config register block, 0x00..0x09 — one bulk auto-increment write,
/// matching the C++ `set_config()`.
pub const CONFIG_REG_COUNT: usize = 10;

/// INT_STATUS (0x02). Writing 1s clears the latched flags.
const REG_INT_STATUS: u8 = 0x02;

pub struct Tdc7200<'d, D> {
    spi: D,
    en: Output<'d>,
    /// Shadow of CONFIG1 (0x00) so `start_measurement` can set
    /// START_MEAS without clobbering the rest of the register — the
    /// C++ keeps the same shadow in `current_regs_.config1`.
    config1: u8,
    /// Shadow of CONFIG2 (0x01) — its NUM_STOP field tells us how many
    /// per-stop results the chip will produce.
    config2: u8,
}

impl<'d, D: SpiDevice> Tdc7200<'d, D> {
    pub fn new(spi: D, en: Output<'d>) -> Self {
        Self {
            spi,
            en,
            config1: 0,
            config2: 0,
        }
    }

    /// Power the chip on (EN HIGH). The C++ raises EN once in `init()`
    /// and only drops it in `shutdown()` — it does not power-cycle
    /// between measurements.
    pub fn power_on(&mut self) {
        self.en.set_high();
    }

    /// Power the chip off (EN LOW). All register state is lost.
    #[allow(dead_code)]
    pub fn power_off(&mut self) {
        self.en.set_low();
    }

    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error> {
        let cmd = [address & 0x3F];
        let mut rx = [0u8; 1];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut rx)])?;
        Ok(rx[0])
    }

    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error> {
        let cmd = [(address & 0x3F) | WRITE_BIT, value];
        self.spi.write(&cmd)
    }

    /// Bulk read with auto-increment starting at `address`.
    pub fn read_bulk(&mut self, address: u8, buf: &mut [u8]) -> Result<(), D::Error> {
        let cmd = [(address & 0x3F) | AUTO_INC_BIT];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(buf)])
    }

    /// Load the 10-byte config block (registers 0x00..0x09) in one
    /// auto-incrementing write, exactly as the C++ `set_config()` does.
    pub fn load_config(&mut self, regs: &[u8; CONFIG_REG_COUNT]) -> Result<(), D::Error> {
        self.config1 = regs[0];
        self.config2 = regs[1];
        let cmd = [WRITE_BIT | AUTO_INC_BIT];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Write(regs)])
    }

    /// Clear all latched interrupt flags (C++ `clear_int_flags()`:
    /// writes 0x1F to INT_STATUS).
    pub fn clear_int_flags(&mut self) -> Result<(), D::Error> {
        self.write_register(REG_INT_STATUS, 0x1F)
    }

    /// Kick off a single ToF measurement: set CONFIG1.START_MEAS on top
    /// of the configured CONFIG1 value and write just that register.
    /// INT (PB0) goes LOW on completion.
    pub fn start_measurement(&mut self) -> Result<(), D::Error> {
        self.write_register(0x00, self.config1 | 0x01)
    }

    /// Read the whole result block (TIME1 .. CALIBRATION2) starting at
    /// 0x10, the same 39 bytes the C++ `trigger_measurement` pulls.
    /// Decode it with `uflowmeter::tdc_lib::decode_tof`.
    pub fn read_results(&mut self) -> Result<[u8; RESULT_BLOCK_LEN], D::Error> {
        let mut rx = [0u8; RESULT_BLOCK_LEN];
        self.read_bulk(0x10, &mut rx)?;
        Ok(rx)
    }

    /// Stops the chip is configured for, from the CONFIG2 shadow.
    pub fn stop_numbers(&self) -> usize {
        stop_numbers(self.config2)
    }
}
