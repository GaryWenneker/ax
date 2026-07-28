use crate::commands::resolve_path;

/// Launch the native wgpu Command Center (embeds ax-web in-process).
pub fn run(path: Option<String>, port: u16, bind: String) -> Result<(), String> {
    let root = resolve_path(path);
    let path_str = root.to_string_lossy().into_owned();
    ax_desktop_client::run(Some(path_str), port, bind)
}
