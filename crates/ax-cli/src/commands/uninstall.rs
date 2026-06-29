pub fn run() -> Result<(), String> {
    crate::installer::run_uninstall()?;
    Ok(())
}
