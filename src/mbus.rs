//! M-Bus protocol implementation (slave, periodic datagram)
//!
//! Sends RSP_U datagram periodically over USART1.
//! Matches C++ mbus.cpp behavior — no request/response,
//! just broadcasts meter data at configured intervals.

use heapless::Vec;

/// M-Bus frame buffer size
const FRAME_BUF: usize = 128;

/// Build an M-Bus RSP_U datagram with current meter data.
/// Returns the complete frame bytes ready to send.
///
/// Frame structure (EN 13757-3):
///   68 L L 68 C A CI [data...] CS 16
pub fn build_datagram(
    slave_address: u8,
    serial_number: u32,
    total_volume: f32,
    flow_rate: f32,
    uptime_minutes: u32,
) -> Vec<u8, FRAME_BUF> {
    let mut frame: Vec<u8, FRAME_BUF> = Vec::new();

    // Header: 68 L L 68
    frame.push(0x68).ok();
    frame.push(0).ok(); // L placeholder
    frame.push(0).ok(); // L placeholder
    frame.push(0x68).ok();

    // Control field: SND_NK (send without acknowledge)
    frame.push(0x08).ok();
    // Address
    frame.push(slave_address).ok();
    // CI: RSP_UD (respond user data)
    frame.push(0x72).ok();

    // --- Data records ---
    // Serial number (BCD)
    let serial_bcd = dec_to_bcd32(serial_number);
    push_le32(&mut frame, serial_bcd);
    // Manufacturer ID: ELK = 0x158B (little-endian)
    push_le16(&mut frame, 0x8B15);
    // Version
    frame.push(0x1F).ok();
    // Medium: cold water (0x16)
    frame.push(0x16).ok();
    // Access number
    frame.push(0x00).ok();
    // Error message
    frame.push(0x00).ok();
    // Reserved
    push_le16(&mut frame, 0x0000);

    // Data block: volume m³ (DIF=0x04 32bit, VIF=0x13 volume m³)
    frame.push(0x04).ok();
    frame.push(0x13).ok();
    push_le32(&mut frame, total_volume.to_bits());

    // Data block: volume flow m³/h (DIF=0x04 32bit, VIF=0x3B flow m³/h)
    frame.push(0x04).ok();
    frame.push(0x3B).ok();
    push_le32(&mut frame, flow_rate.to_bits());

    // Data block: uptime minutes (DIF=0x04 32bit, VIF=0x21 time)
    frame.push(0x04).ok();
    frame.push(0x21).ok();
    push_le32(&mut frame, uptime_minutes);

    // Fix length fields L
    let data_len = frame.len() - 4; // minus 68 L L 68
    if data_len <= 255 {
        frame[1] = data_len as u8;
        frame[2] = data_len as u8;
    }

    // Checksum (sum of bytes from C to last data byte)
    let checksum = frame[4..].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    frame.push(checksum).ok();
    // End marker
    frame.push(0x16).ok();

    frame
}

/// Convert decimal u32 to BCD (for serial number)
fn dec_to_bcd32(mut dec: u32) -> u32 {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    while dec > 0 {
        result |= (dec % 10) << shift;
        dec /= 10;
        shift += 4;
    }
    result
}

fn push_le32(buf: &mut Vec<u8, FRAME_BUF>, val: u32) {
    buf.push(val as u8).ok();
    buf.push((val >> 8) as u8).ok();
    buf.push((val >> 16) as u8).ok();
    buf.push((val >> 24) as u8).ok();
}

fn push_le16(buf: &mut Vec<u8, FRAME_BUF>, val: u16) {
    buf.push(val as u8).ok();
    buf.push((val >> 8) as u8).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcd_conversion() {
        assert_eq!(dec_to_bcd32(12345), 0x00012345);
        assert_eq!(dec_to_bcd32(0), 0);
        assert_eq!(dec_to_bcd32(99999999), 0x99999999);
    }

    #[test]
    fn test_datagram_structure() {
        let frame = build_datagram(1, 12345, 100.0_f32, 0.5_f32, 120);
        // Starts with 68 L L 68
        assert_eq!(frame[0], 0x68);
        assert_eq!(frame[3], 0x68);
        // Ends with checksum + 0x16
        assert_eq!(*frame.last().unwrap(), 0x16);
        // Control field
        assert_eq!(frame[4], 0x08);
        // Address
        assert_eq!(frame[5], 1);
        // CI
        assert_eq!(frame[6], 0x72);
    }

    #[test]
    fn test_datagram_serial_bcd() {
        let frame = build_datagram(1, 12345, 0.0_f32, 0.0_f32, 0);
        // Serial at bytes 7-10 (little-endian BCD)
        let serial = u32::from_le_bytes([frame[7], frame[8], frame[9], frame[10]]);
        assert_eq!(serial, 0x00012345);
    }

    #[test]
    fn test_checksum_is_valid() {
        let frame = build_datagram(5, 999, 42.0_f32, 1.5_f32, 60);
        // Checksum = sum of bytes [4..n-2]
        let data_end = frame.len() - 2;
        let expected_checksum: u8 = frame[4..data_end]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b));
        assert_eq!(frame[data_end], expected_checksum);
    }
}