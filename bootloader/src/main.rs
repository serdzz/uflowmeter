//! uflowmeter bootloader: authenticate a staged update, install it,
//! then hand over to the application.
//!
//! # Flash layout
//!
//! ```text
//!   0x08000000  bootloader  16K   this program
//!   0x08004000  slot A     120K   the application, runs from here
//!   0x08022000  slot B     120K   header page + AES-256-GCM ciphertext
//! ```
//!
//! The application receives an update over XMODEM and writes it into
//! slot B, then resets. Everything after that happens here.
//!
//! # The update is authenticated before slot A is touched
//!
//! GCM is encrypt-then-MAC, so an image can only be judged once all of
//! it has been read. Decrypting while verifying would put unverified
//! plaintext into the live application slot and only afterwards decide
//! whether it was genuine — so instead there are two passes: GHASH the
//! whole ciphertext and check the tag, and only on success erase slot A
//! and decrypt into it. Reads are cheap here because flash is
//! memory-mapped; the cost is one extra pass over 120 KiB.
//!
//! # Power-fail behaviour
//!
//! There is no separate "update pending" flag to get out of step with
//! reality. The state *is* whether slot B holds a header that parses:
//!
//! | Power lost during | Slot B header | Next boot does |
//! |---|---|---|
//! | receiving into B | absent or bad CRC | boots A, update simply did not happen |
//! | verification | valid | verifies again — nothing was written |
//! | erase/write of A | valid | redoes the whole copy; A was already being overwritten |
//! | invalidating B | valid or erased | redoes the copy, or boots A — both correct |
//!
//! Every row either boots the old application or redoes an idempotent
//! copy. The one state that must never occur — jumping into a
//! half-written slot A — cannot, because a valid slot B header always
//! diverts to the copy before any jump is considered.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use embassy_stm32::flash::{Blocking, Error as FlashError, Flash};
use fwimage::{layout, Decryptor, Header, HeaderError, Verifier, HEADER_LEN};

use defmt_rtt as _;

mod key;

/// Park on panic rather than reset.
///
/// `panic-probe` is not used here: its formatting path costs more flash
/// than the 16 KiB budget has spare, and there is nothing to format —
/// this program has no fallible indexing left once `Header::parse` has
/// bounded `payload_len`.
///
/// Parking, not `sys_reset()`: a bootloader that reboots on panic turns
/// any deterministic fault into a boot loop that is hard to attach a
/// probe to. A halted core can be inspected.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    defmt::error!("bootloader: panic");
    park()
}

const PAGE: usize = layout::PAGE as usize;

/// Offset of a flash address from the start of flash — what the embassy
/// flash driver takes, as opposed to an absolute address.
const fn offset_of(addr: u32) -> u32 {
    addr - layout::FLASH_BASE
}

#[entry]
fn main() -> ! {
    defmt::info!("bootloader: start");

    // No `embassy_stm32::init()`: the bootloader needs exactly one
    // peripheral and no clock tree. Leaving the chip in its reset state
    // means the application's own `init()` starts from the same
    // conditions as it does after a plain reset, rather than from
    // whatever this program happened to leave behind.
    let p = unsafe { embassy_stm32::Peripherals::steal() };
    let mut flash = Flash::new_blocking(p.FLASH);

    match Header::parse(slot_b(), layout::MAX_PAYLOAD, layout::WRITE_SIZE) {
        Ok(header) => install(&mut flash, &header),
        // An erased or never-written slot: the ordinary case, not a fault.
        Err(HeaderError::Magic) => defmt::info!("bootloader: no staged image"),
        Err(other) => {
            // The header is damaged rather than absent — most likely a
            // reset partway through receiving. Clear it so the next boot
            // does not spend the time rediscovering the same rubbish.
            defmt::warn!(
                "bootloader: staged header unusable ({=str}), discarding",
                describe(other)
            );
            discard_staged(&mut flash);
        }
    }

    boot_application()
}

/// `HeaderError` as a log string. `fwimage` is deliberately free of
/// defmt so the host packer can share it unchanged, so the mapping
/// lives here.
fn describe(e: HeaderError) -> &'static str {
    match e {
        HeaderError::Truncated => "slot shorter than a header page",
        HeaderError::Magic => "no magic",
        HeaderError::Format => "unsupported format version",
        HeaderError::Crc => "header CRC mismatch",
        HeaderError::PayloadLen => "declared payload length unusable",
    }
}

/// Slot B as a memory-mapped slice.
///
/// Safe in practice for the reason internal flash always is: it is
/// mapped, readable, and this range is inside the part's flash by the
/// `layout_is_consistent` test. Nothing else writes it while we read.
fn slot_b() -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(layout::SLOT_B as *const u8, layout::SLOT_LEN as usize) }
}

/// Verify the staged image and, if it is genuine, copy it into slot A.
fn install(flash: &mut Flash<'static, Blocking>, header: &Header) {
    defmt::info!(
        "bootloader: staged image, {=u32} bytes, version {=u32}",
        header.payload_len,
        header.image_version
    );

    let ciphertext = &slot_b()[HEADER_LEN..HEADER_LEN + header.payload_len as usize];

    let mut verifier = Verifier::new(&key::KEY, header);
    for chunk in ciphertext.chunks(PAGE) {
        verifier.update(chunk);
    }
    if !verifier.verify(&header.tag) {
        // Wrong key, a modified image, or flash damage. All three mean
        // the same thing here: this must not be installed. Discard it so
        // the device does not re-examine it on every boot.
        defmt::error!("bootloader: authentication FAILED, image rejected");
        discard_staged(flash);
        return;
    }
    defmt::info!("bootloader: authenticated, installing");

    match copy_into_slot_a(flash, header, ciphertext) {
        Ok(()) => {
            // Only now is the update complete. Erasing the header is
            // what makes it so — until this succeeds, a reset redoes
            // the copy, which is exactly what we want.
            defmt::info!("bootloader: installed");
            discard_staged(flash);
        }
        Err(e) => {
            // Slot A is now partly written, but slot B still holds a
            // valid header, so the next boot will retry rather than
            // jump into the wreckage.
            defmt::error!(
                "bootloader: install failed ({}), will retry on next boot",
                e
            );
        }
    }
}

/// Erase slot A and decrypt the staged ciphertext into it.
fn copy_into_slot_a(
    flash: &mut Flash<'static, Blocking>,
    header: &Header,
    ciphertext: &[u8],
) -> Result<(), FlashError> {
    let base = offset_of(layout::SLOT_A);

    // Erase the whole slot, not just the pages about to be written, so
    // no tail of the previous application survives past the end of the
    // new one.
    flash.blocking_erase(base, base + layout::SLOT_LEN)?;

    let mut decryptor = Decryptor::new(&key::KEY, header);
    let mut page = [0u8; PAGE];
    let mut written: u32 = 0;

    for chunk in ciphertext.chunks(PAGE) {
        let n = chunk.len();
        page[..n].copy_from_slice(chunk);
        decryptor.apply(&mut page[..n]);
        // `payload_len` is a multiple of the write size and PAGE is
        // too, so every chunk — including the last — is a whole number
        // of flash words.
        flash.blocking_write(base + written, &page[..n])?;
        written += n as u32;
    }

    Ok(())
}

/// Erase slot B's header page, which is what marks an update as no
/// longer pending. Only the header is erased: the ciphertext behind it
/// is unreachable without a header and erasing 120 KiB to no purpose
/// would just cost flash endurance.
fn discard_staged(flash: &mut Flash<'static, Blocking>) {
    let base = offset_of(layout::SLOT_B);
    if let Err(e) = flash.blocking_erase(base, base + layout::PAGE) {
        // Not fatal — the image either authenticates on the next boot
        // and is reinstalled idempotently, or it does not and is
        // rejected again.
        defmt::warn!("bootloader: could not clear the staged header ({})", e);
    }
}

/// Hand control to the application in slot A.
fn boot_application() -> ! {
    let vector_table = layout::SLOT_A as *const u32;
    // SAFETY: slot A is mapped flash; these two words are the vector
    // table's initial stack pointer and reset vector whatever their
    // contents, and both are validated before use.
    let (sp, reset) = unsafe {
        (
            core::ptr::read_volatile(vector_table),
            core::ptr::read_volatile(vector_table.offset(1)),
        )
    };

    if !plausible_vector_table(sp, reset) {
        defmt::error!(
            "bootloader: slot A holds no bootable image (sp={=u32:#x} reset={=u32:#x})",
            sp,
            reset
        );
        park();
    }

    defmt::info!("bootloader: jumping to the application");

    // SAFETY: the vector table has been checked to have a stack pointer
    // inside RAM and a reset vector inside slot A with the Thumb bit
    // set. Interrupts are masked first so nothing can be delivered
    // through a half-updated VTOR, and VTOR is repointed at slot A
    // before the jump — `bootload` does not do that, and an application
    // left running against the bootloader's vector table would take
    // every interrupt into this program's handlers.
    unsafe {
        cortex_m::interrupt::disable();
        let scb = &*cortex_m::peripheral::SCB::PTR;
        scb.vtor.write(layout::SLOT_A);
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        cortex_m::asm::bootload(vector_table)
    }
}

/// Reject a slot that clearly holds no application.
///
/// This is a sanity check against an erased or half-written slot, not a
/// security control — authentication is what decides whether an image
/// may be installed. It exists so a blank device says so on RTT instead
/// of hard-faulting into an unmapped address.
fn plausible_vector_table(sp: u32, reset: u32) -> bool {
    const RAM_START: u32 = 0x2000_0000;
    const RAM_END: u32 = RAM_START + 32 * 1024;

    let stack_ok = sp > RAM_START && sp <= RAM_END && sp.is_multiple_of(4);
    // Thumb state is mandatory on Cortex-M: an even reset vector would
    // fault immediately.
    let reset_ok =
        reset & 1 == 1 && (layout::SLOT_A..layout::SLOT_A + layout::SLOT_LEN).contains(&reset);

    stack_ok && reset_ok
}

/// Nothing to run and nothing to install. Stop, rather than jump
/// somewhere arbitrary — the device is attached to a probe in this
/// state by definition, and a loop is inspectable.
fn park() -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}
