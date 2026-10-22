//! The Windows host process.
//!
//! All behavior lives in `src/host_main.cpp` (compiled into this binary by
//! `build.rs`): it bootstraps the Rust core through the C ABI, installs the
//! push event sink, creates the shell window (the RNW attach point on the
//! real machine step), installs the app menu, and runs the Win32 message
//! pump with the accelerator table from the shell crate.
//!
//! On non-Windows this binary does nothing (macOS keeps it inert, the same
//! as every other cfg-gated Windows artifact).

fn main() {
    #[cfg(windows)]
    {
        // The linker ships a dependency archive only when a Rust unit
        // references it — host_main.cpp's externs do not count. This anchor
        // keeps `kitron-windows` on the link line so every `kitron_*` symbol
        // the C++ entry calls resolves from the same rlib archive.
        #[used]
        static SHELL_ENTRY: unsafe extern "C" fn() -> *mut i8 =
            kitron_windows::kitron_bootstrap;

        unsafe extern "C" {
            fn kitron_host_main(argc: i32, argv: *const *const u8) -> i32;
        }

        // Serviceable argv hand-off; the host reads its wiring from
        // KITRON_CONFIG_JSON / KITRON_HOST_SMOKE so argv stays unused today.
        let argv = build_argv();
        let code = unsafe { kitron_host_main(argv.len() as i32 - 1, argv.as_ptr()) };
        std::process::exit(code);
    }

    #[cfg(not(windows))]
    {
        eprintln!("kitron-windows-host is a Windows artifact (cfg(windows))");
    }
}

#[cfg(windows)]
fn build_argv() -> Vec<*const u8> {
    let mut argv: Vec<*const u8> = Vec::new();
    for arg in std::env::args() {
        // Leak-per-arg is the process lifetime; the host runs until exit.
        let boxed = arg.into_bytes().into_boxed_slice();
        argv.push(boxed.as_ptr());
        std::mem::forget(boxed);
    }
    argv.push(std::ptr::null());
    argv
}
