//! Terminal setup: ANSI colors on Windows, NO_COLOR support.

use std::io::IsTerminal;

/// Enable ANSI colors and VT processing where supported.
pub fn init() {
    if std::env::var("NO_COLOR").is_ok() {
        owo_colors::set_override(false);
        return;
    }

    #[cfg(windows)]
    {
        enable_windows_vt();
    }

    if std::io::stdout().is_terminal() {
        owo_colors::set_override(true);
    }
}

#[cfg(windows)]
fn enable_windows_vt() {
    use std::os::windows::io::AsRawHandle;

    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    unsafe {
        let stdout = std::io::stdout().as_raw_handle() as *mut std::ffi::c_void;
        let stderr = std::io::stderr().as_raw_handle() as *mut std::ffi::c_void;
        for handle in [stdout, stderr] {
            let mut mode: u32 = 0;
            if windows_sys::Win32::System::Console::GetConsoleMode(handle, &mut mode) != 0 {
                let _ = windows_sys::Win32::System::Console::SetConsoleMode(
                    handle,
                    mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                );
            }
        }
    }
}
