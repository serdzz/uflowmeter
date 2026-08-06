//! Tests for the XMODEM-CRC receive state machine.

use crate::xmodem_lib::*;

const SOH: u8 = 0x01;
const EOT: u8 = 0x04;
const CAN: u8 = 0x18;

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for b in data {
        crc ^= (*b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// One 128-byte SOH packet: header, sequence, complement, data, CRC.
fn packet(seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut data = payload.to_vec();
    data.resize(128, 0x1A); // XMODEM pads with SUB
    let crc = crc16(&data);
    let mut p = vec![SOH, seq, 0xFF - seq];
    p.extend_from_slice(&data);
    p.extend_from_slice(&crc.to_be_bytes());
    p
}

/// Feed a byte stream, returning every non-None response in order.
fn drive<S: Sink>(rx: &mut XModemReceiver<S>, bytes: &[u8]) -> Vec<Response> {
    bytes
        .iter()
        .map(|b| rx.feed(*b))
        .filter(|r| *r != Response::None)
        .collect()
}

#[test]
fn starts_out_polling_for_crc_mode() {
    let mut buf = [0u8; 56];
    let rx = XModemReceiver::new(SliceSink::new(&mut buf));
    assert!(rx.is_waiting(), "must ask the sender for CRC mode");
    assert!(!rx.is_finished());
    assert_eq!(POLL, 0x43, "'C' selects CRC mode, not checksum mode");
}

#[test]
fn accepts_a_good_packet_and_stores_the_payload() {
    let mut buf = [0u8; 56];
    let payload: Vec<u8> = (0..56).collect();
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));

    let out = drive(&mut rx, &packet(1, &payload));
    assert_eq!(out, vec![Response::Ack]);
    assert_eq!(rx.written(), 56, "buffer fills, padding is discarded");
    drop(rx);
    assert_eq!(&buf[..], &payload[..]);
}

#[test]
fn eot_completes_the_transfer() {
    let mut buf = [0u8; 56];
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    drive(&mut rx, &packet(1, &[0xAB; 56]));
    assert_eq!(rx.feed(EOT), Response::Ack);
    assert!(rx.is_finished());
    assert!(rx.is_complete());
}

#[test]
fn a_corrupt_packet_is_naked_and_leaves_no_partial_data() {
    let mut buf = [0u8; 56];
    let mut good = packet(1, &[0x11; 56]);
    // Flip a payload byte so the trailing CRC no longer matches.
    good[10] ^= 0xFF;

    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    let out = drive(&mut rx, &good);
    assert_eq!(out, vec![Response::Nak]);
    assert_eq!(
        rx.written(),
        0,
        "a rejected packet must not advance the write cursor"
    );
}

#[test]
fn a_resend_after_nak_lands_correctly() {
    let mut buf = [0u8; 56];
    let payload = [0x5A; 56];
    let mut corrupt = packet(1, &payload);
    corrupt[10] ^= 0xFF;

    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    assert_eq!(drive(&mut rx, &corrupt), vec![Response::Nak]);
    // Sender repeats the same sequence number.
    assert_eq!(drive(&mut rx, &packet(1, &payload)), vec![Response::Ack]);
    assert_eq!(rx.written(), 56);
    drop(rx);
    assert_eq!(&buf[..], &payload[..]);
}

#[test]
fn a_stale_sequence_number_is_ignored() {
    let mut buf = [0u8; 128];
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    drive(&mut rx, &packet(1, &[0x01; 56]));
    // Sender missed our ACK and repeats packet 1; we already moved on
    // to expecting 2, so it must not be written again.
    let before = rx.written();
    let out = drive(&mut rx, &packet(1, &[0x02; 56]));
    assert!(out.is_empty(), "no response to a duplicate packet");
    assert_eq!(rx.written(), before);
}

#[test]
fn a_bad_sequence_complement_is_rejected() {
    let mut buf = [0u8; 56];
    let mut p = packet(1, &[0x33; 56]);
    p[2] = 0x00; // complement should be 0xFE
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    let out = drive(&mut rx, &p);
    assert!(out.is_empty());
    assert_eq!(rx.written(), 0);
}

#[test]
fn cancel_ends_the_transfer_without_completing_it() {
    let mut buf = [0u8; 56];
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    assert_eq!(rx.feed(CAN), Response::CancelAck);
    assert!(rx.is_finished());
    assert!(!rx.is_complete(), "a cancelled transfer is not a good one");
}

#[test]
fn payload_larger_than_the_buffer_is_truncated_not_overflowed() {
    // 20-byte TDC register block: the sender still pads to 128.
    let mut buf = [0u8; 20];
    let payload: Vec<u8> = (0..128).map(|i| i as u8).collect();
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));

    assert_eq!(drive(&mut rx, &packet(1, &payload)), vec![Response::Ack]);
    assert_eq!(rx.written(), 20);
    drop(rx);
    assert_eq!(&buf[..], &payload[..20]);
}

#[test]
fn two_packets_land_back_to_back() {
    // Guards the offset bug the C++ carries: it counts each packet's
    // CRC bytes as payload, so packet 2 would start two bytes late.
    let mut buf = [0u8; 256];
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));

    assert_eq!(
        drive(&mut rx, &packet(1, &[0xAA; 128])),
        vec![Response::Ack]
    );
    assert_eq!(
        drive(&mut rx, &packet(2, &[0xBB; 128])),
        vec![Response::Ack]
    );
    assert_eq!(rx.written(), 256);
    drop(rx);
    assert_eq!(buf[127], 0xAA);
    assert_eq!(buf[128], 0xBB, "second packet starts at 128, not 130");
}

#[test]
fn noise_between_packets_is_ignored() {
    let mut buf = [0u8; 56];
    let mut rx = XModemReceiver::new(SliceSink::new(&mut buf));
    // Line noise while waiting must not be mistaken for a header.
    assert_eq!(drive(&mut rx, &[0x00, 0x7F, 0xFE]), vec![]);
    assert!(rx.is_waiting());
    assert_eq!(drive(&mut rx, &packet(1, &[0x77; 56])), vec![Response::Ack]);
}
