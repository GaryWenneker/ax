use crate::commands::resolve_target_dir;

/// What `ax install` wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum InstallMode {
    /// MCP server, plugins, hooks and instruction files
    Full,
    /// Instruction files only: no MCP, plugins, hooks or daemons
    #[value(alias = "cli_only")]
    CliOnly,
}

pub fn run(
    yes: bool,
    all: bool,
    targets: Vec<String>,
    mode: Option<InstallMode>,
    path: Option<String>,
) -> Result<(), String> {
    let root = resolve_target_dir(path);
    let cli_only = mode == Some(InstallMode::CliOnly);
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
