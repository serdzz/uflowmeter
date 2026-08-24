//! Receive an encrypted firmware image into flash slot B.
//!
//! Implements `xmodem_lib::Sink`, so the XMODEM state machine streams
//! straight into flash. It has to: a 120 KiB image cannot be assembled
//! in RAM on a part with 32 KiB.
//!
//! # The header is written last
//!
//! An update image is `header || ciphertext`, and that is the order it
//! arrives in — but it is not the order it is stored in. The header page
//! is held in RAM for the whole transfer and only written once the body
//! is complete.
//!
//! This is what makes the bootloader's state model work. Over there,
//! "an update is pending" *is* "slot B holds a header that parses";
//! there is no separate flag that could disagree with reality. Writing
//! the header first would break that: a transfer cut short by a reset
//! would leave a valid header in front of a truncated body, and the
//! bootloader would spend a boot authenticating an image that was never
//! going to pass. Writing it last means an interrupted transfer leaves
//! slot B headerless, which reads as "nothing staged" — the truth.
//!
//! # The header is also the length check
//!
//! Parsing it as soon as its 256 bytes have arrived gives `payload_len`
//! before any of the body is stored, so a file that is not an update
//! image is rejected after 256 bytes rather than after 120 KiB, and the
//! trailing padding XMODEM adds to its last packet is dropped rather
//! than written.

use embassy_stm32::flash::{Blocking, Flash};
use fwimage::{layout, Header, HeaderError, HEADER_LEN};
use uflowmeter::xmodem_lib::{Sink, SinkError};

const PAGE: usize = layout::PAGE as usize;

/// Why staging failed. Distinguished so the operator is told which,
/// rather than a bare "upload failed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum StageError {
    /// The first 256 bytes are not a usable image header.
    BadHeader,
    /// The sender stopped before delivering `payload_len` bytes.
    Truncated,
    /// A flash erase or write failed.
    Flash,
}

pub struct SlotBWriter<'d> {
    flash: &'d mut Flash<'static, Blocking>,
    /// Held back until `finish`. See the module comment.
    header: [u8; HEADER_LEN],
    header_len: usize,
    /// Body length promised by the header; zero until it has parsed.
    payload_len: usize,
    /// Body bytes committed to flash plus those pending in `page`.
    body_len: usize,
    page: [u8; PAGE],
    page_len: usize,
}

impl<'d> SlotBWriter<'d> {
    /// Erase slot B and get ready to receive.
    ///
    /// The erase happens here, before the caller starts polling the
    /// sender, because it takes a noticeable fraction of a second —
    /// 480 pages — and doing it mid-transfer would stall the line long
    /// enough to time the sender out.
    pub fn new(flash: &'d mut Flash<'static, Blocking>) -> Result<Self, StageError> {
        let base = layout::SLOT_B - layout::FLASH_BASE;
        flash
            .blocking_erase(base, base + layout::SLOT_LEN)
            .map_err(|_| StageError::Flash)?;
        Ok(Self {
            flash,
            header: [0u8; HEADER_LEN],
            header_len: 0,
            payload_len: 0,
            body_len: 0,
            page: [0u8; PAGE],
            page_len: 0,
        })
    }

    /// Absolute flash offset of the body byte at `index`.
    fn body_offset(index: usize) -> u32 {
        layout::SLOT_B - layout::FLASH_BASE + HEADER_LEN as u32 + index as u32
    }

    /// Buffer body bytes, flushing whole pages as they fill.
    fn push_body(&mut self, mut data: &[u8]) -> Result<(), StageError> {
        while !data.is_empty() {
            let room = PAGE - self.page_len;
            let take = room.min(data.len());
            self.page[self.page_len..self.page_len + take].copy_from_slice(&data[..take]);
            self.page_len += take;
            self.body_len += take;
            data = &data[take..];

            if self.page_len == PAGE {
                let at = Self::body_offset(self.body_len - PAGE);
                self.flash
                    .blocking_write(at, &self.page)
                    .map_err(|_| StageError::Flash)?;
                self.page_len = 0;
            }
        }
        Ok(())
    }

    /// Flush the tail, check the body is complete, then commit the
    /// header — which is the act that marks the update as pending.
    pub fn finish(mut self) -> Result<Header, StageError> {
        if self.header_len < HEADER_LEN || self.body_len != self.payload_len {
            return Err(StageError::Truncated);
        }

        if self.page_len > 0 {
            // Pad the final partial page up to a whole flash word. The
            // header's payload_len is a multiple of the write size, so
            // this only ever pads out to the page, never into the
            // image's own bytes.
            let padded =
                self.page_len.div_ceil(layout::WRITE_SIZE as usize) * layout::WRITE_SIZE as usize;
            self.page[self.page_len..padded].fill(0xFF);
            let at = Self::body_offset(self.body_len - self.page_len);
            self.flash
                .blocking_write(at, &self.page[..padded])
                .map_err(|_| StageError::Flash)?;
            self.page_len = 0;
        }

        let base = layout::SLOT_B - layout::FLASH_BASE;
        self.flash
            .blocking_write(base, &self.header)
            .map_err(|_| StageError::Flash)?;

        Header::parse(&self.header, layout::MAX_PAYLOAD, layout::WRITE_SIZE)
            .map_err(|_| StageError::BadHeader)
    }
}

impl Sink for SlotBWriter<'_> {
    fn write(&mut self, _offset: usize, data: &[u8]) -> Result<usize, SinkError> {
        let mut rest = data;
        let mut taken = 0usize;

        if self.header_len < HEADER_LEN {
            let n = (HEADER_LEN - self.header_len).min(rest.len());
            self.header[self.header_len..self.header_len + n].copy_from_slice(&rest[..n]);
            self.header_len += n;
            taken += n;
            rest = &rest[n..];

            if self.header_len == HEADER_LEN {
                match Header::parse(&self.header, layout::MAX_PAYLOAD, layout::WRITE_SIZE) {
                    Ok(h) => self.payload_len = h.payload_len as usize,
                    Err(e) => {
                        defmt::error!("update: not an image header ({=str})", describe(e));
                        return Err(SinkError);
                    }
                }
            }
        }

        // Accept exactly what the header promised. Everything past that
        // is the padding XMODEM adds to round its last packet up, and
        // is dropped rather than stored — reporting it as untaken also
        // keeps the receiver's byte count honest.
        let wanted = self.payload_len.saturating_sub(self.body_len);
        let n = wanted.min(rest.len());
        if n > 0 && self.push_body(&rest[..n]).is_err() {
            return Err(SinkError);
        }
        taken += n;

        Ok(taken)
    }
}

fn describe(e: HeaderError) -> &'static str {
    match e {
        HeaderError::Truncated => "short",
        HeaderError::Magic => "no magic — is this an .ufw file?",
        HeaderError::Format => "unsupported format version",
        HeaderError::Crc => "header CRC mismatch",
        HeaderError::PayloadLen => "declared size does not fit the slot",
    }
}
