//! Per-target install results for structured CLI output.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Created,
    Updated,
    Unchanged,
    Skipped,
}

impl FileAction {
    pub fn verb(self) -> &'static str {
        match self {
            FileAction::Created => "Created",
            FileAction::Updated => "Updated",
            FileAction::Unchanged => "Unchanged",
            FileAction::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstalledFile {
    pub path: PathBuf,
    pub action: FileAction,
}

#[derive(Debug, Clone)]
pub struct TargetReport {
    pub id: &'static str,
    pub display_name: &'static str,
    pub files: Vec<InstalledFile>,
    pub notes: Vec<String>,
}

impl TargetReport {
    pub fn new(id: &'static str, display_name: &'static str) -> Self {
        Self {
            id,
            display_name,
            files: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn push_file(&mut self, path: PathBuf, action: FileAction) {
        if action == FileAction::Skipped {
            return;
        }
        self.files.push(InstalledFile { path, action });
    }

    pub fn note(&mut self, msg: impl Into<String>) {
        self.notes.push(msg.into());
    }

    pub fn touched(&self) -> bool {
        self.files.iter().any(|f| {
            matches!(
                f.action,
                FileAction::Created | FileAction::Updated | FileAction::Unchanged
            )
        })
    }
}

pub struct InstallSummary {
    pub reports: Vec<TargetReport>,
}

impl InstallSummary {
    pub fn configured_targets(&self) -> Vec<&str> {
        self.reports
            .iter()
            .filter(|r| r.touched())
            .map(|r| r.id)
            .collect()
    }
}
