//! `ax offload` — configure explore reasoning offload (BYO OpenAI-compatible endpoint).

use ax_reasoning::{offload_status, write_offload_config, OffloadConfig};

pub fn run(action: Option<String>, url: Option<String>, key_env: Option<String>) -> Result<(), String> {
    let act = action.unwrap_or_else(|| "status".to_string());
    match act.as_str() {
        "status" => {
            println!("{}", serde_json::to_string_pretty(&offload_status()).unwrap_or_default());
        }
        "set-endpoint" => {
            let endpoint = url.ok_or("usage: ax offload set-endpoint <url> [--key-env VAR]")?;
            let mut cfg = OffloadConfig::default();
            cfg.url = Some(endpoint);
            if let Some(env) = key_env {
                cfg.key_env = Some(env);
            }
            write_offload_config(Some(cfg))?;
            println!("Offload endpoint saved in ~/.ax/config.json");
        }
        "clear" => {
            write_offload_config(None)?;
            println!("Offload configuration cleared.");
        }
        other => return Err(format!("unknown action '{other}' — use status, set-endpoint, or clear")),
    }
    Ok(())
}
