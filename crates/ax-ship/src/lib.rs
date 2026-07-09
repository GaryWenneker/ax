//! Autonomous Git Command Center daemon.

mod config;
mod events;
mod git_watcher;
mod pipeline;
mod state;
mod advanced;

pub use ax_remote::{seed_ship_config, ShipSeedResult};
pub use config::ShipDaemonConfig;
pub use events::{ShipEvent, ShipEventBus};
pub use pipeline::{PipelineResult, ShipPipeline};
pub use state::{GateStepStatus, ShipReport, ShipState};

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

pub struct ShipDaemon {
    pub project_root: PathBuf,
    pub config: Arc<Mutex<ax_remote::ShipConfig>>,
    pub bus: ShipEventBus,
    pub state: Arc<Mutex<ShipState>>,
}

impl ShipDaemon {
    pub fn new(project_root: PathBuf) -> Self {
        let config = Arc::new(Mutex::new(ax_remote::load_ship_config(&project_root)));
        Self {
            project_root,
            config,
            bus: ShipEventBus::new(256),
            state: Arc::new(Mutex::new(ShipState::default())),
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
        *self.config.lock().await = ax_remote::load_ship_config(&self.project_root);
    }

    pub async fn evaluate(&self) -> Result<ShipReport, String> {
        let cfg = self.config.lock().await.clone();
        let pipeline = ShipPipeline::new(self.project_root.clone(), cfg, self.bus.clone());
        let report = pipeline.run_evaluate().await?;
        *self.state.lock().await = ShipState::from_report(&report);
        self.bus.publish(ShipEvent::ReportUpdated { report: report.clone() });
        Ok(report)
    }

    pub async fn run_watch(&self) -> Result<(), String> {
        git_watcher::start_git_watcher(self.project_root.clone(), self.bus.clone()).await?;
        Ok(())
    }
}

pub async fn evaluate_project(project_root: PathBuf) -> Result<ShipReport, String> {
    ShipDaemon::new(project_root).evaluate().await
}
