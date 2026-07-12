//! Persisted log from the last Command Center evaluate run.

use std::path::{Path, PathBuf};

use crate::events::{ShipEvent, ShipEventBus};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LastRunLog {
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub ok: bool,
    pub lines: Vec<String>,
}

pub fn run_log_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("ship-last-run.json")
}

pub fn read_run_log(project_root: &Path) -> LastRunLog {
    let path = run_log_path(project_root);
    if !path.exists() {
        return LastRunLog::default();
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn write_run_log(project_root: &Path, log: &LastRunLog) -> Result<(), String> {
    let path = run_log_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(log).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| e.to_string())
}

pub struct RunLogger {
    project_root: PathBuf,
    log: LastRunLog,
    bus: Option<ShipEventBus>,
}

impl RunLogger {
    pub fn start(project_root: &Path, bus: Option<ShipEventBus>) -> Self {
        let mut log = LastRunLog::default();
        log.started_at = Some(now_label());
        log.ok = true;
        let logger = Self {
            project_root: project_root.to_path_buf(),
            log,
            bus,
        };
        logger.flush();
        logger
    }

    fn flush(&self) {
        let _ = write_run_log(&self.project_root, &self.log);
        if let Some(bus) = &self.bus {
            bus.publish(ShipEvent::RunLogUpdated {
                last_run: self.log.clone(),
            });
        }
    }

    pub fn line(&mut self, message: impl Into<String>) {
        let msg = message.into();
        self.log.lines.push(format!("[{}] {msg}", now_label()));
        self.flush();
    }

    pub fn step_start(&mut self, step: &str) {
        self.line(format!("▶ {step}"));
    }

    pub fn step_ok(&mut self, step: &str, detail: Option<&str>) {
        if let Some(d) = detail {
            self.line(format!("✓ {step} — {d}"));
        } else {
            self.line(format!("✓ {step}"));
        }
    }

    pub fn step_fail(&mut self, step: &str, detail: &str) {
        self.log.ok = false;
        self.line(format!("✕ {step} — {detail}"));
    }

    pub fn sonar_project_start(
        &mut self,
        index: usize,
        total: usize,
        project_key: &str,
        repo_name: &str,
    ) {
        self.line(format!(
            "▶ sonar:{project_key} — {repo_name} ({index}/{total})"
        ));
    }

    pub fn sonar_project_ok(&mut self, project_key: &str, repo_name: &str) {
        self.line(format!("✓ sonar:{project_key} — {repo_name}"));
    }

    pub fn sonar_project_fail(&mut self, project_key: &str, repo_name: &str, detail: &str) {
        self.log.ok = false;
        self.line(format!("✕ sonar:{project_key} — {repo_name}: {detail}"));
    }

    pub fn sonar_project_skip(&mut self, project_key: &str, repo_name: &str, reason: &str) {
        self.line(format!("– sonar:{project_key} — {repo_name} — skipped ({reason})"));
    }

    pub fn finish(mut self, ok: bool) -> LastRunLog {
        if !ok {
            self.log.ok = false;
        }
        self.log.finished_at = Some(now_label());
        self.flush();
        self.log
    }
}

fn now_label() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
