use crate::commands::resolve_path;

pub fn run() -> Result<(), String> {
    let path = resolve_path(None);
    crate::installer::run_interactive_installer(&path)?;
    Ok(())
}
