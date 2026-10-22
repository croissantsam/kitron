//! Windows host build: compiles the C++ entry (`host_main.cpp`) and links the
//! `kitron-windows` staticlib into the final executable alongside the C++
//! object — the same trick the macOS twin uses for its Rust + ObjC++ shell.
//!
//! Non-Windows targets here are a no-op (the host is `#[cfg(windows)]`).

use std::env;
use std::path::PathBuf;

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

    // 2. Link the kitron-windows staticlib produced for this exact profile.
    //    OUT_DIR = <target>/<triple>/<profile>/build/kitron-windows-host-*/out
    //    → three levels up lands in <target>/<triple>/<profile>.
    //    (On plain `cargo build` without --target that is just target/debug.)
    let profile_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    println!("cargo:rustc-link-search=native={}", profile_dir.display());
    println!("cargo:rustc-link-lib=static=kitron_windows");

    // The Windows SDK import libs the shell surfaces touch (user32 comes in
    // through the `windows` crate's link annotations; these complete the set).
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=gdi32");
    println!("cargo:rustc-link-lib=shell32");
    println!("cargo:rustc-link-lib=ole32");
}
