//! `ax agent economics` and `ax agent optimize --report`.

use std::path::PathBuf;

use crate::commands::resolve_path;

pub async fn run_economics(
    path: Option<String>,
    session: Option<String>,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let db = root.join(".ax").join("ax.db");
    let (currency, usd_per_eur) = display_currency(&root);
    match ax_pi::read_economics(&db, session.as_deref(), currency, usd_per_eur).await {
        Ok((text, value)) => print_report(json, &text, &value),
        Err(err) => {
            eprintln!("AX_PI_INTEGRATION_ERROR {err}");
            if json {
                println!("{}", serde_json::json!({ "degraded": true }));
            }
        }
    }
    Ok(())
}

pub async fn run_optimize(path: Option<String>, json: bool) -> Result<(), String> {
    let db = db_path(path);
    match ax_pi::read_optimization(&db).await {
        Ok((text, value)) => print_report(json, &text, &value),
        Err(err) => {
            eprintln!("AX_PI_INTEGRATION_ERROR {err}");
            if json {
                println!("{}", serde_json::json!({ "degraded": true }));
            }
        }
    }
    Ok(())
}

fn db_path(path: Option<String>) -> PathBuf {
    resolve_path(path).join(".ax").join("ax.db")
}

fn display_currency(root: &std::path::Path) -> (&'static str, Option<f64>) {
    let settings = ax_usage::load_settings(Some(root));
    let label = match settings.currency {
        ax_usage::Currency::Eur => "EUR",
        ax_usage::Currency::Usd => "USD",
    };
    (label, settings.usd_per_eur)
}

fn print_report(json: bool, text: &str, value: &serde_json::Value) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
        );
    } else {
        println!("{text}");
    }
}
