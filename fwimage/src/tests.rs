//! Vectors here were produced by an independent AES-256-GCM
//! implementation (Python `cryptography`, i.e. OpenSSL) over exactly
//! the AAD this crate builds — see `Header::aad`. Checking against our
//! own output would only prove self-consistency, which is worth
//! nothing: an image the bootloader accepts has to be one a standard
//! GCM implementation produced.

use super::*;

// A: payload_len=64 version=1 — whole blocks.
const A_KEY: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];
const A_NONCE: [u8; 12] = [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111];
const A_TAG: &str = "c662ca1ade3898db808d847456592c9d";
const A_CT: &str = "4b11cf7e66cf7baa052016b88d3b0f9131b8878204fa6ed60c6315883c6d70947703b138b3149610e6d1510fca7c8f834801f98f4f3c3cac71633576482f1036";

// B: payload_len=100 version=7 — deliberately not a multiple of 16, so
// the GHASH tail has to be zero-padded rather than absorbed whole.
const B_KEY: [u8; 32] = [
    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56,
    57, 58, 59, 60, 61, 62, 63, 64,
];
const B_NONCE: [u8; 12] = [200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211];
const B_TAG: &str = "3f073b94aa2a53205842a6a041a85415";
const B_CT: &str = "fb1b92a8c4ee7891f61f93773e7b0c194a2445d056f6a3765c51c31090f6e85bdc6e6a579cde1ab87a7a4b22c3bdbb373b4415fd0eb82405875c8bd94f78f84cd67aa60d8673c496fd83433668340be65709d3d2f31e27badf0b5363ff9d8cc43bae0049";

// C: payload_len=1024 version=42 — long enough that the CTR counter
// advances well past the first block.
const C_KEY: [u8; 32] = [
    200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218,
    219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231,
];
const C_NONCE: [u8; 12] = [5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
const C_TAG: &str = "19181b8dc636059e227c5df5dd97be1d";

/// Same generator the vectors were built from.
fn plaintext(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 7 + 3) % 256) as u8).collect()
}

fn header(key_len: u32, version: u32, nonce: [u8; 12], tag_hex: &str) -> Header {
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&hex::decode(tag_hex).unwrap());
    Header {
        payload_len: key_len,
        image_version: version,
        nonce,
        tag,
    }
}

// ── GCM against the reference implementation ──────────────────────────

#[test]
fn encrypts_to_the_reference_ciphertext() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let mut buf = plaintext(64);
    Decryptor::new(&A_KEY, &h).apply(&mut buf);
    assert_eq!(hex::encode(&buf), A_CT);
}

#[test]
fn produces_the_reference_tag() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert_eq!(hex::encode(v.finish()), A_TAG);
}

#[test]
fn decrypts_the_reference_ciphertext_back() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let mut buf = hex::decode(A_CT).unwrap();
    Decryptor::new(&A_KEY, &h).apply(&mut buf);
    assert_eq!(buf, plaintext(64));
}

#[test]
fn verifies_the_reference_tag() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert!(v.verify(&h.tag));
}

/// The payload length is not a multiple of the 16-byte GHASH block, so
/// this is the case that catches a missing final zero-pad.
#[test]
fn handles_a_payload_that_is_not_a_block_multiple() {
    let h = header(100, 7, B_NONCE, B_TAG);
    let mut buf = plaintext(100);
    Decryptor::new(&B_KEY, &h).apply(&mut buf);
    assert_eq!(hex::encode(&buf), B_CT);

    let mut v = Verifier::new(&B_KEY, &h);
    v.update(&buf);
    assert_eq!(hex::encode(v.finish()), B_TAG);
}

/// 1 KiB drives the CTR counter past 64 blocks — a 32-bit counter that
/// failed to increment, or incremented in the wrong endianness, would
/// still pass the 64-byte case above.
#[test]
fn handles_a_payload_spanning_many_counter_blocks() {
    let h = header(1024, 42, C_NONCE, C_TAG);
    let mut buf = plaintext(1024);
    Decryptor::new(&C_KEY, &h).apply(&mut buf);

    let mut v = Verifier::new(&C_KEY, &h);
    v.update(&buf);
    assert!(v.verify(&h.tag));
}

// ── Streaming ─────────────────────────────────────────────────────────

/// The bootloader feeds flash a page at a time and the last page is
/// short, so chunk boundaries land anywhere. Every chunking has to
/// produce the tag the packer computed in one pass.
#[test]
fn chunking_does_not_change_the_tag() {
    let h = header(1024, 42, C_NONCE, C_TAG);
    let mut ct = plaintext(1024);
    Decryptor::new(&C_KEY, &h).apply(&mut ct);

    // 1 and 3 are the awkward ones: neither divides 16, so the residual
    // buffer is partially full across nearly every call.
    for chunk in [1usize, 3, 7, 16, 17, 64, 256, 1024] {
        let mut v = Verifier::new(&C_KEY, &h);
        for part in ct.chunks(chunk) {
            v.update(part);
        }
        assert!(
            v.verify(&h.tag),
            "chunk size {chunk} produced a different tag"
        );
    }
}

#[test]
fn chunked_decryption_matches_one_shot() {
    let h = header(1024, 42, C_NONCE, C_TAG);
    let mut one_shot = plaintext(1024);
    Decryptor::new(&C_KEY, &h).apply(&mut one_shot);

    let mut chunked = plaintext(1024);
    let mut d = Decryptor::new(&C_KEY, &h);
    for part in chunked.chunks_mut(30) {
        d.apply(part);
    }
    assert_eq!(one_shot, chunked);
}

// ── Rejection ─────────────────────────────────────────────────────────

#[test]
fn rejects_a_flipped_ciphertext_bit() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let mut ct = hex::decode(A_CT).unwrap();
    ct[30] ^= 0x01;
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert!(!v.verify(&h.tag));
}

/// The whole reason payload_len and image_version are in the AAD: a
/// genuine ciphertext replayed under a different declared length must
/// not authenticate.
#[test]
fn rejects_a_tampered_length_in_the_aad() {
    let mut h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    h.payload_len = 48;
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert!(!v.verify(&h.tag));
}

#[test]
fn rejects_a_tampered_version_in_the_aad() {
    let mut h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    h.image_version = 2;
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert!(!v.verify(&h.tag));
}

#[test]
fn rejects_the_wrong_key() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    let mut wrong = A_KEY;
    wrong[31] ^= 0x01;
    let mut v = Verifier::new(&wrong, &h);
    v.update(&ct);
    assert!(!v.verify(&h.tag));
}

#[test]
fn rejects_a_tag_that_is_one_byte_off() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let ct = hex::decode(A_CT).unwrap();
    let mut near = h.tag;
    near[15] ^= 0x01;
    let mut v = Verifier::new(&A_KEY, &h);
    v.update(&ct);
    assert!(!v.verify(&near));
}

// ── Header ────────────────────────────────────────────────────────────

const SLOT: u32 = 120 * 1024 - HEADER_LEN as u32;
const WRITE: u32 = 4;

fn serialised(h: &Header) -> [u8; HEADER_LEN] {
    let mut raw = [0u8; HEADER_LEN];
    h.write(&mut raw);
    raw
}

#[test]
fn header_survives_a_round_trip() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert_eq!(Header::parse(&raw, SLOT, WRITE), Ok(h));
}

#[test]
fn header_is_padded_to_a_full_erase_page() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert_eq!(raw.len(), 256);
    assert!(raw[48..].iter().all(|b| *b == 0));
}

/// An erased slot reads as all-ones. That is the ordinary "nothing
/// staged" case and must be distinguishable from corruption.
#[test]
fn an_erased_slot_reports_missing_magic() {
    let raw = [0xFFu8; HEADER_LEN];
    assert_eq!(Header::parse(&raw, SLOT, WRITE), Err(HeaderError::Magic));
}

#[test]
fn a_blank_slot_reports_missing_magic() {
    let raw = [0x00u8; HEADER_LEN];
    assert_eq!(Header::parse(&raw, SLOT, WRITE), Err(HeaderError::Magic));
}

#[test]
fn rejects_a_short_buffer() {
    let raw = [0u8; HEADER_LEN - 1];
    assert_eq!(
        Header::parse(&raw, SLOT, WRITE),
        Err(HeaderError::Truncated)
    );
}

#[test]
fn rejects_an_unknown_format_version() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let mut raw = serialised(&h);
    raw[4] = 0xFF;
    assert_eq!(Header::parse(&raw, SLOT, WRITE), Err(HeaderError::Format));
}

/// A header torn by a power cut mid-write can carry a valid magic and a
/// garbage length. The CRC is checked before the length is trusted, so
/// this reports Crc rather than PayloadLen.
#[test]
fn rejects_a_torn_header_before_trusting_its_length() {
    let h = header(64, 1, A_NONCE, A_TAG);
    let mut raw = serialised(&h);
    raw[OFF_PAYLOAD_LEN] = 0xFF;
    raw[OFF_PAYLOAD_LEN + 1] = 0xFF;
    assert_eq!(Header::parse(&raw, SLOT, WRITE), Err(HeaderError::Crc));
}

#[test]
fn rejects_a_payload_larger_than_the_slot() {
    let h = header(SLOT + WRITE, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert_eq!(
        Header::parse(&raw, SLOT, WRITE),
        Err(HeaderError::PayloadLen)
    );
}

#[test]
fn accepts_a_payload_that_exactly_fills_the_slot() {
    let h = header(SLOT, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert!(Header::parse(&raw, SLOT, WRITE).is_ok());
}

#[test]
fn rejects_an_empty_payload() {
    let h = header(0, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert_eq!(
        Header::parse(&raw, SLOT, WRITE),
        Err(HeaderError::PayloadLen)
    );
}

/// Flash programs whole words, so a length that is not a multiple of
/// the write size could not be flashed without inventing padding.
#[test]
fn rejects_a_payload_that_is_not_a_whole_number_of_words() {
    let h = header(66, 1, A_NONCE, A_TAG);
    let raw = serialised(&h);
    assert_eq!(
        Header::parse(&raw, SLOT, WRITE),
        Err(HeaderError::PayloadLen)
    );
}

// ── Layout ────────────────────────────────────────────────────────────

#[test]
fn layout_is_consistent() {
    use layout::*;
    assert_eq!(SLOT_A, FLASH_BASE + BOOTLOADER_LEN);
    assert_eq!(SLOT_B, SLOT_A + SLOT_LEN);
    // The three regions must tile the part's flash exactly — a gap is
    // wasted space, an overlap is a bootloader that erases itself.
    assert_eq!(BOOTLOADER_LEN + 2 * SLOT_LEN, FLASH_LEN);
    assert_eq!(SLOT_B + SLOT_LEN, FLASH_BASE + FLASH_LEN);
    // Every region boundary has to land on an erase page.
    for boundary in [BOOTLOADER_LEN, SLOT_A, SLOT_B, SLOT_LEN] {
        assert_eq!(boundary % PAGE, 0);
    }
    // The header occupies whole pages so ciphertext starts page-aligned.
    assert_eq!(HEADER_LEN as u32 % PAGE, 0);
    assert_eq!(MAX_PAYLOAD % WRITE_SIZE, 0);
}

#[test]
fn crc32_matches_the_known_check_value() {
    // The standard CRC-32/ISO-HDLC check value for "123456789".
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}
