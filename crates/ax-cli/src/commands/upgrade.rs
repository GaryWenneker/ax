//! `ax upgrade` — non-interactive self-update from getax CDN (GitHub fallback).

use std::io::Read;
use std::path::Path;

use crate::ui::{info_line, ok_line, SpinnerGuard};
use crate::version_check::{self, github_token, is_update_available, normalize_version, strip_v};

const CDN_BASE: &str = "https://getax.wenneker.io/releases";
const REQUEST_TIMEOUT_SECS: u64 = 120;

pub async fn run(version: Option<String>, check: bool) -> Result<(), String> {
    if check {
        return version_check::run_check(true).await;
    }

    let current = version_check::current_version();

    let target_version = match version {
        Some(v) => normalize_version(&v),
        None => {
            let _spin = SpinnerGuard::new("Checking for updates…", false);
            version_check::resolve_latest_version(&version_check::github_repo())
                .await
                .map_err(|e| format!("Could not check for updates: {e}"))?
        }
    };

    if !is_update_available(current, &target_version) {
        println!("{}", ok_line(format!("Already up to date ({})", strip_v(current))));
        return Ok(());
    }

    println!(
        "{}",
        ok_line(format!(
            "Updating {} → {}…",
            strip_v(current),
            strip_v(&target_version)
        ))
    );

    let bundle = release_bundle_target();
    let ext = if bundle.starts_with("win32") {
        "zip"
    } else {
        "tar.gz"
    };
    let archive_name = format!("ax-{bundle}.{ext}");

    let _spin = SpinnerGuard::new(&format!("Downloading {archive_name}…"), false);
    let bytes = download_archive(&target_version, &bundle, ext)?;
    drop(_spin);

    let bin_name = if bundle.starts_with("win32") {
        "ax.exe"
    } else {
        "ax"
    };
    let inner_path = format!("ax-{bundle}/{bin_name}");
    let new_binary = if ext == "zip" {
        extract_from_zip(&bytes, &inner_path)?
    } else {
        extract_from_targz(&bytes, &inner_path)?
    };

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    replace_binary(&current_exe, &new_binary)?;

    println!(
        "{}",
        ok_line(format!(
            "Updated to {}. Open a new terminal if `ax version` still shows the old version.",
            strip_v(&target_version)
        ))
    );
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
        (os, arch) => format!("{os}-{arch}"),
    }
}

fn download_archive(version: &str, bundle: &str, ext: &str) -> Result<Vec<u8>, String> {
    let name = format!("ax-{bundle}.{ext}");
    let cdn = format!("{CDN_BASE}/{version}/{name}");
    match download_bytes(&cdn, None) {
        Ok(bytes) => return Ok(bytes),
        Err(e) => {
            eprintln!(
                "{}",
                info_line(format!("CDN unavailable ({e}); trying GitHub…"))
            );
        }
    }

    let repo = version_check::github_repo();
    let gh = format!("https://github.com/{repo}/releases/download/{version}/{name}");
    download_bytes(&gh, github_token().as_deref())
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

fn extract_from_zip(bytes: &[u8], inner_path: &str) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let mut file = archive
        .by_name(inner_path)
        .map_err(|_| format!("binary '{inner_path}' not found in archive"))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
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

fn replace_binary(current_exe: &Path, new_bytes: &[u8]) -> Result<(), String> {
    let dir = current_exe
        .parent()
        .ok_or("cannot determine binary directory")?;

    #[cfg(windows)]
    {
        return replace_binary_windows(dir, current_exe, new_bytes);
    }

    #[cfg(not(windows))]
    {
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
}

#[cfg(windows)]
fn replace_binary_windows(dir: &Path, current_exe: &Path, new_bytes: &[u8]) -> Result<(), String> {
    let staged = dir.join("ax.new.exe");
    std::fs::write(&staged, new_bytes).map_err(|e| format!("write staged binary: {e}"))?;

    let upgrading_self = std::env::current_exe()
        .ok()
        .map(|running| same_file(&running, current_exe))
        .unwrap_or(true);

    if !upgrading_self {
        let backup = dir.join("ax.old.exe");
        let _ = std::fs::remove_file(&backup);
        if std::fs::rename(current_exe, &backup).is_ok() {
            if std::fs::rename(&staged, current_exe).is_ok() {
                let _ = std::fs::remove_file(&backup);
                return Ok(());
            }
            let _ = std::fs::rename(&backup, current_exe);
        }
    }

    // Running exe cannot be overwritten in-place — defer via a short-lived helper script.
    let script = std::env::temp_dir().join(format!("ax-upgrade-{}.cmd", std::process::id()));
    let current = current_exe.to_string_lossy().replace('%', "%%");
    let staged_s = staged.to_string_lossy().replace('%', "%%");
    let body = format!(
        "@echo off\r\n\
         :wait\r\n\
         del /f /q \"{current}\" >nul 2>&1\r\n\
         if exist \"{current}\" (timeout /t 1 /nobreak >nul & goto wait)\r\n\
         move /y \"{staged_s}\" \"{current}\"\r\n\
         del /f /q \"%~f0\"\r\n"
    );
    std::fs::write(&script, body).map_err(|e| format!("write upgrade script: {e}"))?;

    std::process::Command::new("cmd")
        .args(["/C", "start", "", "/MIN", script.to_str().ok_or("script path")?])
        .spawn()
        .map_err(|e| format!("spawn upgrade helper: {e}"))?;

    eprintln!(
        "{}",
        info_line("Finishing upgrade in the background — close this terminal and open a new one.")
    );
    std::process::exit(0);
}

#[cfg(windows)]
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
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
