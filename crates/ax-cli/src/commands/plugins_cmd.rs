//! `ax plugins` — list project extractor plugins and trust or untrust them.
//! Project plugins can run any command, so `ax index` only runs the ones
//! trusted here.

use std::path::Path;

use crate::commands::resolve_path;

fn selected<'a>(
    plugins: &'a [ax_plugins::LoadedPlugin],
    names: &[String],
) -> Result<Vec<&'a ax_plugins::LoadedPlugin>, String> {
    if names.is_empty() {
        return Ok(plugins.iter().collect());
    }
    let mut out = Vec::new();
    for name in names {
        let p = plugins
            .iter()
            .find(|p| &p.manifest.name == name)
            .ok_or_else(|| format!("no plugin named '{name}' under .ax/plugins"))?;
        out.push(p);
    }
    Ok(out)
}

fn describe(p: &ax_plugins::LoadedPlugin) -> String {
    match (&p.manifest.command, &p.manifest.wasm) {
        (_, Some(w)) => format!("wasm {w}"),
        (Some(c), None) => {
            let mut s = c.clone();
            for a in &p.manifest.args {
                s.push(' ');
                s.push_str(a);
            }
            s
        }
        (None, None) => "(no command)".into(),
    }
}

pub fn run_list(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let plugins = ax_plugins::discover_plugins(&root);
    if json {
        let rows: Vec<_> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.manifest.name,
                    "dir": p.dir.to_string_lossy(),
                    "extensions": p.manifest.extensions,
                    "runs": describe(p),
                    "trusted": ax_plugins::is_trusted(p),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return Ok(());
    }
    if plugins.is_empty() {
        println!("No plugins under {}", plugins_dir(&root).display());
        return Ok(());
    }
    for p in &plugins {
        let state = if ax_plugins::is_trusted(p) {
            "trusted"
        } else {
            "NOT trusted"
        };
        println!(
            "{}  [{state}]  {}  runs: {}",
            p.manifest.name,
            p.manifest.extensions.join(","),
            describe(p)
        );
    }
    Ok(())
}

fn plugins_dir(root: &Path) -> std::path::PathBuf {
    root.join(".ax").join("plugins")
}

pub fn run_trust(path: Option<String>, names: Vec<String>, untrust: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let plugins = ax_plugins::discover_plugins(&root);
    if plugins.is_empty() {
        return Err(format!("no plugins under {}", plugins_dir(&root).display()));
    }
    for p in selected(&plugins, &names)? {
        if untrust {
            ax_plugins::untrust_plugin(p).map_err(|e| e.to_string())?;
            println!("Untrusted {} ({})", p.manifest.name, p.dir.display());
        } else {
            ax_plugins::trust_plugin(p).map_err(|e| e.to_string())?;
            println!(
                "Trusted {} ({}); it runs: {}",
                p.manifest.name,
                p.dir.display(),
                describe(p)
            );
        }
    }
    Ok(())
}
