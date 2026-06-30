//! `ax upgrade` — self-update via GitHub releases or cargo install.

use crate::ui::{info_line, ok_line, warn_line, SpinnerGuard};
use crate::version_check;

pub async fn run(version: Option<String>, check: bool) -> Result<(), String> {
    if check {
        return version_check::run_check(true).await;
    }

    let repo = version_check::github_repo();
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("invalid AX_GITHUB_REPO '{repo}' — expected owner/name"));
    }
    let (owner, name) = (parts[0], parts[1]);

    if try_github_upgrade(owner, name, version.as_deref())? {
        return Ok(());
    }

    if try_cargo_reinstall(&repo) {
        return Ok(());
    }

    println!("{}", warn_line("Could not auto-upgrade. Options:"));
    println!("  cargo install --git https://github.com/{repo} ax-cli");
    if let Some(v) = version {
        println!("  Or publish a GitHub release tag {v} for platform binary 'ax'");
    } else {
        println!("  Or publish a GitHub release with platform binary 'ax'");
    }
    Ok(())
}

fn release_bundle_target() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win32-x64".into(),
        ("windows", "aarch64") => "win32-arm64".into(),
        ("linux", "x86_64") => "linux-x64".into(),
        ("linux", "aarch64") => "linux-arm64".into(),
        ("macos", "x86_64") => "darwin-x64".into(),
        ("macos", "aarch64") => "darwin-arm64".into(),
        (os, arch) => format!("{}-{}", os, arch),
    }
}

fn try_github_upgrade(owner: &str, name: &str, version: Option<&str>) -> Result<bool, String> {
    use self_update::backends::github::Update;
    use self_update::Status;

    let bundle = release_bundle_target();
    let bin_in_archive = if bundle.starts_with("win32") {
        format!("ax-{}/ax.exe", bundle)
    } else {
        format!("ax-{}/ax", bundle)
    };
    let mut builder = Update::configure();
    builder
        .repo_owner(owner)
        .repo_name(name)
        .bin_name("ax")
        .target(&bundle)
        .bin_path_in_archive(&bin_in_archive)
        .show_download_progress(true)
        .current_version(self_update::cargo_crate_version!());
    if let Some(v) = version {
        builder.target_version_tag(v);
    }
    let _spinner = SpinnerGuard::new("Checking for updates...", false);
    let update = builder.build().map_err(|e| e.to_string())?;
    match update.update().map_err(|e| e.to_string())? {
        Status::UpToDate(v) => {
            drop(_spinner);
            println!("{}", ok_line(format!("Already up to date ({v})")));
            Ok(true)
        }
        Status::Updated(v) => {
            drop(_spinner);
            println!("{}", ok_line(format!("Updated to {v}. Open a new terminal if the version looks unchanged.")));
            Ok(true)
        }
    }
}

fn try_cargo_reinstall(repo: &str) -> bool {
    let exe = std::env::current_exe().ok();
    let path = exe
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    if !path.contains(".cargo") {
        return false;
    }
    let git_url = format!("https://github.com/{}", repo);
    println!("{}", info_line("Detected cargo install — running cargo install --force..."));
    let status = std::process::Command::new("cargo")
        .arg("install")
        .arg("--force")
        .arg("--git")
        .arg(&git_url)
        .arg("ax-cli")
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("{}", ok_line("cargo install completed"));
            true
        }
        _ => false,
    }
}
