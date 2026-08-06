# Encrypted-update bootloader

Field firmware updates over the meter's existing serial line, with the
image encrypted and authenticated using AES-256-GCM.

> **The application no longer boots on its own.** It links at
> `0x08004000`, not at the reset vector. A board flashed with only
> `uflowmeter` will sit dead. Flash the bootloader too — `make flash`
> does both.

---

## 1. Flash layout

```
0x08000000  ┌──────────────────┐
            │ bootloader  16K  │  verify, install, jump
0x08004000  ├──────────────────┤
            │ slot A     120K  │  the application, runs from here
0x08022000  ├──────────────────┤
            │ slot B     120K  │  header page + ciphertext
0x08040000  └──────────────────┘
```

The numbers live in **`fwimage::layout`**, and its `layout_is_consistent`
test checks that the three regions tile the part exactly and that every
boundary lands on an erase page. The two linker scripts
(`memory-app.x`, `bootloader/memory.x`) repeat them only because a
linker script cannot read Rust constants — if you change one, change all
three and let the test confirm it.

Current occupancy:

| | used | region | |
|---|---|---|---|
| bootloader | 10.6 KiB | 16 KiB | 65 % |
| application | 87.6 KiB | 120 KiB | 71 % |

### Why `memory-app.x` is not called `memory.x`

`cortex-m-rt`'s `link.x` does `INCLUDE memory.x`, and the linker
searches its **working directory** — the workspace root — before any
`-L` path. A file called `memory.x` there is therefore picked up by
every crate in the workspace, silently overriding the script that crate
ships. That is not hypothetical: it is exactly what happened while this
was being built, and the bootloader linked against a 256 KiB region and
overflowed its own without a word of complaint. Each crate now supplies
its script through its own `build.rs`, and nothing called `memory.x`
sits at the root.

While tracing that: the `STACK_SIZE = 8192` line that used to be in the
root `memory.x` never did anything. `cortex-m-rt` honours `_stack_start`
and does not read `STACK_SIZE`; `_stack_start` is the end of RAM, so the
stack has always started at `0x20008000` and grown down into whatever
`.data` and `.bss` leave — about 19 KiB today.

---

## 2. The image format

`fwimage` defines it once and all three consumers share it: the
bootloader, the host packer, and the tests. A format defined in one
place cannot drift between the thing that writes images and the thing
that has to boot from them.

```
offset  size  field
     0     4  magic "UFWU"
     4     4  format version
     8     4  payload length          ┐ authenticated
    12     4  image version           ┘ as GCM AAD
    16    12  GCM nonce
    28    16  GCM tag
    44     4  CRC-32 of bytes 0..44
    48   208  zero padding
   256   ...  ciphertext
```

The header is padded to a full 256-byte erase page so the ciphertext
starts page-aligned and the bootloader can walk it without a shifting
offset.

**The AAD is the point of the length and version fields being in the
clear.** They are not encrypted, but they are covered by the tag, so a
genuine ciphertext cannot be replayed under a forged length — which
would otherwise be a way to make the bootloader read past the slot.

**The CRC is not a security control.** It catches a header torn by a
power cut mid-write, which would otherwise carry a plausible magic in
front of a garbage length. Tampering is the tag's job. Note the order in
`Header::parse`: CRC first, *then* the length is trusted.

### Cryptography

AES-256-GCM, software (this part has no AES accelerator — that is the
L162 line). Composed from `aes` + `ctr` + `ghash` rather than `aes-gcm`,
because the image is up to 120 KiB and the MCU has 32 KiB of RAM:
nothing may buffer a whole image, and `AeadInPlace` wants a mutable
buffer holding all of it. `Verifier` and `Decryptor` are streaming and
take chunks of any size.

Vectors in `fwimage/src/tests.rs` come from **OpenSSL** (via Python
`cryptography`), not from this implementation. Checking our output
against our own would only prove self-consistency; what matters is that
an image a standard GCM implementation produced is one the bootloader
accepts.

---

## 3. Keys

One AES-256 key for the whole fleet, compiled into the bootloader.

```sh
make imgtool
target/<host>/release/imgtool keygen --out ufw.key
UFW_AES_KEY=$(cat ufw.key) make bootloader
```

`*.key` is gitignored. Keep the real one in the release process's secret
store, not on the build machine.

**A build with no key set fails.** There is deliberately no fallback: a
default key that ships by accident is worse than a build error, because
nothing about the resulting firmware looks wrong. For bench work,
`--features dev-key` compiles in a well-known key — you have to ask for
it, which is the point.

Two consequences of one shared key, stated plainly:

- **The key is in flash.** Anyone who can read a device's flash has it,
  and with it the ability to sign an image every unit will accept. RDP
  Level 1 is therefore not optional; without it the key is one
  `probe-rs read` away.
- **Compromise does not stay local.** One extracted key is every unit in
  the field. Moving to per-device keys provisioned at manufacture would
  change only `bootloader/src/key.rs` plus a provisioning step.

---

## 4. Releasing an update

```sh
UFW_AES_KEY=$(cat ufw.key) make image IMG_VERSION=7
```

This builds the application, strips it to a raw binary, encrypts it and
writes `app.ufw`. Inspect or check one with:

```sh
imgtool info app.ufw                  # header, no key needed
imgtool verify --key ufw.key app.ufw  # authenticates, as the bootloader would
```

Install it over the serial line:

```
> firmware_update
Erasing staging slot...
Send the .ufw image with XMODEM-CRC now.
<send app.ufw with any XMODEM-CRC sender>
Staged. Resetting...
```

The meter reboots and the bootloader takes over. `sx --xmodem`, minicom,
Tera Term and HyperTerminal all speak the required protocol; 1K packets
are accepted as well as 128-byte ones.

---

## 5. What happens on a power cut

There is **no separate "update pending" flag**, and that is deliberate —
a flag is a second copy of the truth, and copies drift. The state *is*
whether slot B holds a header that parses.

Two ordering decisions make that work:

**The receiver writes the header last.** The image arrives header-first,
but `SlotBWriter` holds those 256 bytes in RAM for the whole transfer
and commits them only once the body is complete. So an interrupted
transfer leaves slot B headerless, which reads as "nothing staged" — the
truth. Had the header gone down first, a transfer cut short would leave
a valid header in front of a truncated body, and the bootloader would
spend a boot authenticating an image that was never going to pass.

**The bootloader erases the header last.** It is the final act of a
successful install, so until it happens a reset simply redoes the copy.

| Power lost during | Slot B header | Next boot |
|---|---|---|
| receiving into B | absent | boots A; the update did not happen |
| verification | valid | verifies again — nothing had been written |
| erase/write of A | valid | redoes the copy; A was mid-overwrite anyway |
| invalidating B | either | redoes the copy, or boots A — both correct |

Every row either boots the old application or redoes an idempotent copy.
The state that must never occur — jumping into a half-written slot A —
cannot, because a valid slot B header always diverts to the copy before
any jump is considered.

### Verification is a separate pass

GCM is encrypt-then-MAC, so an image can only be judged once all of it
has been read. Decrypting while verifying would put unverified plaintext
into the live application slot and only afterwards decide whether it was
genuine. So the bootloader GHASHes the whole ciphertext and checks the
tag first, and only on success erases slot A. The cost is one extra pass
over 120 KiB, which is cheap because flash is memory-mapped.

The application does **not** check the signature itself. It has no
business holding the key, and a second check there could only disagree
with the one that matters.

---

## 6. Flashing over SWD

Two binaries, two downloads. Each ELF carries its own addresses, so no
offsets are needed.

```sh
make flash              # bootloader + application, from scratch
make flash-bootloader   # bootloader only
make flash-app          # application only, leaves the bootloader alone
```

All of these pass `--speed 500`, for the reason documented in
`CLAUDE.md`: at the default SWD rate this board fails to connect
reproducibly.

`cargo run --release` is no longer the right way to flash. It would
download only the application, leaving whatever is at the reset vector
untouched — on a fresh part, nothing.

---

## 7. Not verified on hardware

Everything below the transport is covered by 338 host tests, including
the GCM implementation against OpenSSL vectors and an end-to-end
pack/verify/tamper round trip in CI. What has **not** been exercised:

- **No image has been installed on a real device.** The bootloader has
  not been flashed and no update has been staged, verified, copied and
  booted on hardware.
- **The XMODEM transfer of a firmware-sized image.** The protocol logic
  is tested and the configuration-sized transfers share the same code
  path, but 120 KiB over the wire needs a serial peer running an XMODEM
  sender, which this bench does not have.
- **Timing.** Erasing a 120 KiB slot is 480 page erases; at the
  datasheet's ~3.94 ms per page that is roughly 1.9 s, twice per update
  (slot B before receiving, slot A before installing). Estimated, not
  measured.

## 8. Open

- **No recovery path if slot A is empty or corrupt.** The bootloader
  parks and says so on RTT; it cannot itself receive an image. Adding a
  UART + XMODEM fallback would cost roughly 4–6 KiB of the 5.4 KiB
  currently spare in the bootloader region — feasible, but it would want
  measuring rather than assuming.
- **No anti-rollback.** `image_version` is carried and authenticated but
  not compared against what is running, so an older signed image can be
  installed. Deliberate for now: with no recovery path, being able to go
  back is worth more than preventing it.
