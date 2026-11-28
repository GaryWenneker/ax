//! `ax spec-approve` records an approval in the owner's name, so it must not
//! run without a person at an interactive terminal.

use std::process::{Command, Stdio};

fn ax_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(std::path::PathBuf::from)
        .expect("CARGO_BIN_EXE_ax")
}

#[tokio::test]
async fn non_interactive_spec_approve_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let ax_dir = project.path().join(".ax");
    std::fs::create_dir_all(&ax_dir).unwrap();
    let db = ax_db::Database::open(&ax_dir.join("ax.db")).await.unwrap();
    drop(db);
    let global = home.path().join(".ax").join("global.db");
    let membership = ax_global_db::open_and_init(&global).await.unwrap();
    assert!(
        ax_global_db::access::bootstrap_owner(&membership, "owner-1", "Owner", None)
            .await
            .unwrap()
    );
    membership.close().await;

    let body =
        r#"{"projectId":"p","artifactKind":"plan","artifactId":"x","version":1,"reason":"r"}"#;
    let out = Command::new(ax_bin())
        .current_dir(project.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("AX_GLOBAL_DB", &global)
        .args(["spec-approve", body])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "non-interactive approve must fail");
    assert!(
        stderr.contains("interactive terminal"),
        "refusal names the reason; stdout: {stdout}; stderr: {stderr}"
    );
    assert!(
        !stdout.contains("\"ok\""),
        "nothing reached the approval call: {stdout}"
    );
}
