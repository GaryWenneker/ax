use super::*;
use crate::store::open_pool_at;
use crate::tokenizer::count_tokens;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    dir: PathBuf,
    pool: SqlitePool,
}

impl Fixture {
    async fn new() -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ax-reuse-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn alpha() {}\n").unwrap();
        std::fs::write(dir.join("src/other.rs"), "fn beta() {}\n").unwrap();
        let pool = open_pool_at(&dir.join("usage.db")).await.unwrap();
        Fixture { dir, pool }
    }

    fn root(&self) -> &Path {
        &self.dir
    }

    async fn store(&self, conv: &str, tool: &str, args: &Value, body: &str) -> Option<String> {
        store_reply(&self.pool, self.root(), conv, tool, args, body, 2_000_000)
            .await
            .unwrap()
    }

    async fn lookup(&self, conv: &str, tool: &str, args: &Value) -> Option<ReuseHit> {
        lookup(&self.pool, self.root(), conv, tool, args).await
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn node_body() -> String {
    "# Node: alpha\n## Entry 1: src/lib.rs::alpha (Function) — src/lib.rs:1-1\n1\tfn alpha() {}\n".repeat(20)
}

fn args() -> Value {
    json!({"symbol": "alpha", "file": "src/lib.rs"})
}

// ---------- L1: pure functions ----------

#[test]
fn cacheable_tools_are_the_read_only_graph_tools() {
    for tool in [
        "ax_explore", "ax_search", "ax_node", "ax_callers", "ax_callees", "ax_impact", "ax_path",
        "ax_affected", "ax_context",
    ] {
        assert!(reuse_cacheable(tool), "{tool} should be cacheable");
    }
    for tool in [
        "ax_preflight", "ax_guard", "ax_sync", "ax_remember", "ax_policy_capture", "ax_expand",
        "ax_stash", "ax_rules", "ax_skill", "ax_index", "unknown",
    ] {
        assert!(!reuse_cacheable(tool), "{tool} must never be cached");
    }
}

#[test]
fn key_ignores_key_order_whitespace_and_fresh() {
    let a: Value = serde_json::from_str(r#"{"symbol":"x","file":"a.rs","depth":2}"#).unwrap();
    let b: Value = serde_json::from_str("{ \"depth\" : 2,\n \"file\":\"a.rs\", \"symbol\":\"x\" }").unwrap();
    let c = json!({"symbol":"x","file":"a.rs","depth":2,"fresh":true});
    assert_eq!(reuse_key("c1", "ax_node", &a), reuse_key("c1", "ax_node", &b));
    assert_eq!(reuse_key("c1", "ax_node", &a), reuse_key("c1", "ax_node", &c));
    assert!(!reuse_key("c1", "ax_node", &a).is_empty());
}

#[test]
fn key_differs_for_conversation_tool_and_any_argument() {
    let a = json!({"symbol":"x"});
    let base = reuse_key("c1", "ax_node", &a);
    assert_ne!(base, reuse_key("c2", "ax_node", &a));
    assert_ne!(base, reuse_key("c1", "ax_search", &a));
    assert_ne!(base, reuse_key("c1", "ax_node", &json!({"symbol":"y"})));
    assert_ne!(base, reuse_key("c1", "ax_node", &json!({"symbol":"x","limit":5})));
    assert_ne!(base, reuse_key("c1", "ax_node", &json!({"symbol":["x"]})));
    assert_ne!(
        reuse_key("c1", "ax_node", &json!({"q":"1"})),
        reuse_key("c1", "ax_node", &json!({"q":1}))
    );
    // No separator ambiguity between conversation and tool.
    assert_ne!(reuse_key("ab", "c", &a), reuse_key("a", "bc", &a));
}

/// Tiny deterministic generator; no new dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn value(&mut self, depth: u32) -> Value {
        match self.next() % if depth > 2 { 4 } else { 6 } {
            0 => json!(self.next() % 5),
            1 => json!(format!("s{}", self.next() % 5)),
            2 => json!(self.next() % 2 == 0),
            3 => Value::Null,
            4 => Value::Array((0..self.next() % 3).map(|_| self.value(depth + 1)).collect()),
            _ => {
                let mut m = serde_json::Map::new();
                for _ in 0..self.next() % 4 {
                    m.insert(format!("k{}", self.next() % 4), self.value(depth + 1));
                }
                Value::Object(m)
            }
        }
    }
}

fn reorder(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().cloned().collect();
            keys.reverse();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), reorder(&m[&k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(reorder).collect()),
        other => other.clone(),
    }
}

#[test]
fn property_key_equal_exactly_when_canonical_equal() {
    let mut g = Lcg(42);
    let samples: Vec<Value> = (0..400).map(|_| g.value(0)).collect();
    for (i, a) in samples.iter().enumerate() {
        assert_eq!(
            reuse_key("c", "ax_node", a),
            reuse_key("c", "ax_node", &reorder(a)),
            "reordering changed the key for {a}"
        );
        for b in samples.iter().skip(i + 1).take(25) {
            let same_canon = canonical_args(a) == canonical_args(b);
            let same_key = reuse_key("c", "ax_node", a) == reuse_key("c", "ax_node", b);
            assert_eq!(same_canon, same_key, "{a} vs {b}");
            let a_no_fresh = strip_fresh(a);
            let b_no_fresh = strip_fresh(b);
            assert_eq!(same_canon, a_no_fresh == b_no_fresh, "canonical form lost information: {a} vs {b}");
        }
    }
}

fn strip_fresh(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(m) = v.as_object_mut() {
        m.remove("fresh");
    }
    v
}

#[test]
fn fresh_flag_is_read_only_when_true() {
    assert!(wants_fresh(&json!({"fresh": true})));
    assert!(!wants_fresh(&json!({"fresh": false})));
    assert!(!wants_fresh(&json!({"fresh": "yes"})));
    assert!(!wants_fresh(&json!({})));
    assert!(!wants_fresh(&json!([])));
}

#[test]
fn cited_files_finds_existing_paths_only() {
    let dir = std::env::temp_dir().join(format!("ax-reuse-cite-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "x").unwrap();
    std::fs::write(dir.join("src/b.rs"), "x").unwrap();
    let body = "## Entry 1: src/lib.rs::alpha (Method) — src/lib.rs:1-9\n\
                - src/b.rs::beta @ src/b.rs:3-4 (Method)\n\
                - missing/nope.rs::gamma @ missing/nope.rs:1-2\n\
                (`src/lib.rs:7`) and 12:30 and http://x.io:80";
    let files = cited_files(body, &dir);
    assert_eq!(files, vec!["src/b.rs".to_string(), "src/lib.rs".to_string()]);
    assert!(cited_files("no paths here", &dir).is_empty());
    assert!(cited_files("../../etc/passwd:1", &dir).is_empty(), "paths escaping the root are ignored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hit_text_is_short_and_names_the_id_and_turn() {
    let hit = ReuseHit {
        key: "k".into(),
        id: "cc_0123".into(),
        tool: "ax_node".into(),
        turn: 3,
        original_tokens: 4_000,
    };
    let text = render_hit(&hit);
    assert!(text.starts_with("[ax cache hit]"), "{text}");
    assert!(text.contains("cc_0123"));
    assert!(text.contains("turn=3"));
    assert!(text.contains("original_tokens=4000"));
    assert!(text.contains("ax_expand"));
    assert!(text.contains("fresh: true"));
    assert!(count_tokens(&text) < 100, "hit text costs {} tokens", count_tokens(&text));
}

#[test]
fn session_context_lists_entries_within_cap() {
    let entries: Vec<ContextEntry> = (0..200)
        .map(|i| ContextEntry {
            tool: "ax_node".into(),
            args_summary: format!("{{\"symbol\":\"sym_{i}\"}}"),
            files: vec![format!("src/f{i}.rs")],
            id: format!("cc_{i:04}"),
        })
        .collect();
    let text = format_session_context(&entries, 1_500);
    assert!(text.starts_with("<ax_session_context"), "{text}");
    assert!(text.trim_end().ends_with("</ax_session_context>"));
    assert!(count_tokens(&text) <= 1_500, "{} tokens", count_tokens(&text));
    assert!(text.contains("cc_0000"));
    assert!(text.contains("src/f0.rs"));
    let lines = text.lines().filter(|l| l.starts_with("- ")).count();
    assert!(lines > 5 && lines < 200, "lines={lines}");
    assert!(format_session_context(&[], 1_500).is_empty());
}

#[test]
fn conversation_key_prefers_active_session() {
    assert_eq!(conversation_key(Some("chat-1".into())), "chat-1");
    let a = conversation_key(None);
    let b = conversation_key(Some(String::new()));
    assert!(a.starts_with("proc-"), "{a}");
    assert_eq!(a, b, "process fallback is stable within one process");
}

#[test]
fn cap_defaults_to_two_megabytes() {
    assert_eq!(reuse_cap_bytes(), 2 * 1024 * 1024);
}

// ---------- L2: store ----------

#[tokio::test]
async fn store_then_lookup_hits_and_expand_returns_the_body() {
    let f = Fixture::new().await;
    let body = node_body();
    let id = f.store("c1", "ax_node", &args(), &body).await.expect("stored");
    let hit = f.lookup("c1", "ax_node", &args()).await.expect("hit");
    assert_eq!(hit.id, id);
    assert_eq!(hit.original_tokens, count_tokens(&body) as i64);
    let loaded = crate::context_cache::load_body(&f.pool, &id).await.unwrap();
    assert_eq!(loaded, body, "expand must be byte-identical");
}

#[tokio::test]
async fn other_conversation_misses() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    assert!(f.lookup("c2", "ax_node", &args()).await.is_none());
}

#[tokio::test]
async fn different_args_or_tool_miss() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    assert!(f.lookup("c1", "ax_node", &json!({"symbol":"beta"})).await.is_none());
    assert!(f.lookup("c1", "ax_callers", &args()).await.is_none());
}

#[tokio::test]
async fn exempt_tools_are_never_stored() {
    let f = Fixture::new().await;
    for tool in ["ax_preflight", "ax_guard", "ax_sync", "ax_remember", "ax_policy_capture", "ax_expand"] {
        assert!(f.store("c1", tool, &args(), &node_body()).await.is_none(), "{tool} was stored");
        assert!(f.lookup("c1", tool, &args()).await.is_none());
    }
}

#[tokio::test]
async fn fresh_skips_lookup() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    let mut fresh = args();
    fresh["fresh"] = json!(true);
    assert!(f.lookup("c1", "ax_node", &fresh).await.is_none());
}

#[tokio::test]
async fn reply_without_cited_files_is_not_cached() {
    let f = Fixture::new().await;
    assert!(f.store("c1", "ax_search", &args(), "no results").await.is_none());
    assert!(f.lookup("c1", "ax_search", &args()).await.is_none());
}

#[tokio::test]
async fn turn_counts_preflights_of_the_conversation() {
    let f = Fixture::new().await;
    for conv in ["c1", "c1", "c2"] {
        sqlx::query(
            "INSERT INTO mcp_session_index (session_id, tool, summary, original_tokens, created_at)
             VALUES (?, 'ax_preflight', '', 1, 0)",
        )
        .bind(conv)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    assert_eq!(f.lookup("c1", "ax_node", &args()).await.unwrap().turn, 2);
}

#[tokio::test]
async fn record_hit_accumulates_tokens_avoided() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    let hit = f.lookup("c1", "ax_node", &args()).await.unwrap();
    record_hit(&f.pool, &hit.key, 100).await.unwrap();
    record_hit(&f.pool, &hit.key, 50).await.unwrap();
    let (hits, avoided): (i64, i64) =
        sqlx::query_as("SELECT hits, tokens_avoided FROM mcp_reuse_cache WHERE reuse_key = ?")
            .bind(&hit.key)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!((hits, avoided), (2, 150));
}

// ---------- L3: invalidation and robustness ----------

#[tokio::test]
async fn edited_file_invalidates_without_sync() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    std::fs::write(f.root().join("src/lib.rs"), "fn alpha() { changed() }\n").unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_none());
}

#[tokio::test]
async fn touch_without_content_change_still_hits() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(f.root().join("src/lib.rs"), "fn alpha() {}\n").unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_some());
}

#[tokio::test]
async fn deleted_file_invalidates() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    std::fs::remove_file(f.root().join("src/lib.rs")).unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_none());
}

#[tokio::test]
async fn edit_to_an_uncited_file_keeps_the_hit() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    std::fs::write(f.root().join("src/other.rs"), "fn beta() { changed() }\n").unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_some());
}

#[tokio::test]
async fn restore_after_miss_stores_the_new_body() {
    let f = Fixture::new().await;
    let old = f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    std::fs::write(f.root().join("src/lib.rs"), "fn alpha() { v2() }\n").unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_none());
    let new_body = format!("{}v2\n", node_body());
    let new = f.store("c1", "ax_node", &args(), &new_body).await.unwrap();
    assert_ne!(old, new);
    assert_eq!(f.lookup("c1", "ax_node", &args()).await.unwrap().id, new);
}

#[test]
fn files_fresh_fails_closed() {
    let dir = std::env::temp_dir().join(format!("ax-reuse-fresh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "a").unwrap();
    let snap = snapshot_files(&dir, &["a.rs".to_string()]).unwrap();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(files_fresh(&dir, &json));
    assert!(!files_fresh(&dir, "not json"));
    assert!(!files_fresh(&dir, "[]"), "nothing to verify against is not fresh");
    assert!(!files_fresh(&dir, r#"[["a.rs","0000"]]"#));
    assert!(!files_fresh(&dir, r#"[["../a.rs","x"]]"#));
    assert!(snapshot_files(&dir, &["missing.rs".to_string()]).is_none());
    assert!(snapshot_files(&dir, &[]).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn corrupt_rows_are_a_miss_not_an_error() {
    let f = Fixture::new().await;
    let id = f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    sqlx::query("UPDATE mcp_reuse_cache SET files_json = '{broken'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.lookup("c1", "ax_node", &args()).await.is_none());

    let f2 = Fixture::new().await;
    let id2 = f2.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    sqlx::query("DELETE FROM mcp_context_cache WHERE id = ?")
        .bind(&id2)
        .execute(&f2.pool)
        .await
        .unwrap();
    assert!(f2.lookup("c1", "ax_node", &args()).await.is_none(), "missing body must miss");
    assert!(!id.is_empty());
}

#[tokio::test]
async fn closed_pool_is_a_miss() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    f.pool.close().await;
    assert!(f.lookup("c1", "ax_node", &args()).await.is_none());
}

#[tokio::test]
async fn size_cap_evicts_oldest_first_and_never_exceeds_cap() {
    let f = Fixture::new().await;
    let cap = 20_000_i64;
    let mut ids = Vec::new();
    for i in 0..30 {
        let body = format!("{}entry {i}\n", node_body());
        let a = json!({"symbol": format!("s{i}")});
        let id = store_reply(&f.pool, f.root(), "c1", "ax_node", &a, &body, cap)
            .await
            .unwrap()
            .unwrap();
        ids.push(id);
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(body_bytes), 0) FROM mcp_reuse_cache WHERE conversation = 'c1'",
        )
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert!(total <= cap, "total {total} exceeds cap after insert {i}");
    }
    assert!(f.lookup("c1", "ax_node", &json!({"symbol":"s29"})).await.is_some(), "newest kept");
    assert!(f.lookup("c1", "ax_node", &json!({"symbol":"s0"})).await.is_none(), "oldest evicted");
    store_reply(&f.pool, f.root(), "c2", "ax_node", &args(), &node_body(), cap)
        .await
        .unwrap();
    assert!(f.lookup("c1", "ax_node", &json!({"symbol":"s29"})).await.is_some(), "cap is per conversation");
}

#[tokio::test]
async fn body_larger_than_cap_is_not_stored() {
    let f = Fixture::new().await;
    let stored = store_reply(&f.pool, f.root(), "c1", "ax_node", &args(), &node_body(), 10)
        .await
        .unwrap();
    assert!(stored.is_none());
}

#[tokio::test]
async fn concurrent_identical_stores_leave_one_intact_row() {
    let f = Fixture::new().await;
    let pool = f.pool.clone();
    let root = f.root().to_path_buf();
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        let root = root.clone();
        tasks.push(tokio::spawn(async move {
            store_reply(&pool, &root, "c1", "ax_node", &args(), &node_body(), 2_000_000).await
        }));
    }
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    let (rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mcp_reuse_cache")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    let hit = f.lookup("c1", "ax_node", &args()).await.unwrap();
    let body = crate::context_cache::load_body(&f.pool, &hit.id).await.unwrap();
    assert_eq!(body, node_body());
}

#[tokio::test]
async fn session_context_lists_only_fresh_entries_of_this_conversation() {
    let f = Fixture::new().await;
    f.store("c1", "ax_node", &args(), &node_body()).await.unwrap();
    let other_body = "## Entry: src/other.rs::beta — src/other.rs:1-1\n".repeat(5);
    f.store("c1", "ax_node", &json!({"symbol":"beta"}), &other_body).await.unwrap();
    f.store("c2", "ax_node", &json!({"symbol":"zeta"}), &node_body()).await.unwrap();
    std::fs::write(f.root().join("src/other.rs"), "fn beta() { changed() }\n").unwrap();
    let text = session_context(&f.pool, f.root(), "c1", 1_500).await.expect("block");
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("src/lib.rs"));
    assert!(!text.contains("beta"), "stale entry listed: {text}");
    assert!(!text.contains("zeta"), "other conversation listed: {text}");
    assert!(session_context(&f.pool, f.root(), "c9", 1_500).await.is_none());
}
