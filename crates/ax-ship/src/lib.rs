//! Autonomous Git Command Center daemon.

mod auto_commit;
mod config;
mod events;
mod git_root;
mod git_watcher;
mod pipeline;
mod run_log;
mod state;
mod advanced;

pub use auto_commit::Checkpoint;
pub use ax_remote::{seed_ship_config, ShipSeedResult};
pub use config::ShipDaemonConfig;
pub use events::{ShipEvent, ShipEventBus};
pub use git_root::{
    diff_all_repos, discover_git_repos, resolve_git_root, resolve_git_root_from, resolve_git_roots,
    resolve_repo_base_branch, resolve_sonar_repo_names, sync_discovered_git_roots, MultiRepoDiff,
};
pub use pipeline::{PipelineResult, ShipPipeline};
pub use run_log::{read_run_log, LastRunLog};
pub use state::{GateStepStatus, ShipReport, ShipState};

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

pub struct ShipDaemon {
    pub project_root: PathBuf,
    pub config: Arc<Mutex<ax_remote::ShipConfig>>,
    pub bus: ShipEventBus,
    pub state: Arc<Mutex<ShipState>>,
    sonar_provisioning: Arc<AtomicBool>,
}

impl ShipDaemon {
    pub fn new(project_root: PathBuf) -> Self {
        run_log::finalize_stale_run_log(&project_root);
        let mut config = ax_remote::load_ship_config(&project_root);
        sync_discovered_git_roots(&project_root, &mut config);
        let config = Arc::new(Mutex::new(config));
        Self {
            project_root,
            config,
            bus: ShipEventBus::new(256),
            state: Arc::new(Mutex::new(ShipState::default())),
            sonar_provisioning: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn config(&self) -> ax_remote::ShipConfig {
        self.config.lock().await.clone()
    }

    pub async fn set_config(&self, cfg: ax_remote::ShipConfig) -> Result<(), String> {
        ax_remote::save_ship_config(&self.project_root, &cfg)?;
        *self.config.lock().await = cfg;
        Ok(())
    }

    pub async fn reload_config(&self) {
        let mut config = ax_remote::load_ship_config(&self.project_root);
        sync_discovered_git_roots(&self.project_root, &mut config);
        *self.config.lock().await = config;
    }

    /// Background Sonar provisioning from autodiscovered git repos (idempotent).
    pub fn spawn_sonar_auto_provision(self: &Arc<Self>) {
        if self
            .sonar_provisioning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let daemon = Arc::clone(self);
        tokio::spawn(async move {
            let result = daemon.auto_provision_sonar().await;
            daemon.sonar_provisioning.store(false, Ordering::SeqCst);
            if let Err(e) = result {
                tracing::warn!(error = %e, "Sonar auto-provision skipped");
            }
        });
    }

    pub async fn auto_provision_sonar(&self) -> Result<(), String> {
        let cfg = self.config.lock().await.clone();
        let repos = resolve_sonar_repo_names(&self.project_root, &cfg);
        ax_quality::auto_provision_sonar_from_discovery(&cfg.sonar, &self.project_root, &repos).await
    }

    pub async fn evaluate(&self) -> Result<ShipReport, String> {
        self.evaluate_with(|_| {}).await
    }

    /// Like `evaluate`, but lets the caller adjust the freshly-reloaded config
    /// for this single run only (e.g. CLI `--auto-commit` / `--revert-on-fail`
    /// flags) without persisting the override to `ship.toml`.
    pub async fn evaluate_with(
        &self,
        apply_overrides: impl FnOnce(&mut ax_remote::ShipConfig),
    ) -> Result<ShipReport, String> {
        self.reload_config().await;
        let mut cfg = self.config.lock().await.clone();
        apply_overrides(&mut cfg);
        let pipeline = ShipPipeline::new(self.project_root.clone(), cfg, self.bus.clone());
        let report = pipeline.run_evaluate().await?;
        *self.state.lock().await = ShipState::from_report(&report);
        self.bus.publish(ShipEvent::ReportUpdated { report: report.clone() });
        Ok(report)
    }

    pub async fn run_watch(&self) -> Result<(), String> {
        let cfg = self.config.lock().await.clone();
        let git_roots = resolve_git_roots(&self.project_root, &cfg)?;
        git_watcher::start_git_watcher(self.project_root.clone(), git_roots, self.bus.clone()).await?;
        Ok(())
    }
}

pub async fn evaluate_project(project_root: PathBuf) -> Result<ShipReport, String> {
    ShipDaemon::new(project_root).evaluate().await
}

/// Single-run auto-commit override for `ax ship --evaluate`'s CLI flags —
/// `None` means "leave `ship.toml` as-is" for that field.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoCommitOverride {
    pub enabled: Option<bool>,
    pub revert_on_fail: Option<bool>,
}

pub async fn evaluate_project_with_overrides(
    project_root: PathBuf,
    auto_commit: AutoCommitOverride,
) -> Result<ShipReport, String> {
    ShipDaemon::new(project_root)
        .evaluate_with(|cfg| {
            if let Some(v) = auto_commit.enabled {
                cfg.auto_commit.enabled = v;
            }
            if let Some(v) = auto_commit.revert_on_fail {
                cfg.auto_commit.revert_on_fail = v;
            }
        })
        .await
}
