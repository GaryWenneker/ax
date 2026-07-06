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
    pub config: ax_remote::ShipConfig,
    pub bus: ShipEventBus,
    pub state: Arc<Mutex<ShipState>>,
}

impl ShipDaemon {
    pub fn new(project_root: PathBuf) -> Self {
        let config = ax_remote::load_ship_config(&project_root);
        Self {
            project_root,
            config,
            bus: ShipEventBus::new(256),
            state: Arc::new(Mutex::new(ShipState::default())),
        }
    }

    pub async fn evaluate(&self) -> Result<ShipReport, String> {
        let pipeline = ShipPipeline::new(self.project_root.clone(), self.config.clone(), self.bus.clone());
        let report = pipeline.run_evaluate().await?;
        *self.state.lock().await = ShipState::from_report(&report);
        self.bus.publish(ShipEvent::ReportUpdated(report.clone()));
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
