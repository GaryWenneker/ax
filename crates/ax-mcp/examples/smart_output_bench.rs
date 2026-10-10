//! Reproducible preflight wire-size benchmark; no agent quality score is inferred.
use ax_mcp::{server::handle_request, McpEngine, ToolHandler};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("output directory required")?;
    std::fs::create_dir_all(&out)?;
    let fixture_dir = std::env::args().nth(2).unwrap_or_else(|| out.clone());
    let budget: Option<u32> = std::env::args().nth(3).map(|n| n.parse()).transpose()?;
    for (name, rules, memories, files) in [
        ("simple", 1, 2, 1),
        ("medium", 4, 20, 4),
        ("complex", 12, 80, 12),
    ] {
        let root = std::path::PathBuf::from(&fixture_dir).join(format!("fixture-{name}"));
        std::fs::create_dir_all(root.join(".agents/rules"))?;
        for i in 0..rules {
            let body = format!("- Keep public API stable for module {i}.\n- Preserve error handling.\n- Test project isolation.\n");
            std::fs::write(
                root.join(format!(".agents/rules/rule-{i}.mdc")),
                format!("---\nid: rule-{i}\nlevel: CRITICAL\nalwaysApply: true\n---\n\n{body}"),
            )?;
        }
        for i in 0..files {
            std::fs::write(
                root.join(format!("module_{i}.rs")),
                format!("pub fn module_{i}(x: u32) -> u32 {{ x + {i} }}\n"),
            )?;
        }
        let existed = root.join(".ax/ax.db").exists();
        let ax = if existed {
            ax_core::Ax::open(&root).await?
        } else {
            ax_core::Ax::init(&root).await?
        };
        ax_policy::index_policy(ax.db_pool(), &root, true).await?;
        for i in 0..if existed { 0 } else { memories } {
            ax_memory::remember(
                ax.db_pool(),
                ax_memory::RememberInput {
                    title: format!("Module {i} architecture"),
                    body: format!("Keep module {i} dependencies explicit and validate its API."),
                    kind: Some("decision".into()),
                    tags: vec![],
                    files: vec![format!("module_{i}.rs")],
                    source: Some("benchmark".into()),
                },
            )
            .await?;
        }
        drop(ax);
        std::fs::write(
            root.join("ax.json"),
            serde_json::to_vec(&json!({"context":{"budgetTokens":budget}}))?,
        )?;
        let mut engine = McpEngine::with_project_root(root);
        handle_request(
            &mut engine,
            "initialize",
            json!({"clientInfo":{"name":"benchmark"}}),
        )
        .await;
        let args = json!({"prompt": "Review module 0 architecture and preserve public API", "files":["module_0.rs"], "session":format!("bench-{name}")});
        let started = std::time::Instant::now();
        let result = handle_request(
            &mut engine,
            "tools/call",
            json!({"name":"ax_preflight","arguments":args.clone()}),
        )
        .await
        .result?;
        let mut repeats = Vec::new();
        for turn in 2..=5 {
            let reply = handle_request(
                &mut engine,
                "tools/call",
                json!({"name":"ax_preflight","arguments":args.clone()}),
            )
            .await
            .result?;
            let text = reply["content"][0]["text"]
                .as_str()
                .ok_or("missing repeat text")?;
            repeats.push(json!({"turn":turn,"textTokens":ax_usage::count_tokens(text),"responseTokens":ax_usage::count_tokens(&reply.to_string()),"responseBytes":reply.to_string().len()}));
        }
        let text = result["content"][0]["text"]
            .as_str()
            .ok_or("missing text")?;
        let mut guard = engine.lock_ax().await;
        let ax = guard.as_mut().ok_or("missing engine")?;
        let raw = ToolHandler::call_tool(ax, "ax_preflight", args.clone()).await?;
        let critical: Vec<_> = raw["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|rule| {
                rule["level"]
                    .as_str()
                    .is_some_and(|l| l.eq_ignore_ascii_case("CRITICAL"))
            })
            .collect();
        let missing: Vec<_> = critical
            .iter()
            .filter(|rule| !text.contains(rule["body"].as_str().unwrap_or("").trim()))
            .map(|rule| rule["id"].clone())
            .collect();
        let rule_coverage = json!({"requiredCriticalRules":critical.len(),"completeCriticalRules":critical.len()-missing.len(),"missingFullBodies":missing});
        drop(guard);
        let measured = ax_usage::tokenizer_available();
        let stats = json!({"textBytes":text.len(),"responseBytes":serde_json::to_vec(&result)?.len(),"structuredBytes":result.get("structuredContent").map(|v| v.to_string().len()).unwrap_or(0),"textTokens":ax_usage::count_tokens(text),"responseTokens":ax_usage::count_tokens(&result.to_string()),"tokenizer":if measured {"o200k_base"} else {"estimate"},"requiredRulesPresent":(0..rules).all(|i| text.contains(&format!("Keep public API stable for module {i}."))),"outputTokens":null,"agentQuality":null,"toolCalls":1,"fiveTurnToolCalls":5,"ruleCoverage":rule_coverage,"durationMs":started.elapsed().as_millis(),"fiveTurnTextTokens":ax_usage::count_tokens(text)+repeats.iter().map(|t| t["textTokens"].as_u64().unwrap_or(0) as usize).sum::<usize>()});
        std::fs::write(
            format!("{out}/{name}.json"),
            serde_json::to_vec_pretty(
                &json!({"response":result,"metrics":stats,"repeatTurns":repeats}),
            )?,
        )?;
        println!("{name}: {stats}");
    }
    Ok(())
}
