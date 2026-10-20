//! `ax upgrade` — non-interactive self-update from GitHub Releases (getax redirect fallback).

#[cfg(not(windows))]
use std::io::Read;
#[cfg(windows)]
use std::io::Write;
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

pub async fn run(
    version: Option<String>,
    check: bool,
    local: Option<String>,
) -> Result<(), String> {
    if check {
        if local.is_some() {
            return Err("--check is not supported with --local".into());
        }
        return version_check::run_check(true).await;
    }

    let current = version_check::current_version();

    let bundle = release_bundle_target();
    let ext = if bundle.starts_with("win32") {
        "zip"
    } else {
        "tar.gz"
    };

    let local_install = local.is_some();
    let explicit_version = version.is_some() && !local_install;

    let target_version = if local_install {
        version
            .filter(|v| !v.is_empty())
            .map(|v| normalize_version(&v))
            .unwrap_or_else(|| "local-build".into())
    } else {
        match version {
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
        }
    };

    if !explicit_version && !local_install {
        if let Some(cmp) = version_check::compare_versions(&target_version, current) {
            if cmp < 0 {
                return Err(format!(
                    "Refusing to downgrade from {} to {} (getax latest.txt may be stale). \
                     Use `ax upgrade {}` to pin a version, or install from GitHub Releases.",
                    strip_v(current),
                    strip_v(&target_version),
                    target_version
                ));
            }
        }
    }

    if local_install {
        println!(
            "{}",
            ok_line(format!(
                "Installing local build{}…",
                if target_version == "local-build" {
                    String::new()
                } else {
                    format!(" ({})", strip_v(&target_version))
                }
            ))
        );
    } else if is_update_available(current, &target_version) {
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
            ok_line(format!("Installing {}…", strip_v(&target_version)))
        );
    }
    let archive_name = format!("ax-{bundle}.{ext}");

    let bytes = if local_install {
        let path = resolve_local_archive_path(local.as_deref(), &bundle, ext)?;
        let _spin = SpinnerGuard::new(format!("Loading {}…", path.display()), false);
        std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?
    } else {
        let _spin = SpinnerGuard::new(format!("Downloading {archive_name}…"), false);
        let bytes = download_archive(&target_version, &bundle, ext)?;
        drop(_spin);
        bytes
    };

    #[cfg(windows)]
    {
        schedule_windows_bundle_upgrade(&bytes, &bundle)?;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        println!(
            "{}",
            ok_line(format!(
                "Staged {} — the install finishes after this process exits.",
                strip_v(&target_version)
            ))
        );
        println!(
            "{}",
            info_line(
                "Open a new terminal and run `ax version`. If the install failed, the next ax command prints why."
            )
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
        let status = std::process::Command::new(&current_exe)
            .args(["init", "--all"])
            .status()
            .map_err(|e| format!("start project discovery: {e}"))?;
        if !status.success() {
            eprintln!("ax: project discovery failed. Re-run `ax init --all`.");
        }
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

fn resolve_local_archive_path(
    local: Option<&str>,
    bundle: &str,
    ext: &str,
) -> Result<PathBuf, String> {
    if let Some(path) = local.filter(|s| !s.is_empty()) {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("local archive not found: {}", p.display()));
    }

    if let Ok(env_path) = std::env::var("AX_UPGRADE_ARCHIVE") {
        let p = PathBuf::from(&env_path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("AX_UPGRADE_ARCHIVE not found: {}", p.display()));
    }

    let name = format!("ax-{bundle}.{ext}");
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("dist").join(&name));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            candidates.push(root.join("dist").join(&name));
        }
    }

    for path in candidates {
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(format!(
        "local archive not found — run .\\scripts\\ship-local.ps1 first, or pass --local dist/{name}"
    ))
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
                info_line(format!(
                    "GitHub download failed ({e}); trying getax redirect…"
                ))
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

    let resp = req.send().map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }
    resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
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
/// Detached child: wait for upgrader exit, swap staged bundle into `%LOCALAPPDATA%\\ax\\current`.
pub fn run_upgrade_apply(parent_pid: u32, staging: PathBuf, dest: PathBuf) -> Result<(), String> {
    wait_for_process_exit(parent_pid, std::time::Duration::from_secs(90));
    std::thread::sleep(std::time::Duration::from_secs(1));
    let _ = stop_other_ax_processes(std::process::id());
    std::thread::sleep(std::time::Duration::from_secs(1));

    let install_root = dest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dest.clone());
    let finished = activate_staged_install(&staging, &dest).and_then(|_| {
        let init_exe = publish_bin_exe(&dest)?;
        sync_cargo_shadow(&dest.join("ax.exe"));
        remove_leftover_staging(&install_root);
        Ok(init_exe)
    });
    let status = finished.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Err(error) = write_upgrade_status(&install_root, &status) {
        tracing::debug!("{error}");
    }
    clear_promotion(&install_root);
    let init_exe = finished?;
    match std::process::Command::new(&init_exe)
        .args(["init", "--all"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(_) => tracing::debug!("project discovery failed; re-run ax init --all"),
        Err(err) => tracing::debug!("project discovery did not start: {err}"),
    }
    Ok(())
}

#[cfg(windows)]
fn windows_no_window() -> u32 {
    0x0800_0000 // CREATE_NO_WINDOW
}

#[cfg(windows)]
fn process_running(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(windows_no_window())
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline && process_running(pid) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(windows)]
fn stop_other_ax_processes(self_pid: u32) -> Result<usize, String> {
    use std::os::windows::process::CommandExt;
    let mut killed = 0usize;
    let procs = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            "name='ax.exe'",
            "get",
            "ProcessId",
            "/format:csv",
        ])
        .creation_flags(windows_no_window())
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&procs.stdout);
    for line in text.lines() {
        let pid: u32 = match line.split(',').nth(1).and_then(|s| s.trim().parse().ok()) {
            Some(p) if p != self_pid => p,
            _ => continue,
        };
        if std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(windows_no_window())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            killed += 1;
        }
    }
    Ok(killed)
}

#[cfg(not(windows))]
pub fn run_upgrade_apply(
    _parent_pid: u32,
    _staging: PathBuf,
    _dest: PathBuf,
) -> Result<(), String> {
    Err("upgrade-apply is only used on Windows".into())
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
    let Some(home) = ax_utils::paths::home_dir() else {
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
    swap_pending_bin_exe();
    // `upgrade` replaces the bundle itself. `upgrade-apply` is the helper that
    // performs the swap and must not promote a staging dir before its parent exits.
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("upgrade") | Some("upgrade-apply")
    ) {
        return;
    }
    let root = windows_install_root();
    if let Some(message) = take_upgrade_status(&root) {
        eprintln!(
            "ax: Windows install did not finish: {message}\n\
             Re-run: irm https://getax.wenneker.io/install.ps1 | iex"
        );
    }
    recover_leftover_staging(&root);
}

#[cfg(windows)]
fn swap_pending_bin_exe() {
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
fn remove_leftover_staging(install_root: &Path) {
    let Ok(entries) = std::fs::read_dir(install_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(STAGING_DIR_PREFIX) {
            // A directory still locked by a running ax.exe is removed on a later launch.
            if let Err(error) = std::fs::remove_dir_all(entry.path()) {
                tracing::debug!("leave leftover staging: {error}");
            }
        }
    }
}

#[cfg(windows)]
fn recover_leftover_staging(root: &Path) {
    let dest = root.join("current");
    let current_bin_exists = dest.join("bin").join("ax.exe").is_file();
    let leftover = sole_leftover_staging(root);
    let running_inside = match (&leftover, std::env::current_exe()) {
        (Ok(Some(staging)), Ok(exe)) => exe.starts_with(staging),
        _ => false,
    };
    let action = staging_recovery(
        current_bin_exists,
        running_inside,
        leftover
            .as_ref()
            .map(|path| path.as_ref().map(|_| ()))
            .map_err(Clone::clone),
    );
    match action {
        StagingRecovery::Idle => {}
        StagingRecovery::Ambiguous(message) => eprintln!("ax: {message}"),
        StagingRecovery::Promote => {
            let Some(staging) = leftover.ok().flatten() else {
                return;
            };
            if !claim_promotion(root) {
                return;
            }
            let result = activate_staged_install(&staging, &dest).and_then(|_| {
                publish_bin_exe(&dest)?;
                sync_cargo_shadow(&dest.join("ax.exe"));
                remove_leftover_staging(root);
                Ok(())
            });
            if let Err(error) = write_upgrade_status(root, &result) {
                tracing::debug!("write upgrade status: {error}");
            }
            clear_promotion(root);
            match result {
                Ok(()) => eprintln!(
                    "ax: finished a pending Windows install in {}",
                    dest.display()
                ),
                Err(error) => {
                    eprintln!("ax: could not finish the pending Windows install: {error}")
                }
            }
        }
        StagingRecovery::WaitForExit => {
            let Some(staging) = leftover.ok().flatten() else {
                return;
            };
            if !claim_promotion(root) {
                return;
            }
            if let Err(error) = schedule_external_helper(&staging, &dest, std::process::id()) {
                clear_promotion(root);
                if let Err(status_error) = write_upgrade_status(root, &Err(error.clone())) {
                    tracing::debug!("write upgrade status: {status_error}");
                }
                eprintln!("ax: could not finish the pending Windows install: {error}");
            } else {
                eprintln!(
                    "ax: this process is running from an unfinished install. It moves into {} after exit.",
                    dest.display()
                );
            }
        }
    }
}

#[cfg(windows)]
fn schedule_external_helper(staging: &Path, dest: &Path, parent_pid: u32) -> Result<(), String> {
    let launch_dir = std::env::temp_dir().join(format!("ax-upgrade-helper-{parent_pid}"));
    let helper = external_helper(&bundle_exe(staging), staging, &launch_dir)?;
    spawn_upgrade_apply_helper(&helper, parent_pid, staging, dest)
}

#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(windows)]
fn resolve_comspec() -> PathBuf {
    std::env::var_os("ComSpec")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"))
}

#[cfg(windows)]
fn spawn_upgrade_apply_helper(
    helper: &Path,
    parent_pid: u32,
    staging: &Path,
    dest: &Path,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let flags = CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let args = [
        "upgrade-apply",
        "--parent-pid",
        &parent_pid.to_string(),
        "--staging",
        &staging.to_string_lossy(),
        "--dest",
        &dest.to_string_lossy(),
    ];

    if helper.is_file() {
        match std::process::Command::new(helper)
            .args(args)
            .creation_flags(flags)
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::debug!("spawn {} failed: {e}", helper.display());
            }
        }
    }

    spawn_upgrade_cmd_batch(parent_pid, staging, dest)
}

#[cfg(windows)]
fn spawn_upgrade_cmd_batch(parent_pid: u32, staging: &Path, dest: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let bin_dir = dest.join("bin");
    let cargo_ax = ax_utils::paths::home_dir()
        .map(|h| h.join(".cargo").join("bin").join("ax.exe"))
        .unwrap_or_default();
    let batch_path = std::env::temp_dir().join(format!("ax-upgrade-{parent_pid}.cmd"));
    let status_file = dest.parent().unwrap_or(dest).join("upgrade-status.txt");
    let batch = upgrade_cmd_batch(parent_pid, staging, dest, &bin_dir, &cargo_ax, &status_file);
    std::fs::write(&batch_path, batch).map_err(|e| format!("write upgrade batch: {e}"))?;

    let comspec = resolve_comspec();
    std::process::Command::new(&comspec)
        .arg("/C")
        .arg(&batch_path)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| {
            format!(
                "spawn upgrade helper via {}: {e}\n\
                 Reinstall once: irm https://getax.wenneker.io/install.ps1 | iex",
                comspec.display()
            )
        })?;
    Ok(())
}

#[cfg(windows)]
fn schedule_windows_bundle_upgrade(bytes: &[u8], bundle: &str) -> Result<(), String> {
    let root = windows_install_root();
    let staging = root.join(format!("{STAGING_DIR_PREFIX}{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    extract_zip_bundle(bytes, &staging, bundle)?;

    let dest = root.join("current");
    let parent_pid = std::process::id();
    if !claim_promotion(&root) {
        // Another helper still owns the install directory. Drop this extract
        // so a second `upgrade-staging-*` is not left behind.
        if let Err(error) = std::fs::remove_dir_all(&staging) {
            tracing::debug!("leave staging after lost claim: {error}");
        }
        return Err("another Windows upgrade is still finishing".into());
    }
    schedule_external_helper(&staging, &dest, parent_pid).map_err(|e| {
        clear_promotion(&root);
        if let Err(error) = std::fs::remove_dir_all(&staging) {
            tracing::debug!("leave staging after failed schedule: {error}");
        }
        format!(
            "{e}\n\
             Or run: $env:AX_VERSION='v{}'; irm https://getax.wenneker.io/install.ps1 | iex",
            env!("CARGO_PKG_VERSION")
        )
    })
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
        let mut perms = std::fs::metadata(&tmp)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, current_exe).map_err(|e| format!("replace binary: {e}"))?;
    Ok(())
}

#[cfg(any(windows, test))]
const STAGING_DIR_PREFIX: &str = "upgrade-staging-";
#[cfg(any(windows, test))]
const PROMOTION_TTL: std::time::Duration = std::time::Duration::from_secs(180);

#[cfg(any(windows, test))]
fn previous_install_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    match dest.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

#[cfg(any(windows, test))]
fn rename_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|e| e.to_string())
}

#[cfg(any(windows, test))]
/// Move `staging` onto `dest`. An existing `dest` is parked as `dest.old` and
/// restored if the incoming rename fails.
fn activate_staged_install(staging: &Path, dest: &Path) -> Result<(), String> {
    activate_staged_install_with(staging, dest, rename_dir)
}

#[cfg(any(windows, test))]
fn activate_staged_install_with(
    staging: &Path,
    dest: &Path,
    rename: impl Fn(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    if !staging.is_dir() {
        return Err(format!("staging directory missing: {}", staging.display()));
    }
    let previous = previous_install_path(dest);
    let mut parked = false;
    if dest.exists() {
        if previous.exists() {
            std::fs::remove_dir_all(&previous)
                .map_err(|e| format!("remove leftover {}: {e}", previous.display()))?;
        }
        rename(dest, &previous).map_err(|e| format!("park current install: {e}"))?;
        parked = true;
    }
    if let Err(error) = rename(staging, dest) {
        if parked {
            if let Err(restore) = rename(&previous, dest) {
                return Err(format!(
                    "activate new install: {error}; restore previous install failed: {restore}"
                ));
            }
        }
        return Err(format!("activate new install: {error}"));
    }
    if previous.exists() {
        // The new tree is already at `dest`. Keeping `dest.old` is harmless.
        if let Err(error) = std::fs::remove_dir_all(&previous) {
            tracing::debug!("leave parked install: {error}");
        }
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn bundle_exe(dir: &Path) -> PathBuf {
    let bin = dir.join("bin").join("ax.exe");
    if bin.is_file() {
        bin
    } else {
        dir.join("ax.exe")
    }
}

#[cfg(any(windows, test))]
fn sole_leftover_staging(install_root: &Path) -> Result<Option<PathBuf>, String> {
    let entries = match std::fs::read_dir(install_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {}: {error}", install_root.display())),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(STAGING_DIR_PREFIX) {
            continue;
        }
        let path = entry.path();
        if bundle_exe(&path).is_file() {
            found.push(path);
        }
    }
    found.sort();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        count => Err(format!(
            "{count} leftover upgrade-staging directories under {}; not promoting",
            install_root.display()
        )),
    }
}

#[cfg(any(windows, test))]
fn copy_helper_exe(source: &Path, launch_dir: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err(format!("upgrade helper missing: {}", source.display()));
    }
    let name = source
        .file_name()
        .ok_or_else(|| format!("upgrade helper has no file name: {}", source.display()))?;
    std::fs::create_dir_all(launch_dir).map_err(|e| format!("create helper dir: {e}"))?;
    let dest = launch_dir.join(name);
    std::fs::copy(source, &dest).map_err(|e| format!("copy upgrade helper: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

#[cfg(any(windows, test))]
fn external_helper(source: &Path, staging: &Path, launch_dir: &Path) -> Result<PathBuf, String> {
    let helper = copy_helper_exe(source, launch_dir)?;
    if helper.starts_with(staging) {
        return Err("upgrade helper must not run from inside the staging directory".into());
    }
    Ok(helper)
}

#[cfg(any(windows, test))]
fn upgrade_status_path(install_root: &Path) -> PathBuf {
    install_root.join("upgrade-status.txt")
}

#[cfg(any(windows, test))]
fn write_upgrade_status(install_root: &Path, result: &Result<(), String>) -> Result<(), String> {
    let body = match result {
        Ok(()) => "ok\n".to_string(),
        Err(error) => format!("error\n{error}\n"),
    };
    let path = upgrade_status_path(install_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, body).map_err(|e| format!("write upgrade status: {e}"))
}

#[cfg(any(windows, test))]
fn upgrade_status_message(text: &str) -> Option<String> {
    let mut lines = text.lines();
    match lines.next().map(str::trim) {
        Some("error") => {
            let detail = lines.collect::<Vec<_>>().join("\n");
            let detail = detail.trim();
            if detail.is_empty() {
                Some("Windows install did not finish".into())
            } else {
                Some(detail.to_string())
            }
        }
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn take_upgrade_status(install_root: &Path) -> Option<String> {
    let path = upgrade_status_path(install_root);
    let text = std::fs::read_to_string(&path).ok()?;
    // A failed delete repeats the message on the next launch.
    if let Err(error) = std::fs::remove_file(&path) {
        tracing::debug!("remove upgrade status: {error}");
    }
    upgrade_status_message(&text)
}

#[derive(Debug, PartialEq, Eq)]
#[cfg(any(windows, test))]
enum StagingRecovery {
    Idle,
    Promote,
    WaitForExit,
    Ambiguous(String),
}

#[cfg(any(windows, test))]
fn staging_recovery(
    current_bin_exists: bool,
    running_inside_staging: bool,
    leftover: Result<Option<()>, String>,
) -> StagingRecovery {
    if current_bin_exists {
        return StagingRecovery::Idle;
    }
    match leftover {
        Ok(None) => StagingRecovery::Idle,
        Err(error) => StagingRecovery::Ambiguous(error),
        Ok(Some(())) if running_inside_staging => StagingRecovery::WaitForExit,
        Ok(Some(())) => StagingRecovery::Promote,
    }
}

#[cfg(any(windows, test))]
fn upgrade_cmd_batch(
    parent_pid: u32,
    staging: &Path,
    dest: &Path,
    bin_dir: &Path,
    cargo_ax: &Path,
    status_file: &Path,
) -> String {
    let previous = previous_install_path(dest);
    format!(
        "@echo off\r\n\
         setlocal EnableExtensions\r\n\
         set \"PARENT={parent_pid}\"\r\n\
         set \"STAGING={staging}\"\r\n\
         set \"DEST={dest}\"\r\n\
         set \"PREVIOUS={previous}\"\r\n\
         set \"BINDIR={bin_dir}\"\r\n\
         set \"CARGOAX={cargo_ax}\"\r\n\
         set \"STATUS={status}\"\r\n\
         :wait_parent\r\n\
         tasklist /FI \"PID eq %PARENT%\" 2>nul | find \" %PARENT% \" >nul && (\r\n\
           timeout /t 1 /nobreak >nul\r\n\
           goto wait_parent\r\n\
         )\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         taskkill /IM ax.exe /F >nul 2>&1\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         if exist \"%DEST%\" (\r\n\
           if exist \"%PREVIOUS%\" rmdir /s /q \"%PREVIOUS%\"\r\n\
           move /Y \"%DEST%\" \"%PREVIOUS%\" >nul\r\n\
           if errorlevel 1 (\r\n\
             echo error> \"%STATUS%\"\r\n\
             echo park current install failed>> \"%STATUS%\"\r\n\
             goto done\r\n\
           )\r\n\
         )\r\n\
         move /Y \"%STAGING%\" \"%DEST%\" >nul\r\n\
         if errorlevel 1 (\r\n\
           if exist \"%PREVIOUS%\" move /Y \"%PREVIOUS%\" \"%DEST%\" >nul\r\n\
           echo error> \"%STATUS%\"\r\n\
           echo activate new install failed>> \"%STATUS%\"\r\n\
           goto done\r\n\
         )\r\n\
         if exist \"%PREVIOUS%\" rmdir /s /q \"%PREVIOUS%\"\r\n\
         if not exist \"%BINDIR%\" mkdir \"%BINDIR%\" >nul\r\n\
         copy /Y \"%DEST%\\ax.exe\" \"%BINDIR%\\ax.new.exe\" >nul\r\n\
         move /Y \"%BINDIR%\\ax.new.exe\" \"%BINDIR%\\ax.exe\" >nul 2>&1\r\n\
         if exist \"%CARGOAX%\" if not \"%AX_KEEP_CARGO_BIN%\"==\"1\" copy /Y \"%DEST%\\ax.exe\" \"%CARGOAX%\" >nul\r\n\
         if exist \"%BINDIR%\\ax.exe\" (\"%BINDIR%\\ax.exe\" init --all) else (\"%DEST%\\ax.exe\" init --all)\r\n\
         echo ok> \"%STATUS%\"\r\n\
         :done\r\n\
         del \"%~f0\" >nul 2>&1\r\n",
        staging = staging.display(),
        dest = dest.display(),
        previous = previous.display(),
        bin_dir = bin_dir.display(),
        cargo_ax = cargo_ax.display(),
        status = status_file.display(),
    )
}

#[cfg(any(windows, test))]
fn publish_bin_exe(dest: &Path) -> Result<PathBuf, String> {
    let root_exe = dest.join("ax.exe");
    if !root_exe.is_file() {
        return Err(format!("ax.exe not found in {}", dest.display()));
    }
    let bin_dir = dest.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let bin_exe = bin_dir.join("ax.exe");
    let bin_new = bin_dir.join("ax.new.exe");
    let _ = std::fs::remove_file(&bin_new);
    std::fs::copy(&root_exe, &bin_new).map_err(|e| format!("stage ax.exe: {e}"))?;
    if std::fs::rename(&bin_new, &bin_exe).is_err() {
        tracing::debug!("bin/ax.exe locked; leaving ax.new.exe for next startup swap");
        if bin_exe.is_file() {
            return Ok(bin_exe);
        }
        return Ok(bin_new);
    }
    Ok(bin_exe)
}

#[cfg(any(windows, test))]
fn promotion_flag(install_root: &Path) -> PathBuf {
    install_root.join("upgrade-promoting")
}

#[cfg(any(windows, test))]
fn claim_promotion(install_root: &Path) -> bool {
    let flag = promotion_flag(install_root);
    if flag.is_file() {
        let fresh = std::fs::metadata(&flag)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .map(|age| age < PROMOTION_TTL)
            .unwrap_or(true);
        if fresh {
            return false;
        }
        let _ = std::fs::remove_file(&flag);
    }
    if let Some(parent) = flag.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&flag)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"1"))
        .is_ok()
}

#[cfg(any(windows, test))]
fn clear_promotion(install_root: &Path) {
    let _ = std::fs::remove_file(promotion_flag(install_root));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(dir: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn bundle_names() {
        assert!(
            release_bundle_target().starts_with("win32-")
                || release_bundle_target().starts_with("linux-")
                || release_bundle_target().starts_with("darwin-")
        );
    }

    #[test]
    fn activate_replaces_current_and_drops_the_parked_copy() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("current");
        let staging = root.path().join("upgrade-staging-4");
        marker(&dest, "marker", "old");
        marker(&staging, "marker", "new");
        activate_staged_install(&staging, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("marker")).unwrap(), "new");
        assert!(!staging.exists());
        assert!(!previous_install_path(&dest).exists());
    }

    #[test]
    fn activate_places_staging_when_current_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("current");
        let staging = root.path().join("upgrade-staging-9");
        marker(&staging, "marker", "new");
        activate_staged_install(&staging, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("marker")).unwrap(), "new");
        assert!(!staging.exists());
    }

    #[test]
    fn activate_restores_current_when_the_staging_rename_fails() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("current");
        let staging = root.path().join("upgrade-staging-5");
        marker(&dest, "marker", "old");
        marker(&staging, "marker", "new");
        let err = activate_staged_install_with(&staging, &dest, |from, to| {
            if from == staging && to == dest {
                return Err("sharing violation".into());
            }
            std::fs::rename(from, to).map_err(|e| e.to_string())
        })
        .unwrap_err();
        assert!(err.contains("sharing violation"), "{err}");
        assert_eq!(std::fs::read_to_string(dest.join("marker")).unwrap(), "old");
        assert_eq!(
            std::fs::read_to_string(staging.join("marker")).unwrap(),
            "new"
        );
        assert!(!previous_install_path(&dest).exists());
    }

    #[test]
    fn one_staging_bundle_is_the_only_recovery_candidate() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("upgrade-staging-empty")).unwrap();
        let staging = root.path().join("upgrade-staging-7");
        marker(&staging.join("bin"), "ax.exe", "exe");
        std::fs::write(root.path().join("upgrade-staging-file"), "nope").unwrap();
        assert_eq!(sole_leftover_staging(root.path()).unwrap(), Some(staging));
    }

    #[test]
    fn two_staging_bundles_are_not_promoted() {
        let root = tempfile::tempdir().unwrap();
        marker(&root.path().join("upgrade-staging-1"), "ax.exe", "a");
        marker(
            &root.path().join("upgrade-staging-2").join("bin"),
            "ax.exe",
            "b",
        );
        let err = sole_leftover_staging(root.path()).unwrap_err();
        assert!(err.contains("2 leftover"), "{err}");
    }

    #[test]
    fn helper_copy_lives_outside_staging() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("upgrade-staging-3");
        let source = staging.join("bin").join("ax.exe");
        marker(source.parent().unwrap(), "ax.exe", "bytes");
        let launch = root.path().join("helper");
        let helper = external_helper(&source, &staging, &launch).unwrap();
        assert!(!helper.starts_with(&staging));
        assert_eq!(std::fs::read(&helper).unwrap(), b"bytes");
    }

    #[test]
    fn helper_inside_staging_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("upgrade-staging-3");
        let source = staging.join("bin").join("ax.exe");
        marker(source.parent().unwrap(), "ax.exe", "bytes");
        let err = external_helper(&source, &staging, &staging.join("again")).unwrap_err();
        assert!(err.contains("must not run from inside"), "{err}");
    }

    #[test]
    fn error_status_is_reported_once_and_ok_is_silent() {
        let root = tempfile::tempdir().unwrap();
        write_upgrade_status(root.path(), &Err("activate new install: locked".into())).unwrap();
        assert_eq!(
            take_upgrade_status(root.path()).as_deref(),
            Some("activate new install: locked")
        );
        assert!(take_upgrade_status(root.path()).is_none());
        write_upgrade_status(root.path(), &Ok(())).unwrap();
        assert!(take_upgrade_status(root.path()).is_none());
        assert!(!upgrade_status_path(root.path()).exists());
    }

    #[test]
    fn recovery_depends_on_current_and_where_the_process_runs() {
        assert_eq!(
            staging_recovery(true, true, Ok(Some(()))),
            StagingRecovery::Idle
        );
        assert_eq!(
            staging_recovery(false, false, Ok(Some(()))),
            StagingRecovery::Promote
        );
        assert_eq!(
            staging_recovery(false, true, Ok(Some(()))),
            StagingRecovery::WaitForExit
        );
        assert_eq!(
            staging_recovery(false, false, Ok(None)),
            StagingRecovery::Idle
        );
        assert_eq!(
            staging_recovery(false, false, Err("2 leftover".into())),
            StagingRecovery::Ambiguous("2 leftover".into())
        );
    }

    #[test]
    fn cmd_batch_parks_current_before_moving_staging() {
        let script = upgrade_cmd_batch(
            4,
            Path::new(r"C:\Users\gary\AppData\Local\ax\upgrade-staging-4"),
            Path::new(r"C:\Users\gary\AppData\Local\ax\current"),
            Path::new(r"C:\Users\gary\AppData\Local\ax\current\bin"),
            Path::new(r"C:\Users\gary\.cargo\bin\ax.exe"),
            Path::new(r"C:\Users\gary\AppData\Local\ax\upgrade-status.txt"),
        );
        let park = script
            .find(r#"move /Y "%DEST%" "%PREVIOUS%""#)
            .expect("park");
        let activate = script
            .find(r#"move /Y "%STAGING%" "%DEST%""#)
            .expect("activate");
        assert!(park < activate, "{script}");
        assert!(
            script.contains(r#"move /Y "%PREVIOUS%" "%DEST%""#),
            "{script}"
        );
        assert!(!script.contains(r#"rmdir /s /q "%DEST%""#), "{script}");
        let dest_guard = script.find(r#"if exist "%DEST%""#).expect("dest guard");
        let wipe_previous = script
            .find(r#"rmdir /s /q "%PREVIOUS%""#)
            .expect("wipe previous");
        assert!(
            dest_guard < wipe_previous,
            "parked install is removed only while current still exists:\n{script}"
        );
        assert!(script.contains("upgrade-status.txt"), "{script}");
    }

    #[test]
    fn publish_bin_exe_copies_the_root_binary_into_bin() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("current");
        marker(&dest, "ax.exe", "fresh");
        let published = publish_bin_exe(&dest).unwrap();
        assert_eq!(published, dest.join("bin").join("ax.exe"));
        assert_eq!(std::fs::read(published).unwrap(), b"fresh");
    }

    #[test]
    fn a_fresh_promotion_claim_blocks_a_second_claim() {
        let root = tempfile::tempdir().unwrap();
        assert!(claim_promotion(root.path()));
        assert!(!claim_promotion(root.path()));
        clear_promotion(root.path());
        assert!(claim_promotion(root.path()));
    }
}
