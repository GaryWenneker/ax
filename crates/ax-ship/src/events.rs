//! SSE event bus for live dashboard updates.

use tokio::sync::broadcast;

use crate::run_log::LastRunLog;
use crate::state::ShipReport;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShipEvent {
    GitChanged { branch: Option<String> },
    StepStarted { step: String },
    StepFinished { step: String, ok: bool, detail: Option<String> },
    SonarProjectStarted {
        project_key: String,
        repo_name: String,
        index: usize,
        total: usize,
    },
    SonarProjectFinished {
        project_key: String,
        repo_name: String,
        ok: bool,
        detail: Option<String>,
    },
    SonarProjectSkipped {
        project_key: String,
        repo_name: String,
        reason: String,
    },
    RunLogUpdated { last_run: LastRunLog },
    ReportUpdated { report: ShipReport },
    Error { message: String },
}

#[derive(Clone)]
pub struct ShipEventBus {
    tx: broadcast::Sender<ShipEvent>,
}

impl ShipEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: ShipEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ShipEvent> {
        self.tx.subscribe()
    }
}
