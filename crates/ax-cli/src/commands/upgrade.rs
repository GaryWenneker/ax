//! `ax upgrade` — self-update from the getax.wenneker.io CDN.

use std::io::Read;
use std::path::PathBuf;

use crate::ui::{ok_line, warn_line, SpinnerGuard};
use crate::version_check::{self, is_update_available, normalize_version, strip_v};

const CDN_BASE: &str = "https://getax.wenneker.io/releases";
const REQUEST_TIMEOUT_SECS: u64 = 120;

pub async fn run(version: Option<String>, check: bool) -> Result<(), String> {
    if check {
        return version_check::run_check(true).await;
    }

    let current = version_check::current_version();

    // Resolve target version (explicit tag or latest from CDN).
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

    println!("{}", ok_line(format!("Updating {} → {}…", strip_v(current), strip_v(&target_version))));

    let target_str = release_bundle_target();
    let ext = if target_str.starts_with("win32") { "zip" } else { "tar.gz" };
    let archive_name = format!("ax-{}.{}", target_str, ext);
    let url = format!("{}/{}/{}", CDN_BASE, target_version, archive_name);

    let _spin = SpinnerGuard::new(&format!("Downloading {}…", archive_name), false);
    let bytes = download_bytes(&url)?;
    drop(_spin);

    let bin_name = if target_str.starts_with("win32") { "ax.exe" } else { "ax" };
    let inner_path = format!("ax-{}/{}", target_str, bin_name);

    let new_binary = if ext == "zip" {
        extract_from_zip(&bytes, &inner_path)?
    } else {
        extract_from_targz(&bytes, &inner_path)?
    };

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    replace_binary(&current_exe, &new_binary)?;

    println!("{}", ok_line(format!(
        "Updated to {}. Open a new terminal if `ax version` still shows the old version.",
        strip_v(&target_version)
    )));
    Ok(())
}

// ---------------------------------------------------------------------------
// Platform target
// ---------------------------------------------------------------------------

fn release_bundle_target() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64")  => "win32-x64".into(),
        ("windows", "aarch64") => "win32-arm64".into(),
        ("linux",   "x86_64")  => "linux-x64".into(),
        ("linux",   "aarch64") => "linux-arm64".into(),
        ("macos",   "x86_64")  => "darwin-x64".into(),
        ("macos",   "aarch64") => "darwin-arm64".into(),
        (os, arch) => format!("{}-{}", os, arch),
    }
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent("ax-upgrade")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("CDN returned HTTP {} for {}", resp.status(), url));
    }
    resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Archive extraction
// ---------------------------------------------------------------------------

fn extract_from_zip(bytes: &[u8], inner_path: &str) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let mut file = archive.by_name(inner_path)
        .map_err(|_| format!("binary '{}' not found in archive", inner_path))?;
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
    Err(format!("binary '{}' not found in archive", inner_path))
}

// ---------------------------------------------------------------------------
// Binary replacement (atomic on all platforms)
// ---------------------------------------------------------------------------

fn replace_binary(current_exe: &PathBuf, new_bytes: &[u8]) -> Result<(), String> {
    let dir = current_exe.parent().ok_or("cannot determine binary directory")?;
    let tmp = dir.join("ax.tmp");
    std::fs::write(&tmp, new_bytes).map_err(|e| format!("write tmp: {e}"))?;

    // Make executable on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
    }

    // On Windows we cannot overwrite a running binary directly — rename it aside first.
    #[cfg(windows)]
    {
        let backup = dir.join("ax.old");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(current_exe, &backup).map_err(|e| format!("rename current: {e}"))?;
    }

    std::fs::rename(&tmp, current_exe).map_err(|e| format!("rename new binary: {e}"))?;
    Ok(())
}
