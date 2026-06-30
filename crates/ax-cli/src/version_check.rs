//! Background and explicit checks for newer ax releases.
//!
//! Primary source: `https://getax.wenneker.io/releases/latest.txt` (plain text, always fresh).
//! Fallback:       GitHub releases API (`GaryWenneker/ax`) for environments where the CDN
//!                 is unreachable or a GitHub token is available.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ui::{ok_line, warn_line};

const CDN_LATEST_URL: &str = "https://getax.wenneker.io/releases/latest.txt";
const DEFAULT_REPO: &str = "GaryWenneker/ax";
const DEFAULT_CACHE_HOURS: u64 = 24;
const REQUEST_TIMEOUT_SECS: u64 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCheckCache {
    checked_at: i64,
    latest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionTriple {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn github_repo() -> String {
    std::env::var("AX_GITHUB_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

/// Token for private repos or higher API rate limits (`AX_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`, then `gh auth token`).
pub fn github_token() -> Option<String> {
    for key in ["AX_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(v) = std::env::var(key) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    github_token_from_gh_cli()
}

fn github_token_from_gh_cli() -> Option<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .user_agent("ax-version-check")
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_latest_via_api(
    client: &reqwest::Client,
    repo: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let api = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut req = client
        .get(&api)
        .header("Accept", "application/vnd.github+json");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub API: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        if token.is_none() {
            return Err(
                "releases not visible (private repo?) — set GITHUB_TOKEN or run `gh auth login`"
                    .to_string(),
            );
        }
        return Err("no published GitHub releases yet".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    body.get("tag_name")
        .and_then(|v| v.as_str())
        .map(normalize_version)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "GitHub did not return a release tag".to_string())
}

/// Parse release tag from `https://github.com/owner/repo/releases/tag/v1.2.3`.
pub fn parse_latest_tag_from_url(url: &str) -> Option<String> {
    let m = regex::Regex::new(r"/releases/tag/([^/?#]+)")
        .ok()?
        .captures(url)?;
    Some(normalize_version(m.get(1)?.as_str()))
}

pub fn normalize_version(v: &str) -> String {
    let t = v.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with('v') {
        t.to_string()
    } else {
        format!("v{t}")
    }
}

pub fn strip_v(v: &str) -> String {
    let t = v.trim();
    if let Some(rest) = t.strip_prefix('v') {
        rest.to_string()
    } else {
        t.to_string()
    }
}

pub fn parse_version_triple(v: &str) -> Option<VersionTriple> {
    let core = strip_v(v);
    let core = core.split('-').next()?.trim();
    let mut parts = core.split('.');
    Some(VersionTriple {
        major: parts.next()?.parse().ok()?,
        minor: parts.next()?.parse().ok()?,
        patch: parts.next()?.parse().ok()?,
    })
}

pub fn compare_versions(a: &str, b: &str) -> Option<i32> {
    let va = parse_version_triple(a)?;
    let vb = parse_version_triple(b)?;
    if va.major != vb.major {
        return Some((va.major as i32) - (vb.major as i32));
    }
    if va.minor != vb.minor {
        return Some((va.minor as i32) - (vb.minor as i32));
    }
    Some((va.patch as i32) - (vb.patch as i32))
}

pub fn is_update_available(current: &str, latest: &str) -> bool {
    match compare_versions(latest, current) {
        Some(cmp) if cmp > 0 => true,
        Some(_) => false,
        None => normalize_version(current) != normalize_version(latest),
    }
}

fn cache_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join("update-check.json"))
        .unwrap_or_else(|| PathBuf::from(".ax/update-check.json"))
}

fn cache_interval() -> Duration {
    let hours = std::env::var("AX_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CACHE_HOURS);
    Duration::from_secs(hours.saturating_mul(3600))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_cache() -> Option<UpdateCheckCache> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_cache(latest: &str) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let cache = UpdateCheckCache {
        checked_at: now_secs(),
        latest: normalize_version(latest),
    };
    if let Ok(json) = serde_json::to_string_pretty(&cache) {
        let _ = std::fs::write(path, json);
    }
}

fn cache_is_fresh(cache: &UpdateCheckCache) -> bool {
    let age = now_secs().saturating_sub(cache.checked_at);
    age >= 0 && Duration::from_secs(age as u64) < cache_interval()
}

pub fn update_check_disabled() -> bool {
    matches!(
        std::env::var("AX_NO_UPDATE_CHECK").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    ) || std::env::var("CI").as_deref() == Ok("true")
}

/// Fetch latest version from `getax.wenneker.io/releases/latest.txt` (primary CDN source).
async fn fetch_latest_from_cdn(client: &reqwest::Client) -> Result<String, String> {
    let resp = client
        .get(CDN_LATEST_URL)
        .send()
        .await
        .map_err(|e| format!("could not reach CDN: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("CDN returned HTTP {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v = text.trim();
    if v.is_empty() {
        return Err("CDN returned empty version".to_string());
    }
    Ok(normalize_version(v))
}

pub async fn resolve_latest_version(repo: &str) -> Result<String, String> {
    let client = http_client()?;

    // Primary: CDN latest.txt — always up to date, no auth needed.
    if let Ok(tag) = fetch_latest_from_cdn(&client).await {
        return Ok(tag);
    }

    // Fallback: GitHub releases API (works with a token for private repos).
    let token = github_token();
    let token_ref = token.as_deref();

    if token_ref.is_some() {
        if let Ok(tag) = fetch_latest_via_api(&client, repo, token_ref).await {
            return Ok(tag);
        }
    }

    let url = format!("https://github.com/{repo}/releases/latest");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub: {e}"))?;

    if resp.status().is_success() {
        if let Some(tag) = parse_latest_tag_from_url(resp.url().as_str()) {
            return Ok(tag);
        }
    }

    fetch_latest_via_api(&client, repo, token_ref).await
}

async fn latest_with_cache(repo: &str) -> Option<String> {
    if let Some(cache) = load_cache() {
        if cache_is_fresh(&cache) {
            return Some(cache.latest);
        }
    }
    match resolve_latest_version(repo).await {
        Ok(latest) => {
            save_cache(&latest);
            Some(latest)
        }
        Err(e) => {
            tracing::debug!("update check skipped: {e}");
            load_cache().map(|c| c.latest)
        }
    }
}

pub fn print_update_notice(current: &str, latest: &str) {
    let current = normalize_version(current);
    let latest = normalize_version(latest);
    eprintln!();
    eprintln!(
        "{}",
        warn_line(format!(
            "Update available: {} → {}. Run `ax upgrade` to install.",
            strip_v(&current),
            strip_v(&latest)
        ))
    );
    eprintln!();
}

/// Non-blocking-ish notice after CLI commands (respects cache + env gates).
pub async fn maybe_notify_update() {
    if update_check_disabled() {
        return;
    }
    let current = current_version();
    let repo = github_repo();
    let Some(latest) = latest_with_cache(&repo).await else {
        return;
    };
    if is_update_available(current, &latest) {
        print_update_notice(current, &latest);
    }
}

/// Explicit `ax upgrade --check` (always hits network unless cache is fresh and shows result).
pub async fn run_check(force_refresh: bool) -> Result<(), String> {
    let current = current_version();
    let repo = github_repo();
    let latest = if force_refresh {
        match resolve_latest_version(&repo).await {
            Ok(latest) => {
                save_cache(&latest);
                latest
            }
            Err(e) if e.contains("no published GitHub releases") => {
                eprintln!(
                    "{}",
                    ok_line(format!(
                        "No published releases yet (installed {})",
                        strip_v(current)
                    ))
                );
                return Ok(());
            }
            Err(e) if e.contains("private repo") => {
                eprintln!("{}", warn_line(e));
                eprintln!("  Set GITHUB_TOKEN or run `gh auth login`, then retry `ax upgrade --check`.");
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", warn_line(format!("Could not check for updates: {e}")));
                return Ok(());
            }
        }
    } else {
        match latest_with_cache(&repo).await {
            Some(latest) => latest,
            None => {
                eprintln!(
                    "{}",
                    warn_line("Could not check for updates (offline or no releases)")
                );
                return Ok(());
            }
        }
    };

    if is_update_available(current, &latest) {
        eprintln!(
            "{}",
            warn_line(format!(
                "An update is available: {} → {}",
                strip_v(current),
                strip_v(&latest)
            ))
        );
        eprintln!("  Run `ax upgrade` to install.");
    } else {
        eprintln!(
            "{}",
            ok_line(format!("Already up to date ({})", strip_v(current)))
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_from_redirect_url() {
        assert_eq!(
            parse_latest_tag_from_url("https://github.com/GaryWenneker/ax/releases/tag/v0.2.0"),
            Some("v0.2.0".into())
        );
    }

    #[test]
    fn compare_and_update_available() {
        assert!(is_update_available("0.1.0", "0.2.0"));
        assert!(!is_update_available("0.2.0", "0.2.0"));
        assert!(!is_update_available("0.3.0", "0.2.0"));
        assert!(is_update_available("v0.1.0", "v0.1.1"));
    }
}
