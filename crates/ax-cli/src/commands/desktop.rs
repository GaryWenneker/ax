use crate::commands::resolve_project_root;

/// Launch the native wgpu Command Center (embeds ax-web in-process).
#[cfg(feature = "desktop")]
pub fn run(path: Option<String>, port: u16, bind: String) -> Result<(), String> {
    let root = resolve_project_root(path);
    let path_str = root.to_string_lossy().into_owned();
    ax_desktop_client::run(Some(path_str), port, bind)
}

/// Headless builds have no native Command Center.
#[cfg(not(feature = "desktop"))]
pub fn run(path: Option<String>, port: u16, bind: String) -> Result<(), String> {
    let _ = (resolve_project_root(path), port, bind);
    Err(crate::commands::unavailable_in_headless_build("ax desktop"))
}
