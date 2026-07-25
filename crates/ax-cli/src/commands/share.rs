use uuid::Uuid;

use crate::commands::resolve_path;

pub async fn run(
    path: Option<String>,
    port: u16,
    bind: String,
    open: bool,
    token: Option<String>,
) -> Result<(), String> {
    let root = resolve_path(path);
    let share_token = token.unwrap_or_else(|| Uuid::new_v4().to_string().replace('-', ""));
    eprintln!("Starting shareable Command Center (read-only)…");
    eprintln!("Tip: wrap with a tunnel for remote access, e.g.");
    eprintln!("  cloudflared tunnel --url http://127.0.0.1:{port}");
    ax_web::serve_with(ax_web::ServeOptions {
        root,
        port,
        open,
        bind,
        share_token: Some(share_token),
    })
    .await
}
