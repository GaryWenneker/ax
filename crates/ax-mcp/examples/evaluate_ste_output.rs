//! Model-free paired MCP measurements. No agent-quality or billed-usage claims.
use ax_core::Ax;
use ax_mcp::{server::handle_request, McpEngine};
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for (task, prompt, policy) in [
        ("simple", "Inspect the parser", "Only edit src/parser.rs. Preserve public function signatures."),
        ("access", "Inspect role permissions", "If role is Reader, deny writes unless an explicit grant exists. The finding is unverified."),
        ("replication", "Inspect replication setup", "If replication configuration exists, hide the setup form. Otherwise, show the setup form. Preserve project data when configuration is removed."),
    ] {
        let dir = tempfile::tempdir()?;
        let ax = Ax::init(dir.path()).await?;
        let rule = dir.path().join(".agents/rules/evaluation.mdc");
        tokio::fs::create_dir_all(rule.parent().ok_or("missing rule parent")?).await?;
        tokio::fs::write(&rule, format!("---\nid: evaluation\nlevel: CRITICAL\nalwaysApply: true\n---\n{policy}\n")).await?;
        ax_policy::index_policy(ax.db_pool(), dir.path(), true).await?;
        for repetition in 0..5 {
            let arms = if repetition % 2 == 0 { ["standard", "ste-inspired"] } else { ["ste-inspired", "standard"] };
            for arm in arms {
                let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
                let session = format!("eval-{task}-{repetition}-{arm}");
                for turn in ["cold", "warm", "reset"] {
                    let started = Instant::now();
                    let reply = handle_request(&mut engine, "tools/call", json!({
                        "name":"ax_preflight", "arguments": {
                            "session":session, "prompt":prompt, "textStyle":arm,
                            "context_reset":turn == "reset"
                        }
                    })).await.result.map_err(std::io::Error::other)?;
                    if reply["isError"] != false { return Err("MCP evaluation request failed".into()); }
                    let text = reply["content"][0]["text"].as_str().ok_or("missing MCP text")?;
                    let source_preserved = if turn == "warm" { !text.contains(policy) } else { text.contains(policy) };
                    if !source_preserved { return Err("required policy delivery mismatch".into()); }
                    let metadata = reply.get("structuredContent").ok_or("missing metadata")?;
                    println!("{}", json!({
                        "task":task, "arm":arm, "repetition":repetition, "turn":turn,
                        "text_tokens":ax_usage::count_tokens(text),
                        "metadata_tokens":ax_usage::count_tokens(&metadata.to_string()),
                        "reported_projection_tokens":metadata["contextBudget"]["finalTokens"],
                        "wire_bytes":reply.to_string().len(),
                        "latency_ms":started.elapsed().as_secs_f64() * 1000.0,
                        "source_preserved":source_preserved, "model_called":false
                    }));
                }
            }
        }
    }
    Ok(())
}
