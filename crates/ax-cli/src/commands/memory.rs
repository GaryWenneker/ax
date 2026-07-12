//! ax remember / ax recall — memory vault CLI.

use crate::commands::resolve_path;

pub async fn run_remember(
    text: String,
    title: Option<String>,
    kind: Option<String>,
    tags: Vec<String>,
    files: Vec<String>,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let row = ax_memory::remember(
        ax.db_pool(),
        ax_memory::RememberInput {
            title: title.unwrap_or_default(),
            body: text,
            kind,
            tags,
            files,
            source: Some("manual".into()),
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    // Near-identical memories are usually duplicates or contradictions.
    let similar = ax_memory::find_similar(ax.db_pool(), &format!("{} {}", row.title, row.body), Some(&row.id), 0.80, 3)
        .await
        .unwrap_or_default();

    if json {
        let out = serde_json::json!({ "memory": row, "similar": similar });
        println!("{}", serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?);
    } else {
        println!("Remembered [{}] {}", row.kind, row.title);
        println!("  id: {}", row.id);
        if !row.tags.is_empty() {
            println!("  tags: {}", row.tags.join(", "));
        }
        if !similar.is_empty() {
            println!("\n  Similar existing memories (possible duplicate/contradiction):");
            for s in &similar {
                println!("  - [{:.0}% similar] {} ({})", s.score * 100.0, s.memory.title, s.memory.id);
            }
        }
    }
    Ok(())
}

pub async fn run_capture_git(limit: Option<u32>, quiet: bool, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = ax_memory::capture_git_history(ax.db_pool(), ax.project_root(), limit.unwrap_or(100) as usize)
        .await
        .map_err(|e| e.to_string())?;

    if quiet {
        return Ok(());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
    } else {
        println!(
            "Git capture: {} commits scanned, {} new memories, {} already captured, {} trivial skipped.",
            result.scanned, result.captured, result.skipped_existing, result.skipped_trivial
        );
    }
    Ok(())
}

pub async fn run_recall(query: String, limit: Option<u32>, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(5).min(50) as usize;
    let matches = ax_memory::recall(ax.db_pool(), &query, limit)
        .await
        .map_err(|e| e.to_string())?;

    if json {
        println!("{}", serde_json::to_string_pretty(&matches).map_err(|e| e.to_string())?);
        return Ok(());
    }

    if matches.is_empty() {
        println!("No memories match \"{query}\". Store one with: ax remember \"...\"");
        return Ok(());
    }

    println!("{} memor{} for \"{}\":\n", matches.len(), if matches.len() == 1 { "y" } else { "ies" }, query);
    for m in &matches {
        let age_days = (now_ms() - m.memory.updated_at).max(0) / 86_400_000;
        println!("[{}] {}  (score {:.1}, {}d old)", m.memory.kind, m.memory.title, m.score, age_days);
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
