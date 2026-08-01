//! Minimal HD44780 4-bit parallel driver for embassy-stm32.
//!
//! Drives RS / RW / E + D4..D7 via `embassy_stm32::gpio::Output` and
//! waits via `embassy_time::Timer`. Async — every command yields while
//! the LCD's internal timing requirements are observed, so the executor
//! can run other tasks during the LCD's ~40 µs / ~1.5 ms windows
//! instead of busy-waiting like the old `lcd` crate did.
//!
//! Not exhaustive — only the commands the uflowmeter UI needs:
//!   - init (4-bit, 2-line, 5x8 font)
//!   - clear / home
//!   - set cursor position
//!   - write text
//!   - upload custom character into CGRAM (for Cyrillic glyphs)
//!
//! Pin assignment matches the board's existing wiring (see
//! `src/hardware/pins.rs`).

use embassy_stm32::gpio::Output;
use embassy_time::Timer;

pub struct Hd44780<'d> {
    rs: Output<'d>,
    rw: Output<'d>,
    e: Output<'d>,
    d4: Output<'d>,
    d5: Output<'d>,
    d6: Output<'d>,
    d7: Output<'d>,
    cursor_col: u8,
    cursor_row: u8,
}

impl<'d> Hd44780<'d> {
    /// Wraps the LCD control pins. Caller must have already configured
    /// them as push-pull outputs in `Level::Low`.
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
            cursor_col: 0,
            cursor_row: 0,
        }
    }

    /// HD44780 4-bit init sequence per datasheet.
    pub async fn init(&mut self) {
        // Wait for LCD internal power-up (>40 ms).
        Timer::after_millis(50).await;
        // Write-only.
        self.rw.set_low();
        // Three function-set 0x3 nibbles to force 8-bit mode, then drop
        // to 4-bit. RS=0 for command.
        self.rs.set_low();
        self.write_nibble(0x3).await;
        Timer::after_millis(5).await;
        self.write_nibble(0x3).await;
        Timer::after_micros(150).await;
        self.write_nibble(0x3).await;
        Timer::after_micros(150).await;
        // 4-bit mode select.
        self.write_nibble(0x2).await;
        Timer::after_micros(150).await;
        // Function set: 4-bit, 2-line, 5x8 font.
        self.command(0x28).await;
        // Display off.
        self.command(0x08).await;
        // Clear display.
        self.command(0x01).await;
        Timer::after_millis(2).await;
        // Entry mode: increment, no shift.
        self.command(0x06).await;
        // Display on, cursor off, blink off.
        self.command(0x0C).await;
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    /// Clear DDRAM, return cursor home.
    pub async fn clear(&mut self) {
        self.command(0x01).await;
        Timer::after_millis(2).await;
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    /// Move cursor. `row` is 0 (top) or 1 (bottom) for a 2x16 LCD.
    pub async fn set_position(&mut self, col: u8, row: u8) {
        let addr = match row {
            0 => col,
            _ => 0x40 | col,
        };
        self.command(0x80 | addr).await;
        self.cursor_col = col;
        self.cursor_row = row;
    }

    /// Write an ASCII string at the current cursor position.
    ///
    /// Unused by the UI path — `DeferredDisplay` batches renders and
    /// flushes through `write_byte_pub`. Kept as a direct-write helper
    /// for bring-up / debugging.
    #[allow(dead_code)]
    pub async fn write_str(&mut self, s: &str) {
        for b in s.bytes() {
            self.data(b).await;
        }
    }

    /// Load an 8-byte pattern into CGRAM slot `slot` (0..8). Pattern bits
    /// 4..0 are the 5 active columns; bit 7 (LSB of pattern[0]) is the
    /// top row.
    pub async fn upload_char(&mut self, slot: u8, pattern: &[u8; 8]) {
        // CGRAM address = 0x40 + slot * 8.
        self.command(0x40 | ((slot & 0x07) << 3)).await;
        for &row in pattern {
            self.data(row).await;
        }
        // Restore DDRAM cursor so the next write_str doesn't land in
        // CGRAM territory.
        self.set_position(self.cursor_col, self.cursor_row).await;
    }

    /// Write a command byte (RS=0). Blocks for the worst-case 40 µs
    /// internal cycle.
    async fn command(&mut self, byte: u8) {
        self.rs.set_low();
        self.write_byte(byte).await;
        Timer::after_micros(40).await;
    }

    /// Write a data byte (RS=1).
    async fn data(&mut self, byte: u8) {
        self.rs.set_high();
        self.write_byte(byte).await;
        Timer::after_micros(40).await;
    }

    /// Public alias for `data()` — used by `drivers::deferred_display`
    /// to flush per-character ops captured from the sync render path.
    pub async fn write_byte_pub(&mut self, byte: u8) {
        self.data(byte).await;
    }

    async fn write_byte(&mut self, byte: u8) {
        self.write_nibble(byte >> 4).await;
        self.write_nibble(byte & 0x0F).await;
    }

    async fn write_nibble(&mut self, nibble: u8) {
        self.set_data_bit(0, nibble & 0x1);
        self.set_data_bit(1, (nibble >> 1) & 0x1);
        self.set_data_bit(2, (nibble >> 2) & 0x1);
        self.set_data_bit(3, (nibble >> 3) & 0x1);
        // Pulse E: high for >=450 ns, then low. Datasheet says enable
        // pulse width 230 ns @ 5 V; we round up to 1 µs to be safe at
        // 3.3 V and account for LCD level shifters.
        self.e.set_high();
        Timer::after_micros(1).await;
        self.e.set_low();
        Timer::after_micros(1).await;
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
