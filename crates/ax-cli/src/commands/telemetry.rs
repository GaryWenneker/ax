//! `ax telemetry` — anonymous usage telemetry controls.

use ax_telemetry::{telemetry, TELEMETRY_DOCS};

pub async fn run(action: Option<String>) -> Result<(), String> {
    let act = action.unwrap_or_else(|| "status".to_string());
    if act == "flush" {
        let queue_before = {
            let mut t = telemetry().lock().map_err(|e| e.to_string())?;
            if !t.is_enabled() {
                return Err("telemetry is disabled — run `ax telemetry on` first".into());
            }
            t.queue_path()
        };
        let had_queue = queue_before.exists();
        ax_telemetry::flush_global(ax_telemetry::DEFAULT_FLUSH_TIMEOUT_MS).await;
        let remaining = if queue_before.exists() {
            std::fs::metadata(&queue_before)
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };
        if had_queue && remaining == 0 {
            println!("Telemetry flushed to ingest.");
        } else if had_queue {
            println!(
                "Telemetry flush sent partial batch; {} bytes still queued.",
                remaining
            );
        } else {
            println!("Nothing queued to flush.");
        }
        return Ok(());
    }

    let mut t = telemetry().lock().map_err(|e| e.to_string())?;
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
            let queue = t.queue_path();
            if queue.exists() {
                let bytes = std::fs::metadata(&queue).map(|m| m.len()).unwrap_or(0);
                println!("queued_bytes: {}", bytes);
            }
        }
        other => {
            return Err(format!(
                "unknown action '{other}' — use on, off, status, or flush"
            ))
        }
    }
    t.persist_sync();
    Ok(())
}

/// Ask whether to share usage data. `None` when no one could be asked
/// (stdin or stderr is not a terminal, or the prompt failed): the caller then
/// stores no choice, so the question comes back on the next interactive run.
pub fn ask_installer_consent() -> Option<bool> {
    use dialoguer::Confirm;
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return None;
    }
    consent_answer(
        Confirm::new()
            .with_prompt(
                "Share anonymous ax usage data? (no code, paths, or names — see docs/TELEMETRY.md)",
            )
            .default(true)
            .interact(),
    )
}

fn consent_answer(answer: dialoguer::Result<bool>) -> Option<bool> {
    answer.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_that_cannot_be_shown_records_no_choice() {
        let failed = dialoguer::Result::<bool>::Err(dialoguer::Error::IO(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "not a terminal",
        )));
        assert_eq!(consent_answer(failed), None);
        assert_eq!(consent_answer(Ok(true)), Some(true));
        assert_eq!(consent_answer(Ok(false)), Some(false));
    }
}
