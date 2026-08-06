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

/// Largest XMODEM packet, i.e. the STX form.
const MAX_PACKET: usize = 1024;

/// A sink refused the data: out of room, or the underlying store
/// failed. A named type rather than `()` so the reason has somewhere to
/// grow, and so the signature says what it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkError;

/// Where a received transfer is put.
///
/// Exists so a transfer can go straight to flash instead of to RAM: a
/// firmware image is far larger than this device's 32 KiB, so it cannot
/// be assembled in a buffer first.
pub trait Sink {
    /// Store `data` at `offset` bytes from the start of the transfer.
    ///
    /// Returns how many bytes were taken. A sink with a fixed capacity
    /// may take fewer than offered, which is how the padding XMODEM
    /// adds to the final packet gets dropped. Returning `Err` aborts
    /// the transfer.
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, SinkError>;
}

/// A sink backed by a caller-provided buffer — the original behaviour,
/// used by the fixed-size configuration uploads. Silently stops
/// accepting once full.
pub struct SliceSink<'a> {
    buf: &'a mut [u8],
}

impl<'a> SliceSink<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf }
    }
}

impl Sink for SliceSink<'_> {
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, SinkError> {
        let room = self.buf.len().saturating_sub(offset);
        let take = room.min(data.len());
        self.buf[offset..offset + take].copy_from_slice(&data[..take]);
        Ok(take)
    }
}

pub struct XModemReceiver<S: Sink> {
    sink: S,
    /// The packet being received. Held whole rather than passed through
    /// byte by byte because a packet is only known to be good once its
    /// CRC has been checked, and a sink writing to flash cannot take
    /// back what a failed packet already handed it. The earlier
    /// buffer-backed version rewound its write cursor on a NAK; flash
    /// has no rewind, so the packet is staged here and released only
    /// when it verifies.
    packet: [u8; MAX_PACKET],
    state: State,
    /// Bytes accepted by the sink so far. Kept separate from the
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

impl<S: Sink> XModemReceiver<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            packet: [0u8; MAX_PACKET],
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

    /// Take the sink back once the transfer is over, so the caller can
    /// finalise it — for the flash sink that is where the header page,
    /// held in RAM throughout, finally gets committed.
    pub fn into_sink(self) -> S {
        self.sink
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

        // Data bytes are staged in `packet`; the two CRC bytes that
        // follow are folded into the checksum only.
        if self.packet_pos < self.packet_size {
            self.packet[self.packet_pos] = byte;
        }
        self.packet_pos += 1;

        if self.packet_pos < self.packet_size + 2 {
            return Response::None;
        }

        self.state = State::Waiting;
        if self.crc != 0 {
            // Nothing was handed to the sink, so there is nothing to
            // undo — the sender simply repeats the packet.
            return Response::Nak;
        }

        match self
            .sink
            .write(self.written, &self.packet[..self.packet_size])
        {
            Ok(taken) => {
                self.written += taken;
                self.sequence = self.sequence.wrapping_add(1);
                Response::Ack
            }
            Err(SinkError) => {
                // The destination refused the data — out of room, or a
                // flash write failed. Cancel rather than acknowledge a
                // packet that was not stored.
                self.state = State::Cancelled;
                Response::CancelAck
            }
        }
    }
}
