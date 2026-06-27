//! Anonymous usage telemetry for ax (see `docs/TELEMETRY.md`).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TELEMETRY_ENDPOINT: &str = "https://getax.wenneker.io/v1/events";
pub const TELEMETRY_DOCS: &str = "https://github.com/GaryWenneker/ax/blob/main/docs/TELEMETRY.md";

const SCHEMA_VERSION: i32 = 1;
const MAX_BUFFER_BYTES: usize = 256 * 1024;
const MAX_EVENTS_PER_REQUEST: usize = 100;
pub const DEFAULT_FLUSH_TIMEOUT_MS: u64 = 1500;

const STALE_CLAIM_MS: u64 = 60 * 60 * 1000;

static TELEMETRY: OnceLock<Mutex<Telemetry>> = OnceLock::new();

pub fn telemetry() -> &'static Mutex<Telemetry> {
    TELEMETRY.get_or_init(|| Mutex::new(Telemetry::new(default_dir())))
}

pub fn default_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax"))
        .unwrap_or_else(|| PathBuf::from(".ax"))
}

pub type UsageKind = &'static str;

pub fn bucket_file_count(n: u32) -> &'static str {
    match n {
        0..=99 => "<100",
        100..=999 => "100-1k",
        1000..=9999 => "1k-10k",
        _ => "10k+",
    }
}

pub fn bucket_duration(ms: u64) -> &'static str {
    match ms {
        0..=9999 => "<10s",
        10000..=59999 => "10-60s",
        60000..=299999 => "1-5m",
        _ => "5m+",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub decided_by: String,
    pub machine_id: Option<String>,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigFile {
    enabled: bool,
    machine_id: String,
    consent_source: String,
    #[serde(default)]
    first_run_notice_shown: bool,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum BufferLine {
    Count(CountLine),
    Event(EventLine),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CountLine {
    v: i32,
    d: String,
    k: String,
    n: String,
    c: u32,
    e: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cv: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventLine {
    v: i32,
    ev: String,
    ts: String,
    props: serde_json::Value,
}

pub struct Telemetry {
    dir: PathBuf,
    counts: HashMap<String, CountLine>,
    events: Vec<EventLine>,
    config_cache: Option<Option<ConfigFile>>,
}

impl Telemetry {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            counts: HashMap::new(),
            events: Vec::new(),
            config_cache: None,
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("telemetry.json")
    }

    pub fn queue_path(&self) -> PathBuf {
        self.dir.join("telemetry-queue.jsonl")
    }

    pub fn get_status(&mut self) -> TelemetryStatus {
        let config = self.read_config();
        let machine_id = config.as_ref().map(|c| c.machine_id.clone());
        let config_path = self.config_path();

        if env_truthy("DO_NOT_TRACK") {
            return TelemetryStatus {
                enabled: false,
                decided_by: "DO_NOT_TRACK".into(),
                machine_id,
                config_path,
            };
        }
        if let Some(forced) = std::env::var("AX_TELEMETRY").ok() {
            let on = !matches!(forced.as_str(), "0" | "false" | "FALSE");
            return TelemetryStatus {
                enabled: on,
                decided_by: "AX_TELEMETRY".into(),
                machine_id,
                config_path,
            };
        }
        if let Some(cfg) = config {
            return TelemetryStatus {
                enabled: cfg.enabled,
                decided_by: "config".into(),
                machine_id: Some(cfg.machine_id),
                config_path,
            };
        }
        TelemetryStatus {
            enabled: true,
            decided_by: "default".into(),
            machine_id,
            config_path,
        }
    }

    pub fn is_enabled(&mut self) -> bool {
        self.get_status().enabled
    }

    pub fn set_enabled(&mut self, enabled: bool, source: &str) {
        let existing = self.read_config();
        let machine_id = existing
            .map(|c| c.machine_id)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        self.write_config(ConfigFile {
            enabled,
            machine_id,
            consent_source: source.into(),
            first_run_notice_shown: true,
            updated_at: iso_timestamp(),
        });
        self.config_cache = None;
        if !enabled {
            let _ = fs::remove_file(self.queue_path());
        }
    }

    pub fn has_stored_choice(&mut self) -> bool {
        self.read_config().is_some()
    }

    pub fn record_usage(&mut self, kind: UsageKind, name: &str, ok: bool, client: Option<&ClientInfo>) {
        if !self.is_enabled() {
            return;
        }
        let day = utc_day();
        let cn = client.and_then(|c| c.name.as_ref()).map(|s| truncate(s, 64));
        let cv = client.and_then(|c| c.version.as_ref()).map(|s| truncate(s, 32));
        let key = format!(
            "{}\0{}\0{}\0{}\0{}",
            day,
            kind,
            truncate(name, 64),
            cn.as_deref().unwrap_or(""),
            cv.as_deref().unwrap_or("")
        );
        if let Some(line) = self.counts.get_mut(&key) {
            line.c += 1;
            if !ok {
                line.e += 1;
            }
        } else {
            self.counts.insert(
                key,
                CountLine {
                    v: SCHEMA_VERSION,
                    d: day,
                    k: kind.to_string(),
                    n: truncate(name, 64),
                    c: 1,
                    e: if ok { 0 } else { 1 },
                    cn,
                    cv,
                },
            );
        }
    }

    pub fn record_lifecycle(&mut self, event: &str, props: serde_json::Value) {
        if !self.is_enabled() {
            return;
        }
        self.events.push(EventLine {
            v: SCHEMA_VERSION,
            ev: event.into(),
            ts: iso_timestamp(),
            props,
        });
    }

    pub fn record_index_event(languages: &[String], files_indexed: u32, duration_ms: u64) {
        if let Ok(mut guard) = telemetry().lock() {
            guard.record_lifecycle(
                "index",
                serde_json::json!({
                    "languages": languages,
                    "file_count_bucket": bucket_file_count(files_indexed),
                    "duration_bucket": bucket_duration(duration_ms),
                }),
            );
        }
    }

    pub fn persist_sync(&mut self) {
        if self.counts.is_empty() && self.events.is_empty() {
            return;
        }
        let mut lines: Vec<BufferLine> = self
            .counts
            .values()
            .cloned()
            .map(BufferLine::Count)
            .chain(self.events.iter().cloned().map(BufferLine::Event))
            .collect();
        self.counts.clear();
        self.events.clear();
        self.append_lines(&lines);
    }

    pub fn maybe_flush(&mut self) {
        let _ = self.flush_now(DEFAULT_FLUSH_TIMEOUT_MS);
    }

    pub async fn flush_now(&mut self, timeout_ms: u64) {
        if !self.is_enabled() {
            return;
        }
        self.persist_sync();
        self.recover_stale_claims();
        let claim = self.claim_queue();
        if claim.is_none() {
            return;
        }
        let (claim_path, lines) = claim.unwrap();
        let today = utc_day();
        let mut sendable = Vec::new();
        let mut keep = Vec::new();
        for line in lines {
            match &line {
                BufferLine::Event(_) => sendable.push(line),
                BufferLine::Count(c) if c.d < today => sendable.push(line),
                _ => keep.push(line),
            }
        }
        let failed = if sendable.is_empty() {
            Vec::new()
        } else {
            self.send_batch(&sendable, timeout_ms).await
        };
        let back: Vec<BufferLine> = failed.into_iter().chain(keep).collect();
        if !back.is_empty() {
            self.append_lines(&back);
        }
        let _ = fs::remove_file(claim_path);
    }

    fn read_config(&mut self) -> Option<ConfigFile> {
        if self.config_cache.is_none() {
            let path = self.config_path();
            let cfg = fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            self.config_cache = Some(cfg);
        }
        self.config_cache.clone().flatten()
    }

    fn write_config(&self, cfg: ConfigFile) {
        let _ = fs::create_dir_all(&self.dir);
        let path = self.config_path();
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = fs::write(path, format!("{}\n", json));
        }
    }

    fn append_lines(&self, lines: &[BufferLine]) {
        let _ = fs::create_dir_all(&self.dir);
        let mut payload = String::new();
        for line in lines {
            if let Ok(s) = serde_json::to_string(line) {
                payload.push_str(&s);
                payload.push('\n');
            }
        }
        let path = self.queue_path();
        let mut existing = fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&payload);
        if existing.len() > MAX_BUFFER_BYTES {
            existing = existing[existing.len().saturating_sub(MAX_BUFFER_BYTES)..].to_string();
            if let Some(pos) = existing.find('\n') {
                existing = existing[pos + 1..].to_string();
            }
        }
        let _ = fs::write(path, existing);
    }

    fn claim_queue(&self) -> Option<(PathBuf, Vec<BufferLine>)> {
        let claim_path = self
            .dir
            .join(format!("telemetry-queue.sending.{}.jsonl", std::process::id()));
        let queue = self.queue_path();
        if fs::rename(&queue, &claim_path).is_err() {
            return None;
        }
        let content = fs::read_to_string(&claim_path).unwrap_or_default();
        let mut lines = Vec::new();
        for raw in content.lines() {
            if raw.trim().is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<BufferLine>(raw) {
                lines.push(parsed);
            }
        }
        Some((claim_path, lines))
    }

    fn recover_stale_claims(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let cutoff = now_ms() - STALE_CLAIM_MS;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("telemetry-queue.sending.") {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if modified >= cutoff {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            let _ = fs::remove_file(&path);
            if content.trim().is_empty() {
                continue;
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.queue_path());
            if let Ok(mut f) = file {
                let _ = f.write_all(content.as_bytes());
            }
        }
    }

    async fn send_batch(&self, lines: &[BufferLine], timeout_ms: u64) -> Vec<BufferLine> {
        let config = self.read_config_sync();
        if config.is_none() {
            return Vec::new();
        }
        let config = config.unwrap();
        let events: Vec<serde_json::Value> = lines
            .iter()
            .map(|line| match line {
                BufferLine::Event(e) => serde_json::json!({
                    "event": e.ev,
                    "ts": e.ts,
                    "props": e.props,
                }),
                BufferLine::Count(c) => {
                    let mut props = serde_json::json!({
                        "kind": c.k,
                        "name": c.n,
                        "count": c.c,
                        "error_count": c.e,
                    });
                    if let Some(cn) = &c.cn {
                        props["client_name"] = serde_json::Value::String(cn.clone());
                    }
                    if let Some(cv) = &c.cv {
                        props["client_version"] = serde_json::Value::String(cv.clone());
                    }
                    serde_json::json!({
                        "event": "usage_rollup",
                        "ts": format!("{}T12:00:00.000Z", c.d),
                        "props": props,
                    })
                }
            })
            .collect();

        let endpoint = std::env::var("AX_TELEMETRY_ENDPOINT")
            .unwrap_or_else(|_| TELEMETRY_ENDPOINT.to_string());
        let envelope = serde_json::json!({
            "machine_id": config.machine_id,
            "ax_version": ax_version(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "ci": env_truthy("CI"),
            "schema_version": SCHEMA_VERSION,
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_default();

        for i in (0..events.len()).step_by(MAX_EVENTS_PER_REQUEST) {
            let end = (i + MAX_EVENTS_PER_REQUEST).min(events.len());
            let chunk = &events[i..end];
            let body = serde_json::json!({
                "machine_id": envelope["machine_id"],
                "ax_version": envelope["ax_version"],
                "os": envelope["os"],
                "arch": envelope["arch"],
                "ci": envelope["ci"],
                "schema_version": envelope["schema_version"],
                "events": chunk,
            });
            let res = client
                .post(&endpoint)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await;
            if res.is_err() {
                return lines[i..].to_vec();
            }
        }
        Vec::new()
    }

    fn read_config_sync(&self) -> Option<ConfigFile> {
        fs::read_to_string(self.config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }
}

pub fn ax_version() -> String {
    std::env::var("AX_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn utc_day() -> String {
    // ISO date without chrono dependency
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Approximate UTC day from epoch — good enough for rollup buckets
    let days = secs / 86400;
    // 1970-01-01 + days — use simple formatting via offset from known date
    days_since_epoch_to_ymd(days)
}

fn days_since_epoch_to_ymd(days: u64) -> String {
    // Algorithm from Howard Hinnant
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + 3 - 12 * (mp / 10);
    let year = y + (if m <= 2 { 1 } else { 0 }) as u64;
    format!("{:04}-{:02}-{:02}", year, m, d)
}

fn iso_timestamp() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let day_secs = secs % 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}Z",
        days_since_epoch_to_ymd(secs / 86400),
        h,
        m,
        s,
        millis
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => !v.is_empty() && v != "0" && v.to_lowercase() != "false",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets() {
        assert_eq!(bucket_file_count(50), "<100");
        assert_eq!(bucket_duration(5000), "<10s");
    }

    #[test]
    fn set_enabled_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = Telemetry::new(dir.path().to_path_buf());
        t.set_enabled(false, "cli");
        assert!(!t.is_enabled());
        t.set_enabled(true, "cli");
        assert!(t.is_enabled());
    }
}
