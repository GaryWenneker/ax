//! Terminal setup: ANSI colors on Windows, NO_COLOR / force-color support.

use std::io::IsTerminal;

/// Enable ANSI colors and VT processing where supported.
pub fn init() {
    #[cfg(windows)]
    {
        enable_windows_vt();
    }

    let enabled = colors_enabled();
    console::set_colors_enabled(enabled);
    owo_colors::set_override(enabled);
}

/// Whether styled output (clap, owo-colors, indicatif) should use ANSI colors.
pub fn colors_enabled() -> bool {
    if force_color() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    if matches!(std::env::var("FORCE_COLOR").as_deref(), Ok("0")) {
        return false;
    }
    std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
}

pub fn configure_clap(cmd: &mut clap::Command) {
    let choice = if colors_enabled() {
        clap::ColorChoice::Always
    } else {
        clap::ColorChoice::Never
    };
    *cmd = cmd.clone().color(choice);
}

fn force_color() -> bool {
    env_is_truthy("AX_FORCE_COLOR")
        || matches!(std::env::var("CLICOLOR_FORCE").as_deref(), Ok("1"))
        || matches!(
            std::env::var("FORCE_COLOR").as_deref(),
            Ok("1") | Ok("true") | Ok("yes")
        )
}

fn env_is_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
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
