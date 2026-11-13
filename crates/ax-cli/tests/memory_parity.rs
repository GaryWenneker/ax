//! One memory, three doors: the library, the CLI, and the MCP tool.

use std::process::Command;

use serde_json::Value;

fn ax_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path.join("ax")
        })
}

fn run_ax(root: &std::path::Path, args: &[&str]) -> Value {
    let out = Command::new(ax_bin())
        .current_dir(root)
        .args(args)
        .output()
        .expect("spawn ax");
    assert!(
        out.status.success(),
        "ax {args:?}\nstdout {}\nstderr {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&out.stdout)))
}

#[tokio::test]
async fn cue_lookup_matches_across_library_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = Command::new(ax_bin())
        .current_dir(root)
        .args(["init"])
        .output()
        .expect("spawn ax init");
    assert!(
        init.status.success(),
        "ax init\nstdout {}\nstderr {}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    let stored = run_ax(
        root,
        &[
            "remember",
            "Copy every ax binary after a build.",
            "--title",
            "Ship together",
            "--trigger",
            "reinstall-all",
            "--json",
        ],
    );
    let id = stored["id"].as_str().unwrap().to_string();

    let cli = run_ax(root, &["recall", "--cue", "reinstall-all", "--json"]);
    let cli_id = cli["matches"][0]["id"].as_str().unwrap();
    let cli_score = cli["matches"][0]["score"].as_f64().unwrap();
    assert_eq!(cli_id, id);

    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let library = ax_memory::search(
        db.pool(),
        &ax_memory::SearchRequest {
            cue: Some("reinstall-all".into()),
            limit: 3,
            ..ax_memory::SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(library[0].memory.id, id);
    assert!((library[0].score - cli_score).abs() < 0.001);

    let mut engine = ax_mcp::McpEngine::with_project_root(root.to_path_buf());
    let mcp = ax_mcp::call_tool(
        &mut engine,
        "ax_recall",
        serde_json::json!({ "cue": "reinstall-all", "limit": 3 }),
    )
    .await
    .unwrap();
    let mcp_id = mcp["matches"][0]["id"].as_str().unwrap();
    let mcp_score = mcp["matches"][0]["score"].as_f64().unwrap();
    assert_eq!(mcp_id, id);
    assert!((mcp_score - cli_score).abs() < 0.001);
}
