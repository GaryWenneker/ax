use crate::commands::resolve_project_root;
use crate::version_check;

#[cfg(feature = "web")]
pub async fn run(path: Option<String>, port: u16, open: bool) -> Result<(), String> {
    let root = resolve_project_root(path);
    version_check::maybe_notify_update_with(true).await;
    ax_web::serve(root, port, open).await
}

#[cfg(not(feature = "web"))]
pub async fn run(path: Option<String>, port: u16, open: bool) -> Result<(), String> {
    let _ = (resolve_project_root(path), port, open);
    version_check::maybe_notify_update_with(true).await;
    Err(crate::commands::unavailable_in_headless_build("ax web"))
}
