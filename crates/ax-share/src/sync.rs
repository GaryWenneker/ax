//! Remote share sync engine — pull from provider, import pack + memory.

use std::path::Path;

use ax_memory::sync::import_shared;
use ax_policy::{export_pack, import_pack_with_options, index_policy, open_rw_pool};
use sqlx::SqlitePool;

use crate::config::{load_share_config, ShareConfig, ShareProvider};
use crate::paths::share_cache_dir;
use crate::providers::{github, gitlab_api, onedrive};
use crate::status::{now_secs, save_status, ShareSyncStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    Pull,
    Push,
    Both,
}

#[derive(Debug, Default)]
pub struct SyncRunResult {
    pub status: ShareSyncStatus,
}

pub async fn run_sync(
    project_root: &Path,
    pool: &SqlitePool,
    direction: SyncDirection,
) -> Result<SyncRunResult, String> {
    let config = load_share_config(project_root);
    let mut status = ShareSyncStatus {
        provider: Some(config.provider.as_str().to_string()),
        ..Default::default()
    };

    let cache = share_cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let work = cache.join("work");
    if work.exists() {
        std::fs::remove_dir_all(&work).ok();
    }
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;

    let (require_review, force) = config.import_mode.import_flags();

    if matches!(direction, SyncDirection::Pull | SyncDirection::Both) {
        let pull = match config.provider {
            ShareProvider::Github if gitlab_api::should_use_api(&config.github) => {
                gitlab_api::pull_gitlab_api(&config.github, &work)
                    .await
                    .map_err(|e| e.to_string())?
            }
            ShareProvider::Github => {
                github::pull_github(&config.github, &work).map_err(|e| e.to_string())?
            }
            ShareProvider::Onedrive => onedrive::pull_onedrive(&config.onedrive, &work)
                .await
                .map_err(|e| e.to_string())?,
        };
        status.remote_files = pull.files_copied;

        if config.content.rules || config.content.skills {
            if let Some(pack_dir) = pull.pack_dir {
                let import = import_pack_with_options(
                    pool,
                    project_root,
                    Some(&pack_dir),
                    force,
                    Some(require_review),
                )
                .await
                .map_err(|e| e.to_string())?;
                status.rules_added = import.rules_added + import.rules_updated;
                status.skills_added = import.skills_added + import.skills_updated;
                status.rules_pending = import.rules_pending;
                status.skills_pending = import.skills_pending;
            }
        }

        if config.content.memory {
            if let Some(mem) = pull.memory_file {
                let mem_result = import_shared(pool, &mem)
                    .await
                    .map_err(|e| e.to_string())?;
                status.memory_inserted = mem_result.inserted;
                status.memory_updated = mem_result.updated;
            }
        }

        let _ = index_policy(pool, project_root, false).await;
    }

    if matches!(direction, SyncDirection::Push | SyncDirection::Both) {
        push_to_remote(project_root, pool, &config).await?;
    }

    status.last_sync_at = Some(now_secs());
    status.last_error = None;
    save_status(project_root, &status)?;
    Ok(SyncRunResult { status })
}

async fn push_to_remote(
    project_root: &Path,
    pool: &SqlitePool,
    config: &ShareConfig,
) -> Result<(), String> {
    let cache = share_cache_dir().join("push");
    if cache.exists() {
        std::fs::remove_dir_all(&cache).ok();
    }
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;

    let pack_out = cache.join("policy").join("shared");
    if config.content.rules || config.content.skills {
        export_pack(pool, project_root, "shared", Some(&pack_out))
            .await
            .map_err(|e| e.to_string())?;
    }

    let memory_path = if config.content.memory {
        Some(ax_memory::sync::export_shared(pool, project_root, "shared", None)
            .await
            .map_err(|e| e.to_string())?
            .path)
    } else {
        None
    };

    match config.provider {
        ShareProvider::Onedrive => {
            onedrive::push_onedrive(
                &config.onedrive,
                &pack_out,
                memory_path.as_deref(),
            )
            .await?;
        }
        ShareProvider::Github if gitlab_api::should_use_api(&config.github) => {
            gitlab_api::push_gitlab_api(&config.github, &pack_out, memory_path.as_deref()).await?;
        }
        ShareProvider::Github => {
            github::push_github(&config.github, &pack_out, memory_path.as_deref())?;
        }
    }
    Ok(())
}

pub fn share_config_for_api(project_root: &Path) -> ShareConfig {
    load_share_config(project_root)
}

pub fn share_status_for_api(project_root: &Path) -> ShareSyncStatus {
    crate::status::load_status(project_root)
}

pub async fn open_policy_pool(project_root: &Path) -> Result<SqlitePool, String> {
    let db_path = project_root.join(".ax").join("ax.db");
    let pool = open_rw_pool(&db_path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(pool)
}
