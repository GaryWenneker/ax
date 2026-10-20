use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSettings {
    #[serde(default)]
    pub report_installs: bool,
    #[serde(default)]
    pub publish_remote: String,
}

pub fn settings_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".ax").join("skill-catalog.json")
}

pub fn load_settings(project_root: &Path) -> Result<CatalogSettings, String> {
    let path = settings_path(project_root);
    if !path.exists() {
        return Ok(CatalogSettings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("skill catalog settings: {e}"))
}

pub fn save_settings(project_root: &Path, settings: &CatalogSettings) -> Result<(), String> {
    let path = settings_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let settings = load_settings(dir.path()).unwrap();
        assert!(!settings.report_installs);
    }
}
