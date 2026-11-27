//! ax remember / ax recall / ax history / ax memory export|import — memory vault CLI.

use std::path::PathBuf;

use crate::commands::resolve_project_root;

pub struct RememberArgs {
    pub text: Option<String>,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<String>,
    pub triggers: Vec<String>,
    pub confirm: Option<String>,
    pub purge: Option<String>,
    pub when: Option<String>,
    pub json: bool,
}

pub async fn run_remember(args: RememberArgs) -> Result<(), String> {
    let RememberArgs {
        text,
        title,
        kind,
        tags,
        files,
        triggers,
        confirm,
        purge,
        when,
        json,
    } = args;
    let root = resolve_project_root(None);
    let text = if confirm.is_none() && purge.is_none() {
        Some(text.ok_or("give the memory text, or --confirm ID, or --purge ID")?)
    } else {
        text
    };
    let trace = ax_usage::ToolTrace::begin(
        Some(&root),
        "ax_remember",
        &serde_json::json!({
            "body": text,
            "title": title,
            "kind": kind,
            "triggers": triggers,
        }),
    );
    ax_usage::log_memory(Some(&root), "remember start");
    let ax = match ax_core::Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            ax_usage::log_memory(Some(&root), "remember fail stage=open");
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };
    if let Some(id) = confirm {
        let confirmed = ax_memory::confirm_memory(ax.db_pool(), &id)
            .await
            .map_err(|e| e.to_string())?;
        if json {
            println!(
                "{}",
                serde_json::json!({ "ok": confirmed, "id": id, "confirmed": confirmed })
            );
        } else if confirmed {
            println!("Confirmed {id}");
        } else {
            println!("No candidate memory with id {id}");
        }
        return Ok(());
    }
    if let Some(id) = purge {
        let purged = ax_memory::purge_memory(ax.db_pool(), &id)
            .await
            .map_err(|e| e.to_string())?;
        if json {
            println!(
                "{}",
                serde_json::json!({ "ok": purged, "id": id, "purged": purged })
            );
        } else if purged {
            println!("Purged {id}");
        } else {
            println!("No memory with id {id}");
        }
        return Ok(());
    }
    let text = text.unwrap_or_default();
    let mut tags = tags;
    if let Some(when) = when {
        if let Some(clause) = ax_memory::parse_when(&when) {
            tags.push(format!("when:{}:{}", clause.kind, clause.pattern));
        } else {
            return Err(format!("invalid --when {when}"));
        }
    }
    if let Err(e) = ax_memory::assert_scope(&root, &files) {
        let msg = e.to_string();
        trace.finish_err(&msg);
        return Err(msg);
    }
    let row = match ax_memory::remember_with_triggers(
        ax.db_pool(),
        ax_memory::RememberInput {
            title: title.unwrap_or_default(),
            body: text,
            kind,
            tags,
            files,
            source: Some("manual".into()),
        },
        &triggers,
    )
    .await
    {
        Ok(row) => row,
        Err(e) => {
            ax_usage::log_memory(Some(&root), "remember fail");
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };

    ax_usage::log_memory(
        Some(&root),
        format!("remember ok id={} kind={}", row.id, row.kind),
    );
    let suggestions = ax_memory::suggested_triggers(&row.title, &row.files, &row.body, &triggers);
    let summary = format!("Remembered [{}] {}", row.kind, row.title);
    trace.finish_ok(&summary);

    // Near-identical memories are usually duplicates or contradictions.
    let similar = ax_memory::find_similar(
        ax.db_pool(),
        &format!("{} {}", row.title, row.body),
        Some(&row.id),
        0.80,
        3,
    )
    .await
    .unwrap_or_default();

    if json {
        let out = serde_json::json!({
            "ok": true,
            "id": row.id,
            "title": row.title,
            "kind": row.kind,
            "memory": row,
            "similar": similar,
            "suggested_triggers": suggestions
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        println!("Remembered [{}] {}", row.kind, row.title);
        println!("  id: {}", row.id);
        if !row.tags.is_empty() {
            println!("  tags: {}", row.tags.join(", "));
        }
        if !similar.is_empty() {
            println!("\n  Similar existing memories (possible duplicate/contradiction):");
            for s in &similar {
                println!(
                    "  - [{:.0}% similar] {} ({})",
                    s.score * 100.0,
                    s.memory.title,
                    s.memory.id
                );
            }
        }
        if !suggestions.is_empty() {
            println!("  suggested triggers: {}", suggestions.join(", "));
        }
    }
    Ok(())
}

pub async fn run_capture_git(limit: Option<u32>, quiet: bool, json: bool) -> Result<(), String> {
    let root = resolve_project_root(None);
    ax_usage::log_memory(
        Some(&root),
        format!("capture-git start limit={}", limit.unwrap_or(100)),
    );
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = match ax_memory::capture_git_history(
        ax.db_pool(),
        ax.project_root(),
        limit.unwrap_or(100) as usize,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_memory(Some(&root), "capture-git fail");
            return Err(e.to_string());
        }
    };
    ax_usage::log_memory(
        Some(&root),
        format!(
            "capture-git ok scanned={} captured={}",
            result.scanned, result.captured
        ),
    );

    if quiet {
        return Ok(());
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "Git capture: {} commits scanned, {} new memories, {} already captured, {} trivial skipped.",
            result.scanned, result.captured, result.skipped_existing, result.skipped_trivial
        );
    }
    Ok(())
}

pub async fn run_history(
    query: Option<String>,
    since: Option<String>,
    limit: Option<u32>,
    id: Option<String>,
    depth: String,
    json: bool,
) -> Result<(), String> {
    let root = resolve_project_root(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let by_id = id.is_some();
    let entries = if let Some(id) = id {
        let entry = ax_memory::history_entry(ax.db_pool(), &id)
            .await
            .map_err(|e| e.to_string())?;
        let Some(entry) = entry else {
            return Err(format!("No turn memory with id {id}."));
        };
        vec![entry]
    } else {
        let query = query
            .filter(|q| !q.trim().is_empty())
            .ok_or("give a file, symbol, or topic (or --id)")?;
        let since_ms = match since.as_deref() {
            Some(date) => Some(ax_memory::parse_since(date).ok_or("--since must be YYYY-MM-DD")?),
            None => None,
        };
        let history_query = ax_memory::HistoryQuery {
            query,
            since_ms,
            limit: limit.unwrap_or(20).clamp(1, 50) as usize,
            archive: ax_memory::Depth::parse(&depth)? == ax_memory::Depth::Archive,
        };
        ax_memory::history(ax.db_pool(), ax.project_root(), &history_query)
            .await
            .map_err(|e| e.to_string())?
    };
    if json {
        println!(
            "{}",
            serde_json::json!({ "entries": entries, "count": entries.len() })
        );
        return Ok(());
    }
    let outcome_chars = if by_id { usize::MAX } else { 600 };
    println!("{}", ax_memory::format_history(&entries, outcome_chars));
    Ok(())
}

pub async fn run_recall(
    query: Option<String>,
    cue: Option<String>,
    id: Option<String>,
    depth: String,
    limit: Option<u32>,
    filter: ax_memory::MemoryFilter,
    json: bool,
) -> Result<(), String> {
    let root = resolve_project_root(None);
    let query = query.unwrap_or_default();
    let q_len = query.chars().count();
    let trace = ax_usage::ToolTrace::begin(
        Some(&root),
        "ax_recall",
        &serde_json::json!({ "query": query, "cue": cue, "id": id, "depth": depth, "limit": limit }),
    );
    ax_usage::log_memory(Some(&root), format!("recall start q_len={q_len}"));
    let ax = match ax_core::Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };
    let limit = limit.unwrap_or(5).min(50) as usize;
    let depth = ax_memory::Depth::parse(&depth)?;
    if id.as_deref().unwrap_or("").is_empty()
        && cue.as_deref().unwrap_or("").is_empty()
        && query.trim().is_empty()
    {
        return Err("give a query, --cue, or --id".into());
    }
    let request = ax_memory::SearchRequest {
        query: query.clone(),
        cue,
        id,
        depth,
        files: Vec::new(),
        limit,
    };
    let mut matches = match ax_memory::search(ax.db_pool(), &request).await {
        Ok(m) => m,
        Err(e) => {
            ax_usage::log_memory(Some(&root), "recall fail");
            let msg = e.to_string();
            trace.finish_err(&msg);
            return Err(msg);
        }
    };
    if let Err(e) = ax_memory::apply_memory_filter(ax.db_pool(), &mut matches, &filter).await {
        let msg = e.to_string();
        trace.finish_err(&msg);
        return Err(msg);
    }
    ax_usage::log_memory(
        Some(&root),
        format!("recall ok hits={} limit={limit}", matches.len()),
    );
    trace.finish_ok(&format!("{} memories", matches.len()));

    if json {
        let cues = match request.id.as_deref().filter(|id| !id.is_empty()) {
            Some(id) => ax_memory::cues_for(ax.db_pool(), id)
                .await
                .unwrap_or_default(),
            None => Vec::new(),
        };
        println!(
            "{}",
            serde_json::json!({ "matches": matches, "cues": cues })
        );
        return Ok(());
    }

    if matches.is_empty() {
        println!("No memories match \"{query}\". Store one with: ax remember \"...\"");
        return Ok(());
    }

    println!(
        "{} memor{} for \"{}\":\n",
        matches.len(),
        if matches.len() == 1 { "y" } else { "ies" },
        query
    );
    for m in &matches {
        let age_days = (now_ms() - m.memory.updated_at).max(0) / 86_400_000;
        println!(
            "[{}] {}  (score {:.1}, {}d old)",
            m.memory.kind, m.memory.title, m.score, age_days
        );
        for line in m.memory.body.lines().take(4) {
            println!("    {line}");
        }
        if !m.memory.files.is_empty() {
            println!("    files: {}", m.memory.files.join(", "));
        }
        println!();
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub async fn run_export(
    tag: Option<String>,
    out: Option<String>,
    quiet: bool,
) -> Result<(), String> {
    let root = resolve_project_root(None);
    ax_usage::log_memory(Some(&root), "export start");
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let tag = tag.unwrap_or_else(|| "shared".into());
    let out_path = out.map(PathBuf::from);
    let result =
        match ax_memory::export_shared(ax.db_pool(), ax.project_root(), &tag, out_path.as_deref())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                ax_usage::log_memory(Some(&root), "export fail");
                return Err(e.to_string());
            }
        };
    ax_usage::log_memory(
        Some(&root),
        format!("export ok written={} tag={tag}", result.written),
    );
    if !quiet {
        println!(
            "Exported {} memor{} tagged \"{}\" → {}",
            result.written,
            if result.written == 1 { "y" } else { "ies" },
            tag,
            result.path.display()
        );
    }
    Ok(())
}

pub async fn run_import(path: Option<String>, quiet: bool) -> Result<(), String> {
    let root = resolve_project_root(None);
    ax_usage::log_memory(Some(&root), "import start");
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| ax_memory::default_shared_path(ax.project_root()));
    let result = match ax_memory::import_shared(ax.db_pool(), &path).await {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_memory(Some(&root), "import fail");
            return Err(e.to_string());
        }
    };
    ax_usage::log_memory(
        Some(&root),
        format!(
            "import ok inserted={} updated={} skipped={}",
            result.inserted, result.updated, result.skipped
        ),
    );
    if !quiet {
        println!(
            "Imported memories from {}: {} new, {} updated, {} skipped",
            path.display(),
            result.inserted,
            result.updated,
            result.skipped
        );
    }
    Ok(())
}

pub async fn run_reindex(json: bool) -> Result<(), String> {
    let root = resolve_project_root(None);
    ax_usage::log_memory(Some(&root), "reindex start");
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let updated = ax_memory::reindex_memory_fts(ax.db_pool())
        .await
        .map_err(|e| e.to_string())?;
    ax_usage::log_memory(Some(&root), format!("reindex ok updated={updated}"));
    if json {
        println!(
            "{}",
            serde_json::json!({ "ok": true, "search_text_updated": updated })
        );
    } else {
        println!("Rebuilt memories_fts; updated search_text on {updated} rows");
    }
    Ok(())
}
