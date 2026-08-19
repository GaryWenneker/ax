//! Documentation catalog sync — AzDO wiki + workspace docs into ax.db.

mod config;
mod memories;
mod scan;

pub use config::DocsCatalogConfig;
pub use memories::MemorySummary;

use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ax_extraction::orchestrator::IndexOptions;
use ax_memory::{import_shared, MemoryImportResult, MemoryRow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Ax(#[from] ax_utils::errors::AxError),
}

impl SyncError {
    fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub wiki_action: String,
    pub wiki_pages: usize,
    pub wiki_sections: usize,
    pub integratie_pages: usize,
    pub digitale_producten: usize,
    pub docs_sections: usize,
    pub skills: usize,
    pub script_readmes: usize,
    pub memories_built: usize,
    pub import_inserted: usize,
    pub import_updated: usize,
    pub import_skipped: usize,
    pub sync_files_indexed: usize,
    pub duration_ms: u64,
    pub jsonl_path: String,
    pub dry_run: bool,
    pub memories: Vec<MemorySummary>,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Info(String),
    Ok(String),
    Warn(String),
    Dim(String),
    Section(String),
    Kv { key: String, value: String },
}

pub struct SyncOptions {
    pub skip_wiki_pull: bool,
    pub dry_run: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            skip_wiki_pull: false,
            dry_run: false,
        }
    }
}

/// Sync documentation catalog into ax.db. Emits progress events when `on_event` is set.
pub async fn sync_catalog(
    project_root: &Path,
    opts: SyncOptions,
    mut on_event: Option<Box<dyn FnMut(SyncEvent) + Send>>,
) -> Result<SyncReport, SyncError> {
    let mut emit = |ev: SyncEvent| {
        if let Some(ref mut cb) = on_event {
            cb(ev);
        }
    };

    let started = Instant::now();
    let cfg = DocsCatalogConfig::load(project_root).map_err(SyncError::msg)?;

    emit(SyncEvent::Info(
        "Documentation catalog sync (ax.db tag: documentation-catalog)".into(),
    ));
    emit(SyncEvent::Dim(format!("Workspace: {}", project_root.display())));

    // 1. Wiki
    emit(SyncEvent::Section("1. AzDO wiki".into()));
    let wiki_action = if opts.skip_wiki_pull {
        if !cfg.wiki_local.exists() {
            return Err(SyncError::msg(format!(
                "wiki clone missing at {} — run without --skip-wiki-pull",
                cfg.wiki_local.display()
            )));
        }
        emit(SyncEvent::Warn("Wiki pull skipped (--skip-wiki-pull)".into()));
        emit(SyncEvent::Dim(format!(
            "{} (existing clone)",
            cfg.wiki_local.display()
        )));
        "skipped".into()
    } else {
        sync_wiki_repo(&cfg.wiki_remote, &cfg.wiki_local, &mut emit)?
    };

    let wiki_apps = cfg.wiki_local.join(&cfg.wiki_apps_subdir);
    let wiki_pages = scan::wiki_page_paths(&cfg.wiki_local);
    let wiki_sections = scan::wiki_top_sections(&wiki_apps);
    let integratie_root = wiki_apps.join("Integraties");
    let integratie_pages: Vec<String> = scan::wiki_page_paths(&integratie_root)
        .into_iter()
        .map(|p| p.trim_end_matches(".md").to_string())
        .collect();
    let digitale_producten =
        scan::digitale_producten_names(&wiki_apps.join("Digitale-Producten.md"));

    emit(SyncEvent::Kv {
        key: "Wiki root".into(),
        value: format!("{} (752)", cfg.wiki_apps_subdir),
    });
    emit(SyncEvent::Kv {
        key: "Pages".into(),
        value: wiki_pages.len().to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Top sections".into(),
        value: wiki_sections.len().to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Integraties".into(),
        value: integratie_pages.len().to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Digitale prod.".into(),
        value: digitale_producten.len().to_string(),
    });

    // 2. Workspace scan
    emit(SyncEvent::Section("2. Workspace scan".into()));
    let docs_sections = scan::docs_sections(&cfg.docs_root);
    let skill_names = scan::skill_names(&cfg.skills_root);
    let script_readmes = scan::script_dirs_with_readme(&cfg.scripts_root);

    emit(SyncEvent::Ok("Scanned local documentation sources".into()));
    emit(SyncEvent::Kv {
        key: ".docs sections".into(),
        value: docs_sections.len().to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Agent skills".into(),
        value: skill_names.len().to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Script READMEs".into(),
        value: script_readmes.len().to_string(),
    });

    let synced_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let scan_data = memories::ScanData {
        wiki_root_url: cfg.wiki_root_url.clone(),
        wiki_pages: wiki_pages.len(),
        wiki_sections: wiki_sections.clone(),
        integratie_pages: integratie_pages.clone(),
        digitale_producten: digitale_producten.clone(),
        docs_sections: docs_sections.clone(),
        skill_names: skill_names.clone(),
        script_readmes: script_readmes.clone(),
        synced_at,
    };

    // 3. Build memories
    emit(SyncEvent::Section("3. Build memory blocks".into()));
    let memory_rows = memories::build_memories(&scan_data);
    let summaries: Vec<MemorySummary> = memory_rows
        .iter()
        .map(|m| MemorySummary {
            id: m.id.clone(),
            kind: m.kind.clone(),
            title: m.title.clone(),
        })
        .collect();

    emit(SyncEvent::Ok(format!(
        "Built {} memory blocks",
        memory_rows.len()
    )));

    // 4. Write JSONL
    emit(SyncEvent::Section("4. Write JSONL".into()));
    let jsonl_path = cfg.jsonl_path.clone();
    if let Some(parent) = jsonl_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SyncError::msg(e.to_string()))?;
    }
    write_jsonl(&jsonl_path, &memory_rows)?;
    emit(SyncEvent::Ok("Wrote .ax/memory/documentation-catalog.jsonl".into()));
    emit(SyncEvent::Dim(jsonl_path.display().to_string()));

    if opts.dry_run {
        emit(SyncEvent::Warn("Dry run — skipped ax memory import and ax sync".into()));
        return Ok(SyncReport {
            wiki_action,
            wiki_pages: wiki_pages.len(),
            wiki_sections: wiki_sections.len(),
            integratie_pages: integratie_pages.len(),
            digitale_producten: digitale_producten.len(),
            docs_sections: docs_sections.len(),
            skills: skill_names.len(),
            script_readmes: script_readmes.len(),
            memories_built: memory_rows.len(),
            import_inserted: 0,
            import_updated: 0,
            import_skipped: 0,
            sync_files_indexed: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            jsonl_path: jsonl_path.display().to_string(),
            dry_run: true,
            memories: summaries,
        });
    }

    // 5. Import
    emit(SyncEvent::Section("5. Import into ax.db".into()));
    let mut ax = ax_core::Ax::open(project_root).await?;
    let import: MemoryImportResult = import_shared(ax.db_pool(), &jsonl_path).await?;
    emit(SyncEvent::Ok("Imported memories into ax.db".into()));
    emit(SyncEvent::Kv {
        key: "New".into(),
        value: import.inserted.to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Updated".into(),
        value: import.updated.to_string(),
    });
    emit(SyncEvent::Kv {
        key: "Skipped".into(),
        value: import.skipped.to_string(),
    });

    // 6. Graph sync
    emit(SyncEvent::Section("6. Sync graph".into()));
    let sync_result = ax
        .sync(
            IndexOptions {
                quiet: true,
                ..Default::default()
            },
            None,
        )
        .await?;
    emit(SyncEvent::Ok(format!(
        "Graph sync complete ({} files)",
        sync_result.files_indexed
    )));

    let duration_ms = started.elapsed().as_millis() as u64;
    emit(SyncEvent::Ok(format!(
        "Documentation catalog synced ({:.1}s)",
        duration_ms as f64 / 1000.0
    )));

    Ok(SyncReport {
        wiki_action,
        wiki_pages: wiki_pages.len(),
        wiki_sections: wiki_sections.len(),
        integratie_pages: integratie_pages.len(),
        digitale_producten: digitale_producten.len(),
        docs_sections: docs_sections.len(),
        skills: skill_names.len(),
        script_readmes: script_readmes.len(),
        memories_built: memory_rows.len(),
        import_inserted: import.inserted,
        import_updated: import.updated,
        import_skipped: import.skipped,
        sync_files_indexed: sync_result.files_indexed as usize,
        duration_ms,
        jsonl_path: jsonl_path.display().to_string(),
        dry_run: false,
        memories: summaries,
    })
}

fn sync_wiki_repo(
    remote: &str,
    local: &Path,
    emit: &mut impl FnMut(SyncEvent),
) -> Result<String, SyncError> {
    if !local.exists() {
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SyncError::msg(e.to_string()))?;
        }
        let status = Command::new("git")
            .args(["clone", remote])
            .arg(local)
            .status()
            .map_err(|e| SyncError::msg(format!("git clone failed: {e}")))?;
        if !status.success() {
            return Err(SyncError::msg(format!(
                "git clone failed for {remote}"
            )));
        }
        emit(SyncEvent::Ok("Cloned Frontends-Algemeen.wiki".into()));
        emit(SyncEvent::Dim(local.display().to_string()));
        return Ok("cloned".into());
    }

    let output = Command::new("git")
        .current_dir(local)
        .args(["pull", "--ff-only"])
        .output()
        .map_err(|e| SyncError::msg(format!("git pull failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(SyncError::msg(format!(
            "git pull failed: {stdout}{stderr}"
        )));
    }
    emit(SyncEvent::Ok("Pulled latest wiki changes".into()));
    emit(SyncEvent::Dim(local.display().to_string()));
    Ok("pulled".into())
}

fn write_jsonl(path: &Path, rows: &[MemoryRow]) -> Result<(), SyncError> {
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        let line = serde_json::to_string(row).map_err(|e| SyncError::msg(e.to_string()))?;
        lines.push(line);
    }
    std::fs::write(path, lines.join("\n") + "\n").map_err(|e| SyncError::msg(e.to_string()))?;
    Ok(())
}

