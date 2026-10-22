//! The Windows host process.
//!
//! All behavior lives in `src/host_main.cpp` (compiled into this binary by
//! `build.rs`): it bootstraps the Rust core through the C ABI, installs the
//! push event sink, creates the shell window (the RNW attach point on the
//! real machine step), installs the app menu, and runs the Win32 message
//! pump with the accelerator table from the shell crate.
//!
//! On non-Windows this binary does nothing (macOS builds keep it inert, the
//! same as every cfg-gated Windows artifact).

fn main() {
    #[cfg(windows)]
    {
        unsafe extern "C" {
            fn kitron_host_main(argc: i32, argv: *const *const u8) -> i32;
        }

        // Serviceable argv hand-off; the host reads its wiring from
        // KITRON_CONFIG_JSON / KITRON_HOST_SMOKE so argv stays unused today.
        let mut argv: Vec<*const u8> = std::env::args()
            .map(|arg| std::ffi::CString::new(arg).map(|c| Box::into_raw(c.into_boxed_c_str())))
            .collect::<Result<_, _>>()
            .unwrap_or_default()
            .into_iter()
            .map(|raw: *mut u8| raw as *const u8)
            .collect();
        argv.push(std::ptr::null());
        let code = unsafe { kitron_host_main(argv.len() as i32 - 1, argv.as_ptr()) };
        std::process::exit(code);
    }

    #[cfg(not(windows))]
    {
        eprintln!("kitron-windows-host is a Windows artifact (cfg(windows))");
    }
}
