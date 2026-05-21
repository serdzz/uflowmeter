//! `CharacterDisplay` adapter that batches writes into a `heapless::Vec`
//! during the sync Widget render pass, then flushes them asynchronously
//! to the LCD.
//!
//! Cyrillic support: HD44780 has 8 CGRAM slots. On each render we
//! reset the slot table and dynamically allocate slots as Cyrillic
//! glyphs appear in `write_str`. If a single render uses >8 distinct
//! Cyrillic glyphs the overflow chars render as `?` — same behavior
//! the legacy display.rs had.

use core::fmt;

use heapless::Vec;
use uflowmeter::CharacterDisplay;

use crate::drivers::cyrillic;
use crate::drivers::hd44780::Hd44780;

/// Max ops per frame. 2x16 LCD has 32 chars + position commands +
/// up to 8 CGRAM uploads (each = 1 op).
const MAX_OPS: usize = 128;

#[derive(Clone, Copy)]
enum Op {
    SetPos(u8, u8),
    Clear,
    /// Upload pattern[..] into CGRAM slot.
    UploadChar(u8, [u8; 8]),
    Data(u8),
}

pub struct DeferredDisplay {
    ops: Vec<Op, MAX_OPS>,
    /// Per-frame CGRAM allocation: which Cyrillic char is in each
    /// slot (0..8). `None` = slot free.
    slots: [Option<char>; 8],
    next_slot: u8,
}

impl DeferredDisplay {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            slots: [None; 8],
            next_slot: 0,
        }
    }

    /// Push the queued ops out to the LCD. Async — yields between
    /// commands so other tasks (and STOP entry) can interleave.
    pub async fn flush(&mut self, lcd: &mut Hd44780<'_>) {
        // CGRAM uploads go FIRST in a separate pass so they can't
        // shift the DDRAM cursor between later SetPos/Data ops
        // (Hd44780::upload_char restores via its own internal cursor
        // tracker which doesn't see our raw write_byte_pub writes).
        for op in self.ops.iter() {
            if let Op::UploadChar(slot, pattern) = op {
                lcd.upload_char(*slot, pattern).await;
            }
        }
        for op in self.ops.drain(..) {
            match op {
                Op::SetPos(col, row) => lcd.set_position(col, row).await,
                Op::Clear => lcd.clear().await,
                Op::UploadChar(_, _) => {}
                Op::Data(b) => lcd.write_byte_pub(b).await,
            }
        }
        // Reset slot table for next frame.
        self.slots = [None; 8];
        self.next_slot = 0;
    }

    /// Look up (or allocate) a CGRAM slot for the given Cyrillic char.
    /// Emits an UploadChar op the first time the glyph appears in this
    /// frame. Returns the slot index (0..8) to write as the data byte,
    /// or None if the slot table is full / the glyph is unknown.
    fn alloc_slot(&mut self, c: char) -> Option<u8> {
        for (i, s) in self.slots.iter().enumerate() {
            if *s == Some(c) {
                return Some(i as u8);
            }
        }
        if self.next_slot >= 8 {
            return None;
        }
        let pattern = cyrillic::lookup(c)?;
        let slot = self.next_slot;
        self.slots[slot as usize] = Some(c);
        self.next_slot += 1;
        self.ops.push(Op::UploadChar(slot, pattern)).ok();
        Some(slot)
    }

    /// Emit one decoded char as one or more ops. Cyrillic chars are
    /// substituted for a CGRAM slot index; plain ASCII passes through.
    fn push_char(&mut self, c: char) {
        if c.is_ascii() {
            self.ops.push(Op::Data(c as u8)).ok();
            return;
        }
        // Cyrillic letters with a Latin look-alike (А/Р/С/...) →
        // use the ROM glyph, save CGRAM for the truly Cyrillic-shaped
        // letters (Д/Ж/Й/...).
        if let Some(b) = cyrillic::latin_lookalike(c) {
            self.ops.push(Op::Data(b)).ok();
            return;
        }
        match self.alloc_slot(c) {
            Some(slot) => {
                self.ops.push(Op::Data(slot)).ok();
            }
            None => {
                self.ops.push(Op::Data(b'?')).ok();
            }
        }
    }
}

impl Default for DeferredDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Write for DeferredDisplay {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // We get a &str — already valid UTF-8. Iterate chars directly.
        for c in s.chars() {
            self.push_char(c);
        }
        Ok(())
    }
}

impl CharacterDisplay for DeferredDisplay {
    fn set_position(&mut self, col: u8, row: u8) {
        self.ops.push(Op::SetPos(col, row)).ok();
    }

    fn clear(&mut self) {
        self.ops.push(Op::Clear).ok();
    }

    fn reset_custom_chars(&mut self) {
        // Slot table is reset in flush(), so by the time the next
        // frame starts here, slots are already empty. No HD44780-side
        // op needed — overwriting a slot in CGRAM is the same as
        // freshly loading it.
        self.slots = [None; 8];
        self.next_slot = 0;
    }
}
