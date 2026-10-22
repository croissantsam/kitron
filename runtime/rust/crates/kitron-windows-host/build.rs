//! Windows host build: compiles the C++ entry (`host_main.cpp`) and hands the
//! linker what the Rust dependency chain cannot produce on its own:
//!
//! - the C++ object itself (into a small static archive inside OUT_DIR),
//! - the user32 import library — the `windows` crate links its Win32 surface
//!   with raw-dylib, which only synthesizes import records for symbols
//!   referenced by *Rust* code; the host's C++ entry calls user32 directly
//!   (GetMessageW / SetTimer / TranslateAcceleratorW…), so the import lib
//!   must be on the link line explicitly.
//!
//! The shell's own `kitron_*` symbols resolve through the normal rlib
//! dependency chain — kept alive by the `#[used]` anchor in `src/main.rs`
//! (rustc drops dependency archives no Rust unit references).
//!
//! Non-Windows targets are a no-op (the host is `cfg(windows)`).

use std::env;

fn main() {
    if !env::var("CARGO_CFG_WINDOWS").is_ok() {
        return;
    }

    let mut build = cc::Build::new();
    build
        .file("src/host_main.cpp")
        .include("include")
        .flag_if_supported("/std:c++20")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/Zc:__cplusplus")
        .warnings(true);
    build.compile("kitron_host_main");

    // MSVC's default link set does not include user32 — the host's own Win32
    // calls need it (the shell adds its Win32 needs through raw-dylib, which
    // needs no import libraries).
    println!("cargo:rustc-link-lib=user32");
}
