//! `GET /api/changes`: one SSE feed that names the data area that changed.
//!
//! Pages subscribe once and reload only their own topic. The feed checks the
//! database files every second and runs its fingerprint queries only when a
//! file (or its WAL) changed, so an idle project costs one `stat` per file.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures_util::Stream;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::workspace_state::WebHub;

pub const TOPICS: [&str; 5] = ["graph", "memory", "rules", "skills", "usage"];

pub type Versions = BTreeMap<&'static str, String>;

const TICK: Duration = Duration::from_secs(1);

const GRAPH_SQL: &str = "SELECT \
    (SELECT count(*) || ':' || coalesce(max(rowid), 0) || ':' || coalesce(max(updated_at), '') FROM nodes) || '|' || \
    (SELECT count(*) || ':' || coalesce(max(rowid), 0) FROM edges) || '|' || \
    (SELECT count(*) || ':' || coalesce(max(indexed_at), '') FROM files) || '|' || \
    (SELECT count(*) FROM unresolved_refs)";

const MEMORY_SQL: &str = "SELECT count(*) || ':' || coalesce(max(updated_at), '') || ':' || total(enabled) FROM memories";

const RULES_SQL: &str = "SELECT count(*) || ':' || coalesce(group_concat(\
    id || '/' || coalesce(status, '') || '/' || coalesce(enabled, '') || '/' || coalesce(content_hash, ''), ','), '') \
    FROM (SELECT * FROM policy_rules ORDER BY id)";

const SKILLS_SQL: &str = "SELECT count(*) || ':' || coalesce(group_concat(\
    name || '/' || coalesce(status, '') || '/' || coalesce(enabled, '') || '/' || coalesce(content_hash, ''), ','), '') \
    FROM (SELECT * FROM policy_skills ORDER BY name)";

const GLOBAL_RULES_SQL: &str =
    "SELECT count(*) || ':' || coalesce(max(synced_at), '') FROM global_policy_rules";

const GLOBAL_SKILLS_SQL: &str =
    "SELECT count(*) || ':' || coalesce(max(synced_at), '') FROM global_policy_skills";

const USAGE_SQL: &str = "SELECT \
    (SELECT coalesce(max(id), 0) FROM mcp_call_log) || '|' || \
    (SELECT coalesce(max(id), 0) || ':' || coalesce(max(source_mtime), 0) FROM agent_session_log) || '|' || \
    (SELECT count(*) FROM model_price_daily) || '|' || \
    (SELECT coalesce(max(last_attempt_at), 0) FROM pricing_sync_meta)";

/// Topics whose fingerprint differs. A topic missing from `prev` is not a change.
pub fn changed_topics(prev: &Versions, next: &Versions) -> Vec<&'static str> {
    next.iter()
        .filter(|(topic, version)| prev.get(*topic).is_some_and(|old| old != *version))
        .map(|(topic, _)| *topic)
        .collect()
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(handle_changes))
        .with_state(hub)
}

/// A missing table or database yields an empty part, never an error.
async fn scalar(pool: &SqlitePool, sql: &str) -> String {
    sqlx::query_scalar::<_, String>(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_default()
}

fn digest(parts: &[&str]) -> String {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Read-only pools for the machine-wide databases, opened once they exist.
#[derive(Default)]
struct SidePools {
    global: Option<SqlitePool>,
    usage: Option<SqlitePool>,
}

async fn open_read_only(path: &Path) -> Option<SqlitePool> {
    if !path.is_file() {
        return None;
    }
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .ok()
}

impl SidePools {
    async fn refresh(&mut self, global: Option<&Path>, usage: &Path) {
        if self.global.is_none() {
            if let Some(path) = global {
                self.global = open_read_only(path).await;
            }
        }
        if self.usage.is_none() {
            self.usage = open_read_only(usage).await;
        }
    }
}

async fn collect_versions(project: &SqlitePool, side: &SidePools) -> Versions {
    let (global_rules, global_skills) = match &side.global {
        Some(pool) => (scalar(pool, GLOBAL_RULES_SQL).await, scalar(pool, GLOBAL_SKILLS_SQL).await),
        None => (String::new(), String::new()),
    };
    let usage = match &side.usage {
        Some(pool) => scalar(pool, USAGE_SQL).await,
        None => String::new(),
    };
    let mut out = Versions::new();
    out.insert("graph", digest(&[&scalar(project, GRAPH_SQL).await]));
    out.insert("memory", digest(&[&scalar(project, MEMORY_SQL).await]));
    out.insert("rules", digest(&[&scalar(project, RULES_SQL).await, &global_rules]));
    out.insert("skills", digest(&[&scalar(project, SKILLS_SQL).await, &global_skills]));
    out.insert("usage", digest(&[&usage]));
    out
}

fn with_wal(path: &Path) -> [PathBuf; 2] {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    [path.to_path_buf(), PathBuf::from(wal)]
}

/// `(modified, len)` for every watched file; a missing file is `(None, 0)`.
fn files_stamp(paths: &[PathBuf]) -> Vec<(Option<SystemTime>, u64)> {
    paths
        .iter()
        .map(|p| match std::fs::metadata(p) {
            Ok(m) => (m.modified().ok(), m.len()),
            Err(_) => (None, 0),
        })
        .collect()
}

fn change_event(topic: &str, version: &str) -> Event {
    let data = serde_json::json!({ "topic": topic, "version": version });
    Event::default().event("change").data(data.to_string())
}

async fn handle_changes(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let global_path = ax_global_db::global_db_path().ok();
    let usage_path = ax_usage::usage_db_path();
    let s = async_stream::stream! {
        let mut side = SidePools::default();
        let mut prev: Option<(PathBuf, Versions)> = None;
        let mut last_stamp = None;
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            let (root, db_path, pool) = {
                let ws = hub.read().await;
                (ws.project_root.clone(), ws.db_path.clone(), ws.graph_pool.clone())
            };
            let mut watched: Vec<PathBuf> = with_wal(&db_path).into();
            if let Some(g) = &global_path {
                watched.extend(with_wal(g));
            }
            watched.extend(with_wal(&usage_path));
            let stamp = files_stamp(&watched);
            let same_root = prev.as_ref().is_some_and(|(r, _)| *r == root);
            if same_root && last_stamp.as_ref() == Some(&stamp) {
                continue;
            }
            last_stamp = Some(stamp);
            side.refresh(global_path.as_deref(), &usage_path).await;
            let next = collect_versions(&pool, &side).await;
            match &prev {
                Some((old_root, old)) if *old_root == root => {
                    for topic in changed_topics(old, &next) {
                        yield Ok(change_event(topic, &next[topic]));
                    }
                }
                Some(_) => {
                    for topic in TOPICS {
                        yield Ok(change_event(topic, &next[topic]));
                    }
                }
                None => {}
            }
            prev = Some((root, next));
        }
    };
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(pairs: &[(&'static str, &str)]) -> Versions {
        pairs.iter().map(|(k, val)| (*k, (*val).to_string())).collect()
    }

    #[test]
    fn unchanged_versions_report_nothing() {
        let a = v(&[("graph", "1"), ("memory", "2")]);
        assert!(changed_topics(&a, &a.clone()).is_empty());
    }

    #[test]
    fn only_the_changed_topic_is_reported() {
        let a = v(&[("graph", "1"), ("memory", "2")]);
        let b = v(&[("graph", "1"), ("memory", "3")]);
        assert_eq!(changed_topics(&a, &b), vec!["memory"]);
    }

    #[test]
    fn a_topic_seen_for_the_first_time_is_not_a_change() {
        let a = v(&[("graph", "1")]);
        let b = v(&[("graph", "1"), ("memory", "3")]);
        assert!(changed_topics(&a, &b).is_empty());
    }
}
