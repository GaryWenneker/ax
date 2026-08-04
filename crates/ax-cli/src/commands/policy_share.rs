//! CLI commands for remote policy share sync.

use ax_share::{
    load_share_config, microsoft_auth_status, microsoft_clear_tokens, poll_device_flow_once,
    project_config_path, run_sync as engine_run_sync, share_status_for_api, start_device_flow,
    write_project_share_config, ShareConfig, SyncDirection,
};

use crate::commands::resolve_path;

pub async fn run_config(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let cfg = load_share_config(&root);
    let config_path = project_config_path(&root);
    if json {
        let out = serde_json::json!({
            "scope": "project",
            "configPath": config_path.display().to_string(),
            "provider": cfg.provider.as_str(),
            "importMode": cfg.import_mode.as_str(),
            "autoSyncMinutes": cfg.auto_sync_minutes,
            "content": cfg.content,
            "onedrive": cfg.onedrive,
            "github": cfg.github,
        });
        println!("{}", serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?);
    } else {
        println!("Scope: project ({})", config_path.display());
        println!("Provider: {}", cfg.provider.as_str());
        println!("Import mode: {}", cfg.import_mode.as_str());
        println!("Auto sync (min): {}", cfg.auto_sync_minutes);
        println!(
            "Content: rules={} skills={} memory={}",
            cfg.content.rules, cfg.content.skills, cfg.content.memory
        );
        match cfg.provider {
            ax_share::ShareProvider::Onedrive => {
                println!("OneDrive URL: {}", cfg.onedrive.share_url);
            }
            ax_share::ShareProvider::Github => {
                println!("GitHub repo: {}", cfg.github.repo_url);
                println!("Branch: {}", cfg.github.branch);
                println!("Subpath: {}", cfg.github.subpath);
            }
        }
        let st = share_status_for_api(&root);
        if let Some(ts) = st.last_sync_at {
            println!("Last sync: {ts}");
        }
        if let Some(err) = st.last_error {
            println!("Last error: {err}");
        }
    }
    Ok(())
}

pub async fn run_sync(
    path: Option<String>,
    pull: bool,
    push: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let pool = ax_share::open_policy_pool(&root).await?;
    let direction = if push && pull {
        SyncDirection::Both
    } else if push {
        SyncDirection::Push
    } else {
        SyncDirection::Pull
    };
    let result = engine_run_sync(&root, &pool, direction).await?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result.status).map_err(|e| e.to_string())?
        );
    } else {
        let s = &result.status;
        println!(
            "Sync complete: +{} rules, +{} skills, pending {}/{}, memory +{}/~{}",
            s.rules_added,
            s.skills_added,
            s.rules_pending,
            s.skills_pending,
            s.memory_inserted,
            s.memory_updated
        );
    }
    Ok(())
}

pub async fn run_config_save(path: Option<String>, config_json: &str) -> Result<(), String> {
    let root = resolve_path(path);
    let cfg: ShareConfig = serde_json::from_str(config_json).map_err(|e| e.to_string())?;
    let saved = write_project_share_config(&root, &cfg)?;
    println!("Saved share config to {}", saved.display());
    Ok(())
}

pub async fn ms_login() -> Result<(), String> {
    let start = start_device_flow().await?;
    println!("{}", start.message);
    println!(
        "Open {} and enter code: {}",
        start.verification_uri, start.user_code
    );
    let interval = start.interval;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        match poll_device_flow_once().await {
            Ok(Some(store)) => {
                println!(
                    "Signed in as {}",
                    store
                        .account
                        .unwrap_or_else(|| "Microsoft account".into())
                );
                return Ok(());
            }
            Ok(None) => continue,
            Err(e) => return Err(e),
        }
    }
}

pub fn ms_logout() -> Result<(), String> {
    microsoft_clear_tokens()?;
    println!("Signed out of Microsoft");
    Ok(())
}

pub fn ms_status(json: bool) -> Result<(), String> {
    let st = microsoft_auth_status();
    if json {
        println!("{}", serde_json::to_string_pretty(&st).map_err(|e| e.to_string())?);
    } else if st.signed_in {
        println!(
            "Signed in: {}",
            st.account.unwrap_or_else(|| "Microsoft account".into())
        );
    } else {
        println!("Not signed in");
    }
    Ok(())
}
