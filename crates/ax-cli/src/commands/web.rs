use crate::commands::resolve_path;
use crate::version_check;

pub async fn run(path: Option<String>, port: u16, open: bool) -> Result<(), String> {
    let root = resolve_path(path);
    version_check::maybe_notify_update_with(true).await;
    ax_web::serve(root, port, open).await
}
