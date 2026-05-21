//! Blocking 4-bit HD44780 driver. Mirrors the async `Hd44780` but uses
//! `cortex_m::asm::delay` for timing so it can sit behind the sync
//! `gui::CharacterDisplay` trait that `ui::MenuController` expects.
//!
//! Acceptable trade-off: a full 2x16 render is ~1.5 ms of busy-wait —
//! short enough that embassy's STOP integration can still enter STOP
//! during the idle gaps between renders.

use core::fmt;

use embassy_stm32::gpio::Output;
use uflowmeter::CharacterDisplay;

/// Cycles per microsecond at the current SYSCLK. embassy's default
/// Config leaves us on MSI ≈ 4.19 MHz → ~4 cycles per microsecond.
/// Worst-case slow side; if SYSCLK changes (e.g. PLL enabled later)
/// this needs to scale up so HD44780 timing windows aren't violated.
const CYCLES_PER_US: u32 = 4;

#[inline]
fn delay_us(us: u32) {
    cortex_m::asm::delay(us.saturating_mul(CYCLES_PER_US).max(1));
}

pub struct Hd44780Sync<'d> {
    rs: Output<'d>,
    rw: Output<'d>,
    e: Output<'d>,
    d4: Output<'d>,
    d5: Output<'d>,
    d6: Output<'d>,
    d7: Output<'d>,
    initialized: bool,
}

impl<'d> Hd44780Sync<'d> {
    pub fn new(
        rs: Output<'d>,
        rw: Output<'d>,
        e: Output<'d>,
        d4: Output<'d>,
        d5: Output<'d>,
        d6: Output<'d>,
        d7: Output<'d>,
    ) -> Self {
        Self {
            rs,
            rw,
            e,
            d4,
            d5,
            d6,
            d7,
            initialized: false,
        }
    }

    pub fn init(&mut self) {
        delay_us(50_000); // 50 ms LCD internal power-up
        self.rw.set_low();
        self.rs.set_low();
        self.write_nibble(0x3);
        delay_us(5_000);
        self.write_nibble(0x3);
        delay_us(150);
        self.write_nibble(0x3);
        delay_us(150);
        self.write_nibble(0x2); // 4-bit mode
        delay_us(150);
        self.command(0x28); // 4-bit, 2-line, 5x8
        self.command(0x08); // display off
        self.command(0x01); // clear
        delay_us(2_000);
        self.command(0x06); // entry mode: increment, no shift
        self.command(0x0C); // display on, cursor off, blink off
        self.initialized = true;
    }

    fn command(&mut self, byte: u8) {
        self.rs.set_low();
        self.write_byte(byte);
        delay_us(40);
    }

    fn data(&mut self, byte: u8) {
        self.rs.set_high();
        self.write_byte(byte);
        delay_us(40);
    }

    fn write_byte(&mut self, byte: u8) {
        self.write_nibble(byte >> 4);
        self.write_nibble(byte & 0x0F);
    }

    fn write_nibble(&mut self, nibble: u8) {
        self.set_data_bit(0, nibble & 0x1);
        self.set_data_bit(1, (nibble >> 1) & 0x1);
        self.set_data_bit(2, (nibble >> 2) & 0x1);
        self.set_data_bit(3, (nibble >> 3) & 0x1);
        self.e.set_high();
        delay_us(1);
        self.e.set_low();
        delay_us(1);
    }

    #[inline]
    fn set_data_bit(&mut self, idx: u8, value: u8) {
        let pin: &mut Output<'d> = match idx {
            0 => &mut self.d4,
            1 => &mut self.d5,
            2 => &mut self.d6,
            _ => &mut self.d7,
        };
        if value != 0 {
            pin.set_high();
        } else {
            pin.set_low();
        }
    }
}

impl<'d> fmt::Write for Hd44780Sync<'d> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            self.data(b);
        }
        Ok(())
    }
}

impl<'d> CharacterDisplay for Hd44780Sync<'d> {
    fn set_position(&mut self, col: u8, row: u8) {
        let addr = if row == 0 { col } else { 0x40 | col };
        self.command(0x80 | addr);
    }

    fn clear(&mut self) {
        self.command(0x01);
        delay_us(2_000);
    }

    fn reset_custom_chars(&mut self) {
        // No-op for now — custom chars aren't loaded yet (will need
        // them once the Russian glyphs show up on the menus).
    }
}
