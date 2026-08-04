use crate::commands::resolve_path;

pub fn run(yes: bool, all: bool, targets: Vec<String>, path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    crate::installer::run_installer(
        &root,
        crate::installer::InstallOptions {
            yes,
            install_all: all,
            targets,
        },
    )
}
