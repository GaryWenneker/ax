//! `ax upgrade` — non-interactive self-update from GitHub Releases (getax redirect fallback).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::ui::{info_line, ok_line, SpinnerGuard};
use crate::version_check::{self, github_token, is_update_available, normalize_version, strip_v};

const CDN_BASE: &str = "https://getax.wenneker.io/releases";
const REQUEST_TIMEOUT_SECS: u64 = 120;

/// Complete a staged `ax.new.exe` swap left by an older upgrade, and sync cargo shadow copies.
pub fn apply_pending_upgrade() {
    #[cfg(windows)]
    apply_pending_upgrade_windows();
}

pub async fn run(version: Option<String>, check: bool) -> Result<(), String> {
    if check {
        return version_check::run_check(true).await;
    }

    let current = version_check::current_version();

    let bundle = release_bundle_target();
    let ext = if bundle.starts_with("win32") {
        "zip"
    } else {
        "tar.gz"
    };

    let target_version = match version {
        Some(v) => normalize_version(&v),
        None => {
            let _spin = SpinnerGuard::new("Resolving latest release…", false);
            version_check::resolve_latest_installable_version(
                &version_check::github_repo(),
                &bundle,
                ext,
            )
            .await
            .map_err(|e| format!("Could not resolve latest release: {e}"))?
        }
    };

    if is_update_available(current, &target_version) {
        println!(
            "{}",
            ok_line(format!(
                "Updating {} → {}…",
                strip_v(current),
                strip_v(&target_version)
            ))
        );
    } else if normalize_version(current) == normalize_version(&target_version) {
        println!(
            "{}",
            ok_line(format!(
                "Reinstalling {} (latest available)…",
                strip_v(&target_version)
            ))
        );
    } else {
        println!(
            "{}",
            ok_line(format!(
                "Installing {}…",
                strip_v(&target_version)
            ))
        );
    }
    let archive_name = format!("ax-{bundle}.{ext}");

    let _spin = SpinnerGuard::new(&format!("Downloading {archive_name}…"), false);
    let bytes = download_archive(&target_version, &bundle, ext)?;
    drop(_spin);

    #[cfg(windows)]
    {
        schedule_windows_bundle_upgrade(&bytes, &bundle)?;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        println!(
            "{}",
            ok_line(format!(
                "Updated to {} — install finishes in a few seconds after exit.",
                strip_v(&target_version)
            ))
        );
        println!(
            "{}",
            info_line("Open a new terminal and run `ax version` to verify.")
        );
        std::process::exit(0);
    }

    #[cfg(not(windows))]
    {
        let bin_name = "ax";
        let inner_path = format!("ax-{bundle}/{bin_name}");
        let new_binary = extract_from_targz(&bytes, &inner_path)?;
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        replace_binary_unix(&current_exe, &new_binary)?;
        println!(
            "{}",
            ok_line(format!(
                "Updated to {}. Open a new terminal if `ax version` still shows the old version.",
                strip_v(&target_version)
            ))
        );
        Ok(())
    }
}

fn release_bundle_target() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win32-x64".into(),
        ("windows", "aarch64") => "win32-arm64".into(),
        ("linux", "x86_64") => "linux-x64".into(),
        ("linux", "aarch64") => "linux-arm64".into(),
        ("macos", "x86_64") => "darwin-x64".into(),
        ("macos", "aarch64") => "darwin-arm64".into(),
        (os, arch) => format!("{os}-{arch}"),
    }
}

fn download_archive(version: &str, bundle: &str, ext: &str) -> Result<Vec<u8>, String> {
    let name = format!("ax-{bundle}.{ext}");
    let repo = version_check::github_repo();
    let gh = format!("https://github.com/{repo}/releases/download/{version}/{name}");
    match download_bytes(&gh, github_token().as_deref()) {
        Ok(bytes) => return Ok(bytes),
        Err(e) => {
            eprintln!(
                "{}",
                info_line(format!("GitHub download failed ({e}); trying getax redirect…"))
            );
        }
    }

    let cdn = format!("{CDN_BASE}/{version}/{name}");
    download_bytes(&cdn, None)
}

fn download_bytes(url: &str, token: Option<&str>) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent("ax-upgrade")
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }

    let resp = req
        .send()
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

fn extract_from_targz(bytes: &[u8], inner_path: &str) -> Result<Vec<u8>, String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = Archive::new(gz);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        if path.to_string_lossy() == inner_path {
            let mut out = Vec::new();
            entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err(format!("binary '{inner_path}' not found in archive"))
}

#[cfg(windows)]
fn windows_install_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ax")
}

#[cfg(windows)]
fn ps_single_quoted(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(windows)]
fn extract_zip_bundle(bytes: &[u8], dest: &Path, bundle: &str) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = file.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut outfile = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
    }

    let inner = dest.join(format!("ax-{bundle}"));
    if inner.is_dir() {
        for entry in std::fs::read_dir(&inner).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let target = dest.join(entry.file_name());
            if target.exists() {
                if target.is_dir() {
                    std::fs::remove_dir_all(&target).ok();
                } else {
                    std::fs::remove_file(&target).ok();
                }
            }
            std::fs::rename(entry.path(), &target).map_err(|e| e.to_string())?;
        }
        std::fs::remove_dir(&inner).ok();
    }

    let exe = dest.join("ax.exe");
    if !exe.is_file() {
        return Err("ax.exe not found in release bundle".into());
    }
    let bin = dest.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    std::fs::copy(&exe, bin.join("ax.exe")).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn sync_cargo_shadow(from_bin: &Path) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    if std::env::var("AX_KEEP_CARGO_BIN").ok().as_deref() == Some("1") {
        return;
    }
    let cargo_ax = home.join(".cargo").join("bin").join("ax.exe");
    if cargo_ax.is_file() {
        let _ = std::fs::copy(from_bin, &cargo_ax);
    }
}

#[cfg(windows)]
fn apply_pending_upgrade_windows() {
    let root = windows_install_root();
    let bin_exe = root.join("current").join("bin").join("ax.exe");
    let staged = bin_exe.with_file_name("ax.new.exe");

    if staged.is_file() {
        let Ok(running) = std::env::current_exe() else {
            return;
        };
        // Swap stale ax.new.exe left by older upgrades — only when not executing from that path.
        if bin_exe.is_file() && !same_path(&running, &bin_exe) {
            let _ = std::fs::remove_file(&bin_exe);
            if std::fs::rename(&staged, &bin_exe).is_ok() {
                let _ = std::fs::remove_file(bin_exe.with_file_name("ax.old.exe"));
                sync_cargo_shadow(&bin_exe);
            }
        }
    }

    if bin_exe.is_file() {
        sync_cargo_shadow(&bin_exe);
    }
}

#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(windows)]
fn schedule_windows_bundle_upgrade(bytes: &[u8], bundle: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let root = windows_install_root();
    let staging = root.join(format!("upgrade-staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    extract_zip_bundle(bytes, &staging, bundle)?;

    let dest = root.join("current");
    let bin_dir = dest.join("bin");
    let parent_pid = std::process::id();
    let cargo_ax = dirs::home_dir()
        .map(|h| h.join(".cargo").join("bin").join("ax.exe"))
        .unwrap_or_default();

    let script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         $parent = {parent_pid}\n\
         $deadline = (Get-Date).AddSeconds(90)\n\
         while ((Get-Process -Id $parent -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $deadline)) {{\n\
           Start-Sleep -Milliseconds 200\n\
         }}\n\
         Start-Sleep -Seconds 1\n\
         Get-Process -Name 'ax' -ErrorAction SilentlyContinue | ForEach-Object {{\n\
           Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue\n\
         }}\n\
         Start-Sleep -Seconds 1\n\
         $dest = '{}'\n\
         $staging = '{}'\n\
         $binDir = '{}'\n\
         $cargo = '{}'\n\
         if (Test-Path $dest) {{ Remove-Item -Recurse -Force $dest }}\n\
         Move-Item -Force $staging $dest\n\
         New-Item -ItemType Directory -Force -Path $binDir | Out-Null\n\
         Copy-Item -Force (Join-Path $dest 'ax.exe') (Join-Path $binDir 'ax.exe')\n\
         if ((Test-Path $cargo) -and ($env:AX_KEEP_CARGO_BIN -ne '1')) {{\n\
           Copy-Item -Force (Join-Path $binDir 'ax.exe') $cargo\n\
         }}\n\
         Remove-Item -Force $MyInvocation.MyCommand.Path -ErrorAction SilentlyContinue\n",
        ps_single_quoted(&dest),
        ps_single_quoted(&staging),
        ps_single_quoted(&bin_dir),
        ps_single_quoted(&cargo_ax),
    );

    let script_path = std::env::temp_dir().join(format!("ax-upgrade-{parent_pid}.ps1"));
    std::fs::write(&script_path, script).map_err(|e| format!("write upgrade script: {e}"))?;

    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            script_path
                .to_str()
                .ok_or("upgrade script path not UTF-8")?,
        ])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("spawn upgrade helper: {e}"))?;

    Ok(())
}

#[cfg(not(windows))]
fn replace_binary_unix(current_exe: &Path, new_bytes: &[u8]) -> Result<(), String> {
    let dir = current_exe
        .parent()
        .ok_or("cannot determine binary directory")?;
    let tmp = dir.join("ax.tmp");
    std::fs::write(&tmp, new_bytes).map_err(|e| format!("write tmp: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, current_exe).map_err(|e| format!("replace binary: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_names() {
        assert!(release_bundle_target().starts_with("win32-")
            || release_bundle_target().starts_with("linux-")
            || release_bundle_target().starts_with("darwin-"));
    }
}
