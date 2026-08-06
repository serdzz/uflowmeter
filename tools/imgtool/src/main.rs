//! Host tool: turn a release binary into an encrypted update image.
//!
//!   imgtool keygen [--out KEYFILE]
//!   imgtool pack --key KEYFILE --in app.bin --out app.ufw [--version N]
//!   imgtool info app.ufw
//!   imgtool verify --key KEYFILE app.ufw
//!
//! The image format lives in the `fwimage` crate, shared with the
//! bootloader, so what this writes is by construction what the
//! bootloader parses.
//!
//! Argument parsing is hand-rolled to keep the tool buildable with no
//! third-party crates beyond the RNG — it is invoked from a Makefile
//! rule, not by a human needing completions.

use fwimage::{layout, Decryptor, Header, HeaderError, Verifier, HEADER_LEN, KEY_LEN, NONCE_LEN};
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(&args[1..]),
        Some("pack") => pack(&args[1..]),
        Some("info") => info(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("--help") | Some("-h") | None => {
            usage();
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(format!("unknown subcommand `{other}`")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("imgtool: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprint!(
        "\
imgtool — pack and encrypt uflowmeter firmware update images

  imgtool keygen [--out KEYFILE]
        Write a fresh random AES-256 key as 64 hex characters.

  imgtool pack --key KEYFILE --in app.bin --out app.ufw [--version N]
        Encrypt a release binary into an update image.

  imgtool info app.ufw
        Print an image's header without needing the key.

  imgtool verify --key KEYFILE app.ufw
        Check an image authenticates, as the bootloader would.
"
    );
}

// ── argument helpers ──────────────────────────────────────────────────

/// Fetch `--name VALUE`. Returns None if absent.
fn opt(args: &[String], name: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{name} needs a value")),
    }
}

fn req(args: &[String], name: &str) -> Result<String, String> {
    opt(args, name)?.ok_or_else(|| format!("missing required {name}"))
}

/// The first argument that is not an option or an option's value.
fn positional(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for (i, a) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a.starts_with("--") {
            skip_next = true;
            continue;
        }
        return args.get(i).cloned();
    }
    None
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) {
        return Err("hex string has an odd number of digits".into());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&clean[i..i + 2], 16)
                .map_err(|_| format!("bad hex at byte {}", i / 2))
        })
        .collect()
}

fn load_key(path: &str) -> Result<[u8; KEY_LEN], String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading key {path}: {e}"))?;
    let bytes = from_hex(&text)?;
    if bytes.len() != KEY_LEN {
        return Err(format!(
            "key must be {KEY_LEN} bytes ({} hex chars), got {}",
            KEY_LEN * 2,
            bytes.len()
        ));
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&bytes);
    Ok(key)
}

// ── subcommands ───────────────────────────────────────────────────────

fn keygen(args: &[String]) -> Result<(), String> {
    let mut key = [0u8; KEY_LEN];
    getrandom::getrandom(&mut key).map_err(|e| format!("no entropy available: {e}"))?;
    let hex = to_hex(&key);
    match opt(args, "--out")? {
        Some(path) => {
            fs::write(&path, format!("{hex}\n")).map_err(|e| format!("writing {path}: {e}"))?;
            eprintln!("wrote a new AES-256 key to {path}");
            eprintln!("This key is the only thing protecting the update path. Back it up");
            eprintln!("somewhere the build machine is not, and do not commit it.");
        }
        None => println!("{hex}"),
    }
    Ok(())
}

fn pack(args: &[String]) -> Result<(), String> {
    let key = load_key(&req(args, "--key")?)?;
    let in_path = req(args, "--in")?;
    let out_path = req(args, "--out")?;
    let version: u32 = match opt(args, "--version")? {
        Some(v) => v.parse().map_err(|_| format!("bad --version `{v}`"))?,
        None => 1,
    };

    let mut payload = fs::read(&in_path).map_err(|e| format!("reading {in_path}: {e}"))?;
    if payload.is_empty() {
        return Err(format!("{in_path} is empty"));
    }
    let raw_len = payload.len();

    // Flash programs whole words. Pad with the erased-flash value so a
    // short tail is indistinguishable from never-written flash.
    let ws = layout::WRITE_SIZE as usize;
    if payload.len() % ws != 0 {
        payload.resize(payload.len().div_ceil(ws) * ws, 0xFF);
    }

    if payload.len() as u32 > layout::MAX_PAYLOAD {
        return Err(format!(
            "image is {} bytes, slot holds {} — the application has outgrown the two-slot layout",
            payload.len(),
            layout::MAX_PAYLOAD
        ));
    }

    // A repeated nonce under the same key breaks GCM outright: it leaks
    // the XOR of two plaintexts and hands over the authentication
    // subkey. 96 random bits per image is what keeps that from ever
    // happening, so this must come from the OS, never a counter or a
    // timestamp.
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("no entropy available: {e}"))?;

    let mut header = Header {
        payload_len: payload.len() as u32,
        image_version: version,
        nonce,
        tag: [0u8; fwimage::TAG_LEN],
    };

    // Encrypt, then authenticate the ciphertext — the tag covers the
    // header's AAD, which is why it is computed after the length is final.
    Decryptor::new(&key, &header).apply(&mut payload);
    let mut v = Verifier::new(&key, &header);
    v.update(&payload);
    header.tag = v.finish();

    let mut raw_header = [0u8; HEADER_LEN];
    header.write(&mut raw_header);

    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&raw_header);
    out.extend_from_slice(&payload);
    fs::write(&out_path, &out).map_err(|e| format!("writing {out_path}: {e}"))?;

    let padding = payload.len() - raw_len;
    eprintln!("packed {in_path} -> {out_path}");
    eprintln!(
        "  payload    {} bytes{}",
        payload.len(),
        if padding > 0 {
            format!(" ({raw_len} + {padding} pad)")
        } else {
            String::new()
        }
    );
    eprintln!(
        "  slot use   {:.1}% of {} bytes",
        100.0 * payload.len() as f64 / layout::MAX_PAYLOAD as f64,
        layout::MAX_PAYLOAD
    );
    eprintln!("  version    {version}");
    eprintln!("  nonce      {}", to_hex(&header.nonce));
    eprintln!("  tag        {}", to_hex(&header.tag));
    Ok(())
}

/// Read an image and split it into header and ciphertext.
fn open_image(path: &str) -> Result<(Header, Vec<u8>), String> {
    let raw = fs::read(path).map_err(|e| format!("reading {path}: {e}"))?;
    let header = Header::parse(&raw, layout::MAX_PAYLOAD, layout::WRITE_SIZE)
        .map_err(|e| format!("{path}: {}", describe(e)))?;
    let body = raw
        .get(HEADER_LEN..HEADER_LEN + header.payload_len as usize)
        .ok_or_else(|| {
            format!(
                "{path}: header declares {} payload bytes but the file holds {}",
                header.payload_len,
                raw.len().saturating_sub(HEADER_LEN)
            )
        })?
        .to_vec();
    Ok((header, body))
}

fn describe(e: HeaderError) -> &'static str {
    match e {
        HeaderError::Truncated => "file is shorter than one header page",
        HeaderError::Magic => "not an update image (no magic)",
        HeaderError::Format => "unsupported header format version",
        HeaderError::Crc => "header CRC mismatch — the file is damaged",
        HeaderError::PayloadLen => "declared payload length is unusable",
    }
}

fn info(args: &[String]) -> Result<(), String> {
    let path = positional(args).ok_or("info needs an image path")?;
    let (h, _) = open_image(&path)?;
    println!("payload    {} bytes", h.payload_len);
    println!("version    {}", h.image_version);
    println!("nonce      {}", to_hex(&h.nonce));
    println!("tag        {}", to_hex(&h.tag));
    println!(
        "slot use   {:.1}% of {} bytes",
        100.0 * h.payload_len as f64 / layout::MAX_PAYLOAD as f64,
        layout::MAX_PAYLOAD
    );
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let key = load_key(&req(args, "--key")?)?;
    let path = positional(args).ok_or("verify needs an image path")?;
    let (h, body) = open_image(&path)?;

    let mut v = Verifier::new(&key, &h);
    v.update(&body);
    if !v.verify(&h.tag) {
        return Err(format!(
            "{path}: authentication FAILED — wrong key, or the image was modified"
        ));
    }
    println!(
        "{path}: authenticates, {} bytes, version {}",
        h.payload_len, h.image_version
    );
    Ok(())
}
