//! The AES-256 key the bootloader trusts.
//!
//! One key for the whole fleet, compiled in — the arrangement chosen
//! for this product. Two consequences worth being explicit about:
//!
//!   * **The key is in flash.** Anyone who can read the device's flash
//!     has it, and with it the ability to sign an image every unit will
//!     accept. Readout protection (RDP Level 1) is therefore not
//!     optional here; without it the key is a `probe-rs read` away.
//!   * **Compromise does not stay local.** One extracted key is every
//!     unit in the field. If that is unacceptable later, moving to a
//!     per-device key provisioned at manufacture changes only this
//!     module and adds a provisioning step.
//!
//! The key is supplied at build time through `UFW_AES_KEY` (64 hex
//! characters). A build with no key set **fails** rather than falling
//! back to something insecure — a default key that ships by accident is
//! worse than a build error, because nothing about the resulting
//! firmware looks wrong.

use fwimage::KEY_LEN;

#[cfg(not(feature = "dev-key"))]
pub const KEY: [u8; KEY_LEN] = parse_hex(env!(
    "UFW_AES_KEY",
    "UFW_AES_KEY is not set. Generate a key with `imgtool keygen`, then \
     build with UFW_AES_KEY=<64 hex chars>. To build for the bench with a \
     well-known key instead, use --features dev-key."
));

/// Well-known key for bench work. Guarded behind a feature so it cannot
/// end up in a release build by omission — you have to ask for it.
#[cfg(feature = "dev-key")]
pub const KEY: [u8; KEY_LEN] =
    parse_hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");

/// Decode 64 hex characters at compile time.
///
/// `const` so a malformed key is a build failure with a readable
/// message, not a device that silently rejects every image.
const fn parse_hex(hex: &str) -> [u8; KEY_LEN] {
    let bytes = hex.as_bytes();
    assert!(
        bytes.len() == KEY_LEN * 2,
        "UFW_AES_KEY must be exactly 64 hex characters (32 bytes)"
    );
    let mut out = [0u8; KEY_LEN];
    let mut i = 0;
    while i < KEY_LEN {
        out[i] = (nibble(bytes[i * 2]) << 4) | nibble(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("UFW_AES_KEY contains a non-hexadecimal character"),
    }
}
