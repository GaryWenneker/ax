//! Daily MCP verbose log files under `<project>/.ax/mcp-verbose-YYYY-MM-DD.log`.
//!
//! Calendar day boundaries follow `[ui].timezone` in `.ax/ship.toml` (empty / `local` = host local).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Deserialize;

pub const LEGACY_LOG_NAME: &str = "mcp-verbose.log";
pub const DATED_LOG_PREFIX: &str = "mcp-verbose-";
pub const DATED_LOG_SUFFIX: &str = ".log";

#[derive(Debug, Deserialize, Default)]
struct ShipUiFile {
    #[serde(default)]
    ui: UiSection,
}

#[derive(Debug, Deserialize, Default)]
struct UiSection {
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    verbose_mcp: bool,
}

/// True when `AX_MCP_VERBOSE` is truthy or project `[ui].verbose_mcp` is set.
/// Domain + MCP verbose appends no-op unless this returns true.
pub fn verbose_enabled(project_root: Option<&Path>) -> bool {
    if env_flag_truthy("AX_MCP_VERBOSE") {
        return true;
    }
    let Some(root) = project_root else {
        return false;
    };
    read_ship_verbose_mcp(root)
}

fn env_flag_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn read_ship_verbose_mcp(project_root: &Path) -> bool {
    let root = strip_verbatim_prefix(project_root);
    let path = root.join(".ax").join("ship.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    toml::from_str::<ShipUiFile>(&text)
        .map(|c| c.ui.verbose_mcp)
        .unwrap_or(false)
}

/// Path to today's log file for a project (after legacy migration attempt).
pub fn mcp_verbose_log_path(project_root: &Path) -> PathBuf {
    current_log_path(Some(project_root))
}

/// Active dated log path for `project_root` (or `~/.ax/` when None).
pub fn current_log_path(project_root: Option<&Path>) -> PathBuf {
    let ax_dir = ax_dir_for(project_root);
    let _ = migrate_legacy_log(project_root);
    let day = rotation_calendar_date(project_root, Utc::now());
    path_for_date(&ax_dir, day)
}

pub fn path_for_date(ax_dir: &Path, day: NaiveDate) -> PathBuf {
    ax_dir.join(format!(
        "{DATED_LOG_PREFIX}{}{DATED_LOG_SUFFIX}",
        day.format("%Y-%m-%d")
    ))
}

pub fn parse_log_day_from_filename(name: &str) -> Option<NaiveDate> {
    let stem = name.strip_prefix(DATED_LOG_PREFIX)?.strip_suffix(DATED_LOG_SUFFIX)?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// Calendar date in the configured rotation timezone at `instant`.
pub fn rotation_calendar_date(project_root: Option<&Path>, instant: DateTime<Utc>) -> NaiveDate {
    let tz_str = read_ship_timezone(project_root);
    if tz_str.is_empty() || tz_str.eq_ignore_ascii_case("local") {
        return instant.with_timezone(&Local).date_naive();
    }
    if let Ok(tz) = tz_str.parse::<Tz>() {
        return instant.with_timezone(&tz).date_naive();
    }
    instant.date_naive()
}

pub fn read_ship_timezone(project_root: Option<&Path>) -> String {
    let Some(root) = project_root else {
        return String::new();
    };
    let root = strip_verbatim_prefix(root);
    let path = root.join(".ax").join("ship.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return String::new();
    };
    toml::from_str::<ShipUiFile>(&text)
        .map(|c| c.ui.timezone.trim().to_string())
        .unwrap_or_default()
}

/// Append trace lines with UTC ISO timestamps (one line per event).
/// No-op when verbose MCP logging is off (`AX_MCP_VERBOSE` / `[ui].verbose_mcp`).
pub fn append_verbose_log(lines: &[String], project_root: Option<&Path>) {
    if lines.is_empty() || !verbose_enabled(project_root) {
        return;
    }
    let _ = migrate_legacy_log(project_root);
    let path = current_log_path(project_root);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let ts = iso_timestamp_utc();
    for line in lines {
        let body = sanitize_one_line(line);
        let _ = writeln!(f, "{ts} {body}");
    }
}

/// All dated log files for a project, oldest first.
pub fn list_dated_log_files(project_root: &Path) -> Vec<(NaiveDate, PathBuf)> {
    let _ = migrate_legacy_log(Some(project_root));
    let ax_dir = ax_dir_for(Some(project_root));
    let mut out = Vec::new();
    let Ok(read) = fs::read_dir(&ax_dir) else {
        return out;
    };
    for entry in read.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(day) = parse_log_day_from_filename(&name) {
            out.push((day, entry.path()));
        }
    }
    out.sort_by_key(|(d, _)| *d);
    out
}

pub fn read_log_file_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub fn read_log_for_day(project_root: &Path, day: NaiveDate) -> String {
    let path = path_for_date(&ax_dir_for(Some(project_root)), day);
    read_log_file_text(&path)
}

pub fn previous_calendar_day(day: NaiveDate) -> NaiveDate {
    day.pred_opt().unwrap_or(day)
}

pub fn has_older_log_day(project_root: &Path, day: NaiveDate) -> bool {
    let prev = previous_calendar_day(day);
    path_for_date(&ax_dir_for(Some(project_root)), prev).is_file()
}

/// Nearest existing dated log file strictly before `before`, skipping gaps
/// (a day with verbose logging off, or a historical rotation bug) instead of
/// dead-ending on the very next calendar day like `has_older_log_day` does.
/// Returns the found day plus whether an even-older dated file exists beyond it.
pub fn nearest_dated_log_before(
    project_root: &Path,
    before: NaiveDate,
) -> Option<(NaiveDate, bool)> {
    let older: Vec<NaiveDate> = list_dated_log_files(project_root)
        .into_iter()
        .map(|(d, _)| d)
        .filter(|d| *d < before)
        .collect();
    let &day = older.last()?;
    let has_older = older.len() > 1;
    Some((day, has_older))
}

/// Merge all verbose logs (dated + any unmigrated legacy) for audit windows.
pub fn read_merged_verbose_log(project_root: &Path) -> String {
    let _ = migrate_legacy_log(Some(project_root));
    let mut parts = Vec::new();
    for (_day, path) in list_dated_log_files(project_root) {
        let text = read_log_file_text(&path);
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    let legacy = ax_dir_for(Some(project_root)).join(LEGACY_LOG_NAME);
    if legacy.is_file() {
        let text = read_log_file_text(&legacy);
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    parts.join("\n")
}

/// Idempotent: move or merge legacy monolithic log into a dated file.
pub fn migrate_legacy_log(project_root: Option<&Path>) -> std::io::Result<()> {
    let ax_dir = ax_dir_for(project_root);
    let legacy = ax_dir.join(LEGACY_LOG_NAME);
    if !legacy.is_file() {
        return Ok(());
    }
    let _ = fs::create_dir_all(&ax_dir);
    let meta = fs::metadata(&legacy)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0))
        .flatten();
    let text = fs::read_to_string(&legacy)?;
    let day = infer_legacy_log_day(project_root, &text, mtime);
    let target = path_for_date(&ax_dir, day);
    if target.exists() {
        let mut existing = fs::read_to_string(&target)?;
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&text);
        fs::write(&target, existing)?;
    } else {
        fs::rename(&legacy, &target)?;
        return Ok(());
    }
    fs::remove_file(&legacy)?;
    Ok(())
}

fn infer_legacy_log_day(
    project_root: Option<&Path>,
    text: &str,
    mtime: Option<DateTime<Utc>>,
) -> NaiveDate {
    if let Some(first) = text.lines().find(|l| !l.trim().is_empty()) {
        if let Some(ts) = parse_line_timestamp(first) {
            return rotation_calendar_date(project_root, ts);
        }
    }
    if let Some(ts) = mtime {
        return rotation_calendar_date(project_root, ts);
    }
    rotation_calendar_date(project_root, Utc::now())
}

fn parse_line_timestamp(line: &str) -> Option<DateTime<Utc>> {
    let trimmed = line.trim();
    if trimmed.len() < 20 {
        return None;
    }
    let prefix = trimmed.get(..24)?;
    if !prefix.as_bytes().get(10).is_some_and(|b| *b == b'T') {
        return None;
    }
    DateTime::parse_from_rfc3339(prefix)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            let z = if trimmed.starts_with("20") && trimmed.len() >= 20 {
                format!("{}Z", &trimmed[..19])
            } else {
                return None;
            };
            DateTime::parse_from_rfc3339(&z).ok().map(|dt| dt.with_timezone(&Utc))
        })
}

fn ax_dir_for(project_root: Option<&Path>) -> PathBuf {
    if let Some(root) = project_root {
        strip_verbatim_prefix(root).join(".ax")
    } else {
        dirs::home_dir()
            .map(|h| h.join(".ax"))
            .unwrap_or_else(|| PathBuf::from(".ax"))
    }
}

fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn sanitize_one_line(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' | '\r' => '⏎',
            '\t' => ' ',
            other => other,
        })
        .collect()
}

fn iso_timestamp_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let millis = dur.subsec_millis();
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days);
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_enabled_reads_ship_toml() {
        let dir = tempfile_dir();
        let ax = dir.join(".ax");
        fs::create_dir_all(&ax).unwrap();
        fs::write(
            ax.join("ship.toml"),
            "[ui]\nverbose_mcp = true\nshow_savings = true\n",
        )
        .unwrap();
        assert!(verbose_enabled(Some(&dir)));
        fs::write(ax.join("ship.toml"), "[ui]\nshow_savings = true\n").unwrap();
        // Env may still force-enable in developer shells — only assert ship.toml path
        // when AX_MCP_VERBOSE is unset.
        if std::env::var("AX_MCP_VERBOSE").is_err() {
            assert!(!verbose_enabled(Some(&dir)));
        }
    }

    #[test]
    fn dated_filename_roundtrip() {
        let day = NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
        let p = path_for_date(Path::new("/tmp/.ax"), day);
        assert!(p.to_string_lossy().ends_with("mcp-verbose-2026-07-24.log"));
        assert_eq!(
            parse_log_day_from_filename("mcp-verbose-2026-07-24.log"),
            Some(day)
        );
    }

    #[test]
    fn legacy_migration_idempotent() {
        let dir = tempfile_dir();
        let ax = dir.join(".ax");
        fs::create_dir_all(&ax).unwrap();
        fs::write(ax.join(LEGACY_LOG_NAME), "2026-07-20T10:00:00.000Z [ax-mcp] test\n").unwrap();
        migrate_legacy_log(Some(&dir)).unwrap();
        assert!(!ax.join(LEGACY_LOG_NAME).exists());
        assert!(ax.join("mcp-verbose-2026-07-20.log").exists());
        migrate_legacy_log(Some(&dir)).unwrap();
        assert!(!ax.join(LEGACY_LOG_NAME).exists());
    }

    #[test]
    fn nearest_dated_log_before_skips_gaps() {
        let dir = tempfile_dir();
        let ax = dir.join(".ax");
        fs::create_dir_all(&ax).unwrap();
        // Gap between 07-22 and 07-25: no 07-23 / 07-24 files (mirrors a stale
        // daemon that mixed real activity into the wrong dated file for two days).
        fs::write(ax.join("mcp-verbose-2026-07-22.log"), "line\n").unwrap();
        fs::write(ax.join("mcp-verbose-2026-07-20.log"), "line\n").unwrap();

        let before = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let (day, has_older) = nearest_dated_log_before(&dir, before).unwrap();
        assert_eq!(day, NaiveDate::from_ymd_opt(2026, 7, 22).unwrap());
        assert!(has_older, "07-20 exists before 07-22");

        let before2 = NaiveDate::from_ymd_opt(2026, 7, 22).unwrap();
        let (day2, has_older2) = nearest_dated_log_before(&dir, before2).unwrap();
        assert_eq!(day2, NaiveDate::from_ymd_opt(2026, 7, 20).unwrap());
        assert!(!has_older2, "nothing before 07-20");

        let before3 = NaiveDate::from_ymd_opt(2026, 7, 20).unwrap();
        assert!(nearest_dated_log_before(&dir, before3).is_none());
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-vlog-{}", uuid_simple()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uuid_simple() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
