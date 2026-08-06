//! Put this crate's `memory.x` where the linker will find it.
//!
//! `cortex-m-rt`'s `link.x` does `INCLUDE memory.x`, resolved against
//! the linker search path. Cargo does not add the crate root to that
//! path, so without this the file is silently ignored and whatever
//! other `memory.x` is on the path wins — which is exactly how the
//! application's own `memory.x` sat dead in the tree while embassy's
//! generated one was linked instead.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=UFW_AES_KEY");
}
