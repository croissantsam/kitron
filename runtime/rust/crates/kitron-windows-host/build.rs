//! Windows host build: compiles the C++ entry (`host_main.cpp`) and links the
//! `kitron-windows` staticlib into the final executable alongside the C++
//! object — the same trick the macOS twin uses for its Rust + ObjC++ shell.
//!
//! Non-Windows targets here are a no-op (the host is `#[cfg(windows)]`).

use std::env;

fn main() {
    if !env::var("CARGO_CFG_WINDOWS").is_ok() {
        return;
    }

    // 1. Compile the C++ entry with the Win32 SDK headers that MSVC provides.
    let mut build = cc::Build::new();
    build
        .file("src/host_main.cpp")
        .include("include")
        .flag_if_supported("/std:c++20")
        .flag_if_supported("/EHsc")
        .flag_if_supported("/Zc:__cplusplus")
        .warnings(true);
    build.compile("kitron_host_main");

    // Linking_model: the shell comes through the normal dependency rlibs
    // (`kitron-windows` → `kitron-core` → …) — the linker pulls each needed
    // object from the archives because the C++ entry references the C ABI
    // symbols directly (kitron_bootstrap & co). No manual `.lib` handoff:
    // a cargo dependency does not produce its `staticlib` artifact, only
    // its rlib, and pointing the linker at a non-existing artifact is
    // exactly LNK1181.
}
