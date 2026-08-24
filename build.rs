//! Hand the application's linker script to the linker.
//!
//! See `memory-app.x` for why it is not simply called `memory.x`: a
//! file by that name in the workspace root is found by every crate in
//! the workspace, because the linker searches its working directory
//! before the -L paths, and it would quietly override the bootloader's
//! own script.
//!
//! This also replaces embassy-stm32's `memory-x` feature, which
//! generates a script covering the whole 256 KiB part — correct before
//! there was a bootloader, wrong now that the application lives in slot
//! A.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory-app.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory-app.x");
    println!("cargo:rerun-if-changed=build.rs");
}
