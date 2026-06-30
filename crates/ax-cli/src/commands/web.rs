use crate::commands::resolve_path;

pub async fn run(path: Option<String>, port: u16, open: bool) -> Result<(), String> {
    let root = resolve_path(path);
    ax_web::serve(root, port, open).await
}
