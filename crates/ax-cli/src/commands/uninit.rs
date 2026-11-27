use std::path::Path;

use ax_context::directory::get_ax_dir;
use ax_sync::git_hooks::remove_git_sync_hooks;

use crate::commands::resolve_target_dir;

/// Remove ax from a project: the git sync hooks first, then the `.ax/`
/// directory. Lists what `.ax/` holds and asks first; without a terminal the
/// delete needs `--yes`.
pub async fn run(path: Option<String>, yes: bool) -> Result<(), String> {
    let root = resolve_target_dir(path);
    let ax_dir = get_ax_dir(&root);
    if ax_dir.exists() && !yes && !confirm(&describe(&ax_dir))? {
        return Err("cancelled; nothing was removed".into());
    }
    // Hooks first: if this fails, .ax/ is still there and `ax` keeps working.
    remove_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    if ax_dir.exists() {
        std::fs::remove_dir_all(&ax_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What deleting `ax_dir` removes, one entry per line.
fn describe(ax_dir: &Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(ax_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let note = match name.as_str() {
                        "ax.db" => " (index and stored memories)",
                        "policy" => " (project rules and skills)",
                        "memory" => " (shared memories)",
                        "backups" => " (backups)",
                        _ => "",
                    };
                    let slash = if entry.path().is_dir() { "/" } else { "" };
                    format!("  {name}{slash}{note}")
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    format!(
        "ax uninit deletes {} and everything in it:\n{}",
        ax_dir.display(),
        names.join("\n")
    )
}

fn confirm(summary: &str) -> Result<bool, String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(format!(
            "{summary}\nNo terminal to confirm; run `ax uninit --yes` to delete it."
        ));
    }
    eprintln!("{summary}");
    dialoguer::Confirm::new()
        .with_prompt("Delete it? This cannot be undone")
        .default(false)
        .interact()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_names_shared_policy_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("policy")).unwrap();
        std::fs::create_dir_all(dir.path().join("memory")).unwrap();
        std::fs::write(dir.path().join("ax.db"), "").unwrap();
        let text = describe(dir.path());
        assert!(text.contains("policy/ (project rules and skills)"), "{text}");
        assert!(text.contains("memory/ (shared memories)"), "{text}");
        assert!(text.contains("ax.db (index and stored memories)"), "{text}");
    }
}
