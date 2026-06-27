//! Remove stale `.ax/ax.lock` — CG: `codegraph unlock`.

use ax_context::directory::{get_ax_dir, is_initialized};

use crate::commands::resolve_path;
use crate::ui::ok_line;

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    if !is_initialized(&root) {
        return Err(format!(
            "project not initialized in {} — run ax init first",
            root.display()
        ));
    }
    let lock_path = get_ax_dir(&root).join("ax.lock");
    if !lock_path.exists() {
        println!("{}", ok_line("No lock file found — nothing to do"));
        return Ok(());
    }
    std::fs::remove_file(&lock_path).map_err(|e| e.to_string())?;
    println!("{}", ok_line("Removed lock file. You can run indexing again."));
    Ok(())
}
