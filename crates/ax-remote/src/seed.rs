//! Default Command Center config — embedded at compile time, written on `ax init`.

use std::path::Path;

const TEMPLATE: &str = include_str!("../ship.toml.example");

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShipSeedResult {
    pub created: Vec<String>,
    pub skipped: Vec<String>,
}

/// Write default `.ax/ship.toml` when missing. Never overwrites existing files.
pub fn seed_ship_config(ax_dir: &Path) -> std::io::Result<ShipSeedResult> {
    std::fs::create_dir_all(ax_dir)?;
    let dest = ax_dir.join("ship.toml");
    let rel = "ship.toml".to_string();
    let mut result = ShipSeedResult::default();
    if dest.exists() {
        result.skipped.push(rel);
        return Ok(result);
    }
    std::fs::write(&dest, TEMPLATE.as_bytes())?;
    result.created.push(rel);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ax_dir() -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ax-ship-seed-{n}"))
    }

    #[test]
    fn creates_ship_toml_once() {
        let ax = temp_ax_dir();
        let first = seed_ship_config(&ax).unwrap();
        assert_eq!(first.created, vec!["ship.toml"]);
        assert!(first.skipped.is_empty());
        assert!(ax.join("ship.toml").exists());

        let second = seed_ship_config(&ax).unwrap();
        assert!(second.created.is_empty());
        assert_eq!(second.skipped, vec!["ship.toml"]);

        let _ = std::fs::remove_dir_all(&ax);
    }
}
