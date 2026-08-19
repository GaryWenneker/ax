//! `ax docs-catalog` — sync AzDO wiki + workspace docs into ax.db.

use ax_docs_catalog::{sync_catalog, SyncEvent, SyncOptions, SyncReport};

use crate::commands::resolve_path;
use crate::ui::{dim, info_line, kv_line, ok_line, warn_line};

pub async fn run_sync(skip_wiki_pull: bool, dry_run: bool, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let report = sync_catalog(
        &root,
        SyncOptions {
            skip_wiki_pull,
            dry_run,
        },
        Some(Box::new(|ev| print_event(ev))),
    )
    .await
    .map_err(|e| e.to_string())?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return Ok(());
    }

    print_summary(&report);
    Ok(())
}

fn print_event(ev: SyncEvent) {
    match ev {
        SyncEvent::Info(msg) => {
            println!();
            println!("{}", info_line(msg));
        }
        SyncEvent::Ok(msg) => println!("{}", ok_line(msg)),
        SyncEvent::Warn(msg) => println!("{}", warn_line(msg)),
        SyncEvent::Dim(msg) => println!("  {}", dim(msg)),
        SyncEvent::Section(title) => {
            println!();
            println!("{}", title);
        }
        SyncEvent::Kv { key, value } => println!("  {}", kv_line(key, value)),
    }
}

fn print_summary(report: &SyncReport) {
    println!();
    println!("{}", kv_line("Memories", format!(
        "{} blocks (tag: documentation-catalog)",
        report.memories_built
    )));
    println!(
        "  {}",
        kv_line("Wiki pages", report.wiki_pages.to_string())
    );
    println!(
        "  {}",
        kv_line("JSONL", ".ax/memory/documentation-catalog.jsonl")
    );
    println!();
    println!("Next steps:");
    println!("  ax recall documentation-catalog");
    println!("  Skill: vfpf-docs-catalog");
    println!();
    println!(
        "  {}",
        dim("Done! Agents will inject catalog via ax_preflight / ax_recall.")
    );
    println!();
}
