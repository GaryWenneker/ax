//! `ax telemetry` — anonymous usage telemetry controls.

use ax_telemetry::{telemetry, TELEMETRY_DOCS};

pub fn run(action: Option<String>) -> Result<(), String> {
    let mut t = telemetry().lock().map_err(|e| e.to_string())?;
    let act = action.unwrap_or_else(|| "status".to_string());
    match act.as_str() {
        "on" => {
            t.set_enabled(true, "cli");
            println!("Telemetry enabled. See {}", TELEMETRY_DOCS);
        }
        "off" => {
            t.set_enabled(false, "cli");
            println!("Telemetry disabled. No data will be sent.");
        }
        "status" => {
            let s = t.get_status();
            println!("enabled: {}", s.enabled);
            println!("decided_by: {}", s.decided_by);
            if let Some(id) = s.machine_id {
                println!("machine_id: {}", id);
            }
            println!("config: {}", s.config_path.display());
        }
        other => return Err(format!("unknown action '{other}' — use on, off, or status")),
    }
    t.persist_sync();
    Ok(())
}

pub fn ask_installer_consent() -> bool {
    use dialoguer::Confirm;
    Confirm::new()
        .with_prompt("Share anonymous ax usage data? (no code, paths, or names — see docs/TELEMETRY.md)")
        .default(true)
        .interact()
        .unwrap_or(true)
}
