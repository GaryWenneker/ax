use crate::commands::resolve_path;

pub fn run(
    yes: bool,
    all: bool,
    targets: Vec<String>,
    mode: Option<String>,
    path: Option<String>,
) -> Result<(), String> {
    let root = resolve_path(path);
    let cli_only = mode
        .as_deref()
        .map(|m| m.eq_ignore_ascii_case("cli-only") || m.eq_ignore_ascii_case("cli_only"))
        .unwrap_or(false);
    crate::installer::run_installer(
        &root,
        crate::installer::InstallOptions {
            yes,
            install_all: all,
            targets,
            cli_only,
        },
    )
}
