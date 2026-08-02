//! XMODEM-CRC receive state machine — pure, no HAL, host-testable.
//!
//! Port of the C++ `XModem` (`UFlowMeter_c++/UFlowMeter/hardware/
//! xmodem.cpp`), which the meter uses to upload configuration over the
//! shell rather than firmware: 56 bytes of calibration (14 f32) or the
//! 20-byte TDC register block.
//!
//! The transport is left to the caller. Feed received bytes to
//! `feed()` and send whatever `Response` it hands back; call `poll()`
//! on a timer while `is_waiting()` so the sender knows we want CRC
//! mode.

/// Start-of-header: a 128-byte data packet follows.
const SOH: u8 = 0x01;
/// Start-of-text: a 1024-byte data packet follows.
const STX: u8 = 0x02;
const EOT: u8 = 0x04;
pub const ACK: u8 = 0x06;
pub const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
/// 'C' — asks the sender for CRC mode rather than checksum mode.
pub const POLL: u8 = 0x43;

/// Retries before `read` gives up, matching the C++ `RETRIES`.
pub const RETRIES: u8 = 15;

/// What the caller should transmit after feeding a byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// Nothing to send.
    None,
    /// Packet accepted.
    Ack,
    /// Packet rejected; the sender should repeat it.
    Nak,
    /// Sender cancelled — echo two CANs back.
    CancelAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Between packets: waiting for SOH / STX / EOT / CAN.
    Waiting,
    /// Expecting the sequence number.
    Sequence1,
    /// Expecting its complement.
    Sequence2,
    /// Reading packet data plus the two CRC bytes.
    Receiving,
    Finished,
    Cancelled,
}

/// Running CRC-16/XMODEM: poly 0x1021, init 0, no reflection. Feeding
/// the two trailing CRC bytes through it leaves zero on success, which
/// is the check the C++ makes via `crc_.finish() == 0`.
fn crc16_update(crc: u16, byte: u8) -> u16 {
    let mut crc = crc ^ ((byte as u16) << 8);
    for _ in 0..8 {
        if crc & 0x8000 != 0 {
            crc = (crc << 1) ^ 0x1021;
        } else {
            crc <<= 1;
        }
    }
    crc
}

pub struct XModemReceiver<'a> {
    buf: &'a mut [u8],
    state: State,
    /// Bytes of payload written so far. Kept separate from the
    /// per-packet counter below: the C++ uses one cumulative counter
    /// for both, so its second and later packets land two bytes late,
    /// having counted the previous packet's CRC as payload. Harmless
    /// there because every transfer it performs fits one packet, but
    /// not worth reproducing.
    written: usize,
    /// Bytes consumed within the current packet, CRC included.
    packet_pos: usize,
    packet_size: usize,
    crc: u16,
    sequence: u8,
}

impl<'a> XModemReceiver<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            state: State::Waiting,
            written: 0,
            packet_pos: 0,
            packet_size: 0,
            crc: 0,
            sequence: 1,
        }
    }

    /// True while the sender has not started a packet, i.e. when the
    /// caller should keep emitting `POLL`.
    pub fn is_waiting(&self) -> bool {
        self.state == State::Waiting
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, State::Finished | State::Cancelled)
    }

    /// True only for a transfer the sender ended with EOT.
    pub fn is_complete(&self) -> bool {
        self.state == State::Finished
    }

    /// Payload bytes stored so far.
    pub fn written(&self) -> usize {
        self.written
    }

    pub fn feed(&mut self, byte: u8) -> Response {
        match self.state {
            State::Waiting => self.on_waiting(byte),
            State::Sequence1 => {
                // A packet whose sequence number does not match ours is
                // a repeat of one already accepted; drop back to
                // waiting rather than corrupting the stream.
                self.state = if byte == self.sequence {
                    State::Sequence2
                } else {
                    State::Waiting
                };
                Response::None
            }
            State::Sequence2 => {
                if 0xFF - byte == self.sequence {
                    self.crc = 0;
                    self.packet_pos = 0;
                    self.state = State::Receiving;
                } else {
                    self.state = State::Waiting;
                }
                Response::None
            }
            State::Receiving => self.on_receiving(byte),
            State::Finished | State::Cancelled => Response::None,
        }
    }

    fn on_waiting(&mut self, byte: u8) -> Response {
        match byte {
            SOH => {
                self.packet_size = 128;
                self.state = State::Sequence1;
                Response::None
            }
            STX => {
                self.packet_size = 1024;
                self.state = State::Sequence1;
                Response::None
            }
            EOT => {
                self.state = State::Finished;
                Response::Ack
            }
            CAN => {
                self.state = State::Cancelled;
                Response::CancelAck
            }
            _ => Response::None,
        }
    }

    fn on_receiving(&mut self, byte: u8) -> Response {
        self.crc = crc16_update(self.crc, byte);

        // Data bytes go to the caller's buffer; the two CRC bytes that
        // follow are folded into the checksum only.
        if self.packet_pos < self.packet_size && self.written < self.buf.len() {
            self.buf[self.written] = byte;
            self.written += 1;
        }
        self.packet_pos += 1;

        if self.packet_pos < self.packet_size + 2 {
            return Response::None;
        }

        self.state = State::Waiting;
        if self.crc == 0 {
            self.sequence = self.sequence.wrapping_add(1);
            Response::Ack
        } else {
            // Rewind whatever this packet contributed so the resend
            // overwrites it instead of appending.
            self.written = self
                .written
                .saturating_sub(self.packet_size.min(self.buf.len()));
            Response::Nak
        }
    }
}
