//! Installer for AI agent targets.

pub mod targets;


pub fn run_interactive_installer(project_root: &std::path::Path) -> Result<(), String> {
    if let Ok(mut t) = ax_telemetry::telemetry().lock() {
        if !t.has_stored_choice() {
            let on = crate::commands::telemetry::ask_installer_consent();
            t.set_enabled(on, "installer");
            t.persist_sync();
        }
    }
    targets::install_all_detected(project_root)?;
    println!("{} ax installed for detected agents", crate::glyphs::ok());
    Ok(())
}

pub fn run_uninstall() -> Result<(), String> {
    targets::uninstall_all()?;
    println!("{} ax removed from agent configs", crate::glyphs::ok());
    Ok(())
}
