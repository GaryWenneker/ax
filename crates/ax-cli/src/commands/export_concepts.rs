//! Alias for Open Knowledge Format (OKF) export (`ax export concepts` → `ax export okf`).

use super::export_okf::{self, ExportOkfArgs};

pub async fn run(
    path: Option<String>,
    out: Option<String>,
    limit: usize,
) -> Result<(), String> {
    export_okf::run(ExportOkfArgs {
        path,
        out,
        limit,
        check: false,
        ci: false,
        publish_wiki: false,
        dry_run: false,
        no_push: false,
        json: false,
    })
    .await
}
