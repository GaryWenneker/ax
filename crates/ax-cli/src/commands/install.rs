use crate::commands::resolve_path;

pub fn run(yes: bool, all: bool) -> Result<(), String> {
    let path = resolve_path(None);
    crate::installer::run_installer(
        &path,
        crate::installer::InstallOptions { yes, install_all: all },
    )
}
