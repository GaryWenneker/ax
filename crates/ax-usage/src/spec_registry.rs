//! Versioned spec registry. The project database is the authority for identity
//! and history. The Markdown file is the readable copy of the approved body.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Registry tables; the shared `specs` and `spec_revisions` tables come from
/// `spec_hierarchy::SPEC_TABLES`.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS spec_bindings (
    session_id TEXT PRIMARY KEY,
    spec_id TEXT NOT NULL
);
-- Sessions whose binding came from a scanned plan file, not from a capture
-- or edit in that session. Only these bindings are moved by a later scan.
CREATE TABLE IF NOT EXISTS spec_implicit_bindings (
    session_id TEXT PRIMARY KEY
);
-- Plan files already imported, so an unchanged file is not read again.
CREATE TABLE IF NOT EXISTS spec_plan_files (
    path TEXT PRIMARY KEY,
    modified INTEGER NOT NULL,
    size INTEGER NOT NULL,
    spec_id TEXT,
    stored INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS spec_deliveries (
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    spec_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    PRIMARY KEY (session_id, turn_id)
);
CREATE TABLE IF NOT EXISTS spec_file_seals (
    spec_id TEXT NOT NULL,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    sealed_version INTEGER NOT NULL,
    PRIMARY KEY (spec_id, path)
);
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecRevision {
    pub spec_id: String,
    pub version: i64,
    pub status: String,
    pub body: String,
    pub content_hash: String,
}

/// A mention of the word plan is not a request. These phrases are.
pub fn is_plan_request(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    [
        "make a plan",
        "maak een plan",
        "write a plan",
        "create a plan",
        "draft a plan",
        "nieuwe spec",
        "write a spec",
        "maak een spec",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

static REGISTRY_MIGRATED: crate::spec_hierarchy::MigratedPools =
    crate::spec_hierarchy::MigratedPools::new();

/// Create the registry tables. The work runs once per pool.
pub async fn ensure_spec_schema(pool: &SqlitePool) -> Result<(), String> {
    if REGISTRY_MIGRATED.contains(pool) {
        return Ok(());
    }
    crate::spec_hierarchy::run_schema(pool, crate::spec_hierarchy::SPEC_TABLES).await?;
    crate::spec_hierarchy::run_schema(pool, SCHEMA).await?;
    REGISTRY_MIGRATED.insert(pool);
    Ok(())
}

/// Store a finished plan. A partial stream (`finished == false`) and an empty
/// body write nothing. A turn that did not ask for a plan writes nothing.
#[allow(clippy::too_many_arguments)]
pub async fn capture_finished_plan(
    pool: &SqlitePool,
    project_id: &str,
    spec_id: &str,
    title: &str,
    prompt: &str,
    body: &str,
    session_id: &str,
    turn_id: &str,
    source: &str,
    finished: bool,
) -> Result<Option<SpecRevision>, String> {
    if !finished || body.trim().is_empty() {
        return Ok(None);
    }
    let body = stored_body(body);
    if source == "turn" && !is_plan_request(prompt) {
        return Ok(None);
    }
    ensure_spec_schema(pool).await?;
    if crate::spec_hierarchy::hierarchy_enabled(pool)
        .await
        .unwrap_or(false)
    {
        crate::spec_hierarchy::queue_finished_plan(
            pool, project_id, spec_id, title, body, session_id, turn_id, source,
        )
        .await?;
        return Ok(None);
    }
    // A changed spec file only binds a session that has no spec yet.
    let bind = |pool, session_id, spec_id| async move {
        if source == "file" {
            bind_session_if_unbound(pool, session_id, spec_id).await
        } else {
            bind_session(pool, session_id, spec_id).await
        }
    };
    if let Some(existing) = revision_for_turn(pool, spec_id, session_id, turn_id).await? {
        bind(pool, session_id, spec_id).await?;
        return Ok(Some(existing));
    }
    // The same body as the latest revision is not a new draft.
    if let Some(latest) = latest_revision(pool, spec_id).await? {
        if latest.content_hash == hash_text(body) {
            bind(pool, session_id, spec_id).await?;
            return Ok(Some(latest));
        }
    }
    let markdown_path = format!("docs/specs/{spec_id}/SPEC.md");
    sqlx::query(
        "INSERT INTO specs (id, project_id, title, markdown_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(spec_id)
    .bind(project_id)
    .bind(title)
    .bind(&markdown_path)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    let revision = insert_revision(
        pool, spec_id, "draft", body, source, session_id, turn_id, None,
    )
    .await?;
    bind(pool, session_id, spec_id).await?;
    Ok(Some(revision))
}

#[allow(clippy::too_many_arguments)]
pub async fn on_turn_finished(
    pool: &SqlitePool,
    root: &Path,
    project_id: &str,
    session_id: &str,
    turn_id: &str,
    prompt: &str,
    outcome: &str,
    files: &[String],
) -> Result<(), String> {
    let spec_id = slug(prompt);
    let _ = capture_finished_plan(
        pool, project_id, &spec_id, prompt, prompt, outcome, session_id, turn_id, "turn", true,
    )
    .await?;
    for file in files {
        // The Markdown mirror of an existing spec is checked by `sync_markdown`.
        if is_spec_markdown(file) && spec_for_markdown(pool, file).await?.is_none() {
            if let Ok(body) = std::fs::read_to_string(root.join(file)) {
                let _ = capture_finished_plan(
                    pool,
                    project_id,
                    &markdown_spec_id(file),
                    file,
                    file,
                    &body,
                    session_id,
                    turn_id,
                    "file",
                    true,
                )
                .await?;
            }
        }
    }
    if let Some(spec_id) = bound_spec(pool, session_id).await? {
        let _ = check_linked_files(pool, root, &spec_id, session_id, turn_id).await?;
        let _ = sync_markdown(pool, root, &spec_id).await?;
        if let Some(approved) = approved_revision(pool, &spec_id).await? {
            record_delivery(
                pool,
                session_id,
                turn_id,
                &spec_id,
                approved.version,
                &approved.content_hash,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn approve_revision(
    pool: &SqlitePool,
    root: &Path,
    spec_id: &str,
    version: i64,
    linked_files: &[String],
) -> Result<SpecRevision, String> {
    ensure_spec_schema(pool).await?;
    let revision = revision(pool, spec_id, version)
        .await?
        .ok_or_else(|| format!("spec {spec_id} has no version {version}"))?;
    let path = markdown_path(pool, spec_id).await?;
    // Seal each path once, hashed before anything is written.
    let mut seals: Vec<String> = Vec::new();
    for linked in linked_files {
        if !seals.contains(linked) {
            seals.push(linked.clone());
        }
    }
    for named in paths_named_in(&revision.body) {
        if seals.contains(&named) {
            continue;
        }
        if named.split('/').count() < 2 || named.contains("..") {
            continue;
        }
        if root.join(&named).is_file() {
            seals.push(named);
        }
    }
    let sealed: Vec<(String, String)> = seals
        .into_iter()
        .map(|linked| {
            let hash = std::fs::read(root.join(&linked))
                .map(|bytes| hash_bytes(&bytes))
                .unwrap_or_else(|_| "missing".into());
            (linked, hash)
        })
        .collect();
    // All database changes commit together, and only after the Markdown copy
    // was replaced; a failure at any step leaves the previous approval as is.
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE spec_revisions SET status = 'superseded'
         WHERE spec_id = ?1 AND status = 'approved' AND version != ?2",
    )
    .bind(spec_id)
    .bind(version)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE spec_revisions SET status = 'approved' WHERE spec_id = ?1 AND version = ?2",
    )
    .bind(spec_id)
    .bind(version)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM spec_file_seals WHERE spec_id = ?1")
        .bind(spec_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for (linked, hash) in &sealed {
        sqlx::query(
            "INSERT INTO spec_file_seals (spec_id, path, content_hash, sealed_version)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(spec_id)
        .bind(linked)
        .bind(hash)
        .bind(version)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    ax_utils::atomic_write(&root.join(&path), format!("{}\n", revision.body).as_bytes())
        .map_err(|error| format!("write {path}: {error}"))?;
    tx.commit().await.map_err(|error| error.to_string())?;
    revision_row(pool, spec_id, version).await
}

/// Linked-file bytes changed since approval. The approved body stays. A
/// `change_requested` draft records the discrepancy once per new hash.
pub async fn check_linked_files(
    pool: &SqlitePool,
    root: &Path,
    spec_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<bool, String> {
    ensure_spec_schema(pool).await?;
    let seals: Vec<(String, String)> =
        sqlx::query_as("SELECT path, content_hash FROM spec_file_seals WHERE spec_id = ?1")
            .bind(spec_id)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?;
    let mut drifted = Vec::new();
    for (path, sealed) in seals {
        let current = std::fs::read(root.join(&path))
            .map(|bytes| hash_bytes(&bytes))
            .unwrap_or_else(|_| "missing".into());
        if current != sealed {
            drifted.push(format!("{path} {sealed} -> {current}"));
        }
    }
    if drifted.is_empty() {
        return Ok(false);
    }
    let note = format!(
        "Linked files changed since the approved spec. The approved body was not rewritten.\n{}",
        drifted.join("\n")
    );
    let approved = approved_revision(pool, spec_id).await?;
    insert_revision_if_new(
        pool,
        spec_id,
        "change_requested",
        &note,
        "drift",
        session_id,
        turn_id,
        approved.as_ref().map(|item| item.version),
    )
    .await?;
    Ok(true)
}

/// The Markdown file and the approved revision disagree. Neither replaces the other.
pub async fn sync_markdown(
    pool: &SqlitePool,
    root: &Path,
    spec_id: &str,
) -> Result<Option<(String, String)>, String> {
    ensure_spec_schema(pool).await?;
    let Some(approved) = approved_revision(pool, spec_id).await? else {
        return Ok(None);
    };
    let path = root.join(markdown_path(pool, spec_id).await?);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let file_body = stored_body(&text);
    let file_hash = hash_text(file_body);
    // Older rows kept the trailing newline in the body; compare without it.
    if file_hash == approved.content_hash || file_body == stored_body(&approved.body) {
        return Ok(None);
    }
    insert_revision_if_new(
        pool,
        spec_id,
        "draft",
        file_body,
        "file",
        "",
        "",
        Some(approved.version),
    )
    .await?;
    Ok(Some((approved.content_hash, file_hash)))
}

pub async fn spec_context_block(pool: &SqlitePool, root: &Path, session_id: &str) -> String {
    let Ok(()) = ensure_spec_schema(pool).await else {
        return "none".into();
    };
    let _ = import_workspace_plans(pool, root, "local").await;
    let spec_id = match bound_spec(pool, session_id).await {
        Ok(Some(spec_id)) => spec_id,
        Ok(None) => match live_bound_spec(pool, root).await {
            Some(spec_id) => spec_id,
            None => return "none".into(),
        },
        Err(_) => return "none".into(),
    };
    let Ok(Some(approved)) = approved_revision(pool, spec_id.as_str()).await else {
        return format!("spec: {spec_id}\napproved: none\npointer: ax spec show {spec_id}");
    };
    let conflict = sync_markdown(pool, root, &spec_id).await.ok().flatten();
    // A failed check is not evidence that nothing drifted.
    let drift = match check_linked_files(pool, root, &spec_id, session_id, "preflight").await {
        Ok(true) => "yes",
        Ok(false) => "no",
        Err(_) => "unknown",
    };
    let mut lines = vec![
        format!("spec: {spec_id}"),
        format!("approved: v{}", approved.version),
        format!("hash: {}", approved.content_hash),
        format!("objective: {}", first_line(&approved.body)),
        format!("drift: {drift}"),
        format!("pointer: ax spec show {spec_id}@{}", approved.version),
    ];
    if let Some((approved_hash, file_hash)) = conflict {
        lines.push("delivery: conflict".into());
        lines.push(format!("approvedHash: {approved_hash}"));
        lines.push(format!("fileHash: {file_hash}"));
    }
    lines.join("\n")
}

pub async fn record_delivery(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    spec_id: &str,
    version: i64,
    content_hash: &str,
) -> Result<(), String> {
    ensure_spec_schema(pool).await?;
    sqlx::query(
        "INSERT INTO spec_deliveries (session_id, turn_id, spec_id, version, content_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id, turn_id) DO UPDATE SET
            spec_id = excluded.spec_id,
            version = excluded.version,
            content_hash = excluded.content_hash",
    )
    .bind(session_id)
    .bind(turn_id)
    .bind(spec_id)
    .bind(version)
    .bind(content_hash)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn delivered_version(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
) -> Result<Option<i64>, String> {
    ensure_spec_schema(pool).await?;
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM spec_deliveries WHERE session_id = ?1 AND turn_id = ?2",
    )
    .bind(session_id)
    .bind(turn_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(version)
}

pub async fn revision(
    pool: &SqlitePool,
    spec_id: &str,
    version: i64,
) -> Result<Option<SpecRevision>, String> {
    ensure_spec_schema(pool).await?;
    revision_optional(pool, spec_id, version).await
}

async fn revision_row(
    pool: &SqlitePool,
    spec_id: &str,
    version: i64,
) -> Result<SpecRevision, String> {
    revision_optional(pool, spec_id, version)
        .await?
        .ok_or_else(|| format!("spec {spec_id} version {version} missing"))
}

async fn revision_optional(
    pool: &SqlitePool,
    spec_id: &str,
    version: i64,
) -> Result<Option<SpecRevision>, String> {
    let row: Option<(i64, String, String, String)> = sqlx::query_as(
        "SELECT version, status, body, content_hash FROM spec_revisions
         WHERE spec_id = ?1 AND version = ?2",
    )
    .bind(spec_id)
    .bind(version)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(
        row.map(|(version, status, body, content_hash)| SpecRevision {
            spec_id: spec_id.to_string(),
            version,
            status,
            body,
            content_hash,
        }),
    )
}

pub async fn approved_revision(
    pool: &SqlitePool,
    spec_id: &str,
) -> Result<Option<SpecRevision>, String> {
    ensure_spec_schema(pool).await?;
    let row: Option<(i64, String, String, String)> = sqlx::query_as(
        "SELECT version, status, body, content_hash FROM spec_revisions
         WHERE spec_id = ?1 AND status = 'approved'
         ORDER BY version DESC LIMIT 1",
    )
    .bind(spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(
        row.map(|(version, status, body, content_hash)| SpecRevision {
            spec_id: spec_id.to_string(),
            version,
            status,
            body,
            content_hash,
        }),
    )
}

async fn revision_for_turn(
    pool: &SqlitePool,
    spec_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<Option<SpecRevision>, String> {
    if turn_id.is_empty() {
        return Ok(None);
    }
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM spec_revisions
         WHERE spec_id = ?1 AND session_id = ?2 AND turn_id = ?3
         ORDER BY version DESC LIMIT 1",
    )
    .bind(spec_id)
    .bind(session_id)
    .bind(turn_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    match version {
        Some(version) => revision_optional(pool, spec_id, version).await,
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_revision(
    pool: &SqlitePool,
    spec_id: &str,
    status: &str,
    body: &str,
    source: &str,
    session_id: &str,
    turn_id: &str,
    based_on: Option<i64>,
) -> Result<SpecRevision, String> {
    insert_revision_where(
        pool, spec_id, status, body, source, session_id, turn_id, based_on, false,
    )
    .await?
    .ok_or_else(|| format!("spec {spec_id}: revision was not stored"))
}

/// Store a revision unless one with the same body already exists for the spec.
/// Returns `None` when such a revision exists.
#[allow(clippy::too_many_arguments)]
async fn insert_revision_if_new(
    pool: &SqlitePool,
    spec_id: &str,
    status: &str,
    body: &str,
    source: &str,
    session_id: &str,
    turn_id: &str,
    based_on: Option<i64>,
) -> Result<Option<SpecRevision>, String> {
    insert_revision_where(
        pool, spec_id, status, body, source, session_id, turn_id, based_on, true,
    )
    .await
}

/// One statement picks the next version and inserts the row, so concurrent
/// callers never compute the same version. With `only_new`, the same
/// statement also skips a body the spec already has.
#[allow(clippy::too_many_arguments)]
async fn insert_revision_where(
    pool: &SqlitePool,
    spec_id: &str,
    status: &str,
    body: &str,
    source: &str,
    session_id: &str,
    turn_id: &str,
    based_on: Option<i64>,
    only_new: bool,
) -> Result<Option<SpecRevision>, String> {
    let body = stored_body(body);
    let content_hash = hash_text(body);
    let version: Option<i64> = sqlx::query_scalar(
        "INSERT INTO spec_revisions
         (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
         SELECT ?1, COALESCE(MAX(version), 0) + 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
         FROM spec_revisions WHERE spec_id = ?1
         HAVING ?10 = 0 OR NOT EXISTS (
             SELECT 1 FROM spec_revisions WHERE spec_id = ?1 AND content_hash = ?4
         )
         RETURNING version",
    )
    .bind(spec_id)
    .bind(status)
    .bind(body)
    .bind(&content_hash)
    .bind(source)
    .bind(session_id)
    .bind(turn_id)
    .bind(based_on)
    .bind(now_ms())
    .bind(only_new)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(version.map(|version| SpecRevision {
        spec_id: spec_id.to_string(),
        version,
        status: status.to_string(),
        body: body.to_string(),
        content_hash,
    }))
}

async fn bind_session(pool: &SqlitePool, session_id: &str, spec_id: &str) -> Result<(), String> {
    if session_id.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO spec_bindings (session_id, spec_id) VALUES (?1, ?2)
         ON CONFLICT(session_id) DO UPDATE SET spec_id = excluded.spec_id",
    )
    .bind(session_id)
    .bind(spec_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM spec_implicit_bindings WHERE session_id = ?1")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Bind a session found in a scanned plan file. A session with no binding, or
/// one that an earlier scan bound, follows the plan. A binding made by a
/// capture or an edit in the session itself is kept.
async fn bind_session_implicit(
    pool: &SqlitePool,
    session_id: &str,
    spec_id: &str,
) -> Result<(), String> {
    if session_id.is_empty() {
        return Ok(());
    }
    match bound_spec(pool, session_id).await? {
        Some(current) if current == spec_id => return Ok(()),
        Some(_) => {
            let implicit: Option<String> = sqlx::query_scalar(
                "SELECT session_id FROM spec_implicit_bindings WHERE session_id = ?1",
            )
            .bind(session_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
            if implicit.is_none() {
                return Ok(());
            }
        }
        None => {}
    }
    sqlx::query(
        "INSERT INTO spec_bindings (session_id, spec_id) VALUES (?1, ?2)
         ON CONFLICT(session_id) DO UPDATE SET spec_id = excluded.spec_id",
    )
    .bind(session_id)
    .bind(spec_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("INSERT OR IGNORE INTO spec_implicit_bindings (session_id) VALUES (?1)")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn bind_session_if_unbound(
    pool: &SqlitePool,
    session_id: &str,
    spec_id: &str,
) -> Result<(), String> {
    if session_id.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO spec_bindings (session_id, spec_id) VALUES (?1, ?2)
         ON CONFLICT(session_id) DO NOTHING",
    )
    .bind(session_id)
    .bind(spec_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn latest_revision(pool: &SqlitePool, spec_id: &str) -> Result<Option<SpecRevision>, String> {
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM spec_revisions WHERE spec_id = ?1 ORDER BY version DESC LIMIT 1",
    )
    .bind(spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    match version {
        Some(version) => revision_optional(pool, spec_id, version).await,
        None => Ok(None),
    }
}

/// The spec whose Markdown mirror is this project-relative path.
async fn spec_for_markdown(pool: &SqlitePool, file: &str) -> Result<Option<String>, String> {
    ensure_spec_schema(pool).await?;
    sqlx::query_scalar("SELECT id FROM specs WHERE markdown_path = ?1 LIMIT 1")
        .bind(file.replace('\\', "/"))
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())
}

/// Spec id for a changed spec file: the folder name for `docs/specs/<id>/SPEC.md`,
/// else a slug of the whole path.
fn markdown_spec_id(file: &str) -> String {
    let path = file.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["docs", "specs", dir, name] if name.eq_ignore_ascii_case("SPEC.md") => {
            let id = slug(dir);
            if id.is_empty() {
                slug(file)
            } else {
                id
            }
        }
        _ => slug(file),
    }
}

async fn bound_spec(pool: &SqlitePool, session_id: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar("SELECT spec_id FROM spec_bindings WHERE session_id = ?1")
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())
}

async fn markdown_path(pool: &SqlitePool, spec_id: &str) -> Result<String, String> {
    sqlx::query_scalar("SELECT markdown_path FROM specs WHERE id = ?1")
        .bind(spec_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())
}

fn is_spec_markdown(path: &str) -> bool {
    let path = path.replace('\\', "/");
    (path.starts_with("docs/specs/") || path.starts_with("docs/plans/")) && path.ends_with(".md")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecListItem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub markdown_path: String,
    pub sessions: String,
    pub approved_version: Option<i64>,
    pub latest_status: String,
    pub revision_count: i64,
    pub kind: String,
    pub pointer_version: Option<i64>,
    pub pointer_hash: Option<String>,
    pub updated_at: i64,
    pub latest_version: i64,
    pub latest_source: String,
    pub latest_based_on: Option<i64>,
    /// `plan` when any revision came from plan mode; `kind` can differ after an owner classification.
    pub detected_kind: String,
    pub plans: Vec<crate::spec_documents::ListedPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecPointer {
    pub id: String,
    pub version: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecHistoryRow {
    pub version: i64,
    pub status: String,
    pub source: String,
    pub body: String,
    pub content_hash: String,
    pub based_on: Option<i64>,
    pub created_at: i64,
}

async fn pointer_pair(pool: &SqlitePool, spec_id: &str) -> Result<Option<(i64, String)>, String> {
    let approved: Option<(i64, String)> = sqlx::query_as(
        "SELECT version, content_hash FROM spec_revisions
         WHERE spec_id = ?1 AND status = 'approved'
         ORDER BY version DESC LIMIT 1",
    )
    .bind(spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    if approved.is_some() {
        return Ok(approved);
    }
    sqlx::query_as(
        "SELECT version, content_hash FROM spec_revisions
         WHERE spec_id = ?1 ORDER BY version DESC LIMIT 1",
    )
    .bind(spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn spec_pointer_for(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<SpecPointer>, String> {
    ensure_spec_schema(pool).await?;
    let Some(spec_id) = bound_spec(pool, session_id).await? else {
        return Ok(None);
    };
    let Some((version, content_hash)) = pointer_pair(pool, &spec_id).await? else {
        return Ok(None);
    };
    Ok(Some(SpecPointer {
        id: spec_id,
        version,
        content_hash,
    }))
}

pub async fn show_spec(pool: &SqlitePool, spec_ref: &str) -> Result<String, String> {
    ensure_spec_schema(pool).await?;
    let (spec_id, requested) = split_spec_ref(spec_ref)?;
    let version = match requested {
        Some(version) => version,
        None => pointer_pair(pool, &spec_id)
            .await?
            .map(|(version, _)| version)
            .ok_or_else(|| format!("spec {spec_id} was not found"))?,
    };
    revision(pool, &spec_id, version)
        .await?
        .map(|row| row.body)
        .ok_or_else(|| format!("spec {spec_id} has no version {version}"))
}

fn split_spec_ref(spec_ref: &str) -> Result<(String, Option<i64>), String> {
    let spec_ref = spec_ref.trim();
    if spec_ref.is_empty() {
        return Err("spec id is required".into());
    }
    if let Some((spec_id, version)) = spec_ref.rsplit_once('@') {
        if spec_id.is_empty() {
            return Err("spec id is required".into());
        }
        let version = version
            .parse::<i64>()
            .map_err(|_| format!("bad spec version in {spec_ref}"))?;
        return Ok((spec_id.to_string(), Some(version)));
    }
    Ok((spec_ref.to_string(), None))
}

pub async fn list_specs(pool: &SqlitePool) -> Result<Vec<SpecListItem>, String> {
    ensure_spec_schema(pool).await?;
    let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, project_id, title, markdown_path, updated_at FROM (
             SELECT id, project_id, title, markdown_path,
                    COALESCE(
                        (SELECT MAX(created_at) FROM spec_revisions WHERE spec_id = specs.id),
                        created_at
                    ) AS updated_at
             FROM specs
         )
         ORDER BY updated_at DESC, id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let types = crate::spec_documents::document_types(pool).await?;
    let mut plans = crate::spec_documents::plans_by_spec(pool).await?;
    let mut items = Vec::new();
    for (id, project_id, title, markdown_path, updated_at) in rows {
        let approved_version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM spec_revisions
             WHERE spec_id = ?1 AND status = 'approved'
             ORDER BY version DESC LIMIT 1",
        )
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
        let latest: Option<(i64, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT version, status, source, based_on
             FROM spec_revisions WHERE spec_id = ?1
             ORDER BY version DESC LIMIT 1",
        )
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
        let (latest_version, latest_status, latest_source, latest_based_on) =
            latest.unwrap_or((0, "none".into(), String::new(), None));
        let revision_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM spec_revisions WHERE spec_id = ?1")
                .bind(&id)
                .fetch_one(pool)
                .await
                .map_err(|error| error.to_string())?;
        let session_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT session_id FROM spec_bindings WHERE spec_id = ?1 ORDER BY session_id",
        )
        .bind(&id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
        let mut session_ids = session_rows
            .into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>();
        if session_ids.is_empty() {
            let fallback = format!("plan-{id}");
            bind_session(pool, &fallback, &id).await?;
            session_ids.push(fallback);
        }
        let sessions = session_ids.join(" ");
        let pointer = pointer_pair(pool, &id).await?;
        let plan_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM spec_revisions WHERE spec_id = ?1 AND source = 'plan_mode'",
        )
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
        let detected = if plan_count > 0 { "plan" } else { "spec" };
        let kind = types.get(&id).map(String::as_str).unwrap_or(detected);
        let spec_plans = plans.remove(&id).unwrap_or_default();
        items.push(SpecListItem {
            id,
            project_id,
            title,
            markdown_path,
            sessions,
            approved_version,
            latest_status,
            revision_count,
            kind: kind.into(),
            pointer_version: pointer.as_ref().map(|row| row.0),
            pointer_hash: pointer.map(|row| row.1),
            updated_at,
            latest_version,
            latest_source,
            latest_based_on,
            detected_kind: detected.into(),
            plans: spec_plans,
        });
    }
    Ok(items)
}

pub(crate) type RevisionColumns = (i64, String, String, String, String, Option<i64>, i64);

pub async fn list_revisions(
    pool: &SqlitePool,
    spec_id: &str,
) -> Result<Vec<SpecHistoryRow>, String> {
    ensure_spec_schema(pool).await?;
    let rows: Vec<RevisionColumns> = sqlx::query_as(
        "SELECT version, status, source, body, content_hash, based_on, created_at
         FROM spec_revisions WHERE spec_id = ?1 ORDER BY version",
    )
    .bind(spec_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(version, status, source, body, content_hash, based_on, created_at)| SpecHistoryRow {
                version,
                status,
                source,
                body,
                content_hash,
                based_on,
                created_at,
            },
        )
        .collect())
}

/// Append a draft from an edit. An empty body writes nothing. A body that
/// matches the selected revision returns that revision and does not append.
/// The selected row, any approved row, and the Markdown file are left as they are.
pub async fn save_revision(
    pool: &SqlitePool,
    spec_id: &str,
    based_on: i64,
    body: &str,
) -> Result<SpecRevision, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("empty body".into());
    }
    ensure_spec_schema(pool).await?;
    let selected = revision(pool, spec_id, based_on)
        .await?
        .ok_or_else(|| format!("spec {spec_id} has no version {based_on}"))?;
    if selected.body.trim() == trimmed {
        return Ok(selected);
    }
    insert_revision(
        pool,
        spec_id,
        "draft",
        trimmed,
        "edit",
        "editor",
        &format!("edit-v{based_on}"),
        Some(based_on),
    )
    .await
}

/// A Cursor plan file is finished when it has at least two non-empty lines.
/// A one-line fragment is not stored.
fn plan_file_is_finished(body: &str) -> bool {
    body.trim()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        >= 2
}

/// Store finished Markdown plans under one project root. A one-line fragment
/// is ignored. Nothing is approved and no Markdown copy is written.
pub async fn import_plan_mode_files(
    pool: &SqlitePool,
    root: &Path,
    project_id: &str,
) -> Result<usize, String> {
    let mut dirs = Vec::new();
    push_plan_dirs(root, PROJECT_PLAN_REL, &mut dirs);
    import_plan_dirs(pool, project_id, &dirs, None, &[]).await
}

/// Store plans from workspace members, recent projects, and `~/.cursor/plans`.
/// Home and the filesystem root are not scanned. The newest user plan is also
/// bound to the live session ids when those files exist.
pub async fn import_workspace_plans(
    pool: &SqlitePool,
    root: &Path,
    project_id: &str,
) -> Result<usize, String> {
    let recent = recent_project_paths();
    let (mut dirs, user_plans) =
        plan_directories(root, ax_utils::paths::home_dir().as_deref(), &recent);
    for extra in extra_user_plan_dirs() {
        if !dirs.iter().any(|dir| dir == &extra) {
            dirs.push(extra);
        }
    }
    let live = live_session_ids(root);
    import_plan_dirs(pool, project_id, &dirs, user_plans.as_deref(), &live).await
}

fn extra_user_plan_dirs() -> Vec<PathBuf> {
    let mut extra = Vec::new();
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let xdg = PathBuf::from(xdg.trim());
        if !xdg.as_os_str().is_empty() && !is_broad_root(&xdg) {
            extra.push(xdg.join("opencode").join("plans"));
        }
    }
    if let Ok(hermes) = std::env::var("HERMES_HOME") {
        let hermes = PathBuf::from(hermes.trim());
        if !hermes.as_os_str().is_empty() && !is_broad_root(&hermes) {
            extra.push(hermes.join("plans"));
        }
    }
    extra
}

const MAX_PLAN_FILES: usize = 400;
const SKIP_PLAN_DIRS: &[&str] = &["node_modules", "target", "target-dev", ".git"];

/// One Markdown file found in a plan folder. `body` is read only when the
/// file is new or its size or modification time changed since the last import.
struct PlanFile {
    path: PathBuf,
    modified: i64,
    size: i64,
    body: Option<String>,
}

/// What an earlier import recorded for a file: its size and modification
/// time, the spec it became (none for a fragment), and whether a revision was
/// stored for it.
struct KnownPlanFile {
    modified: i64,
    size: i64,
    spec_id: Option<String>,
    stored: bool,
}

fn modified_nanos(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Walk the plan folders and read the files that changed. Blocking file
/// access, so it runs on the blocking pool.
fn scan_plan_files(
    dirs: &[PathBuf],
    known: &std::collections::HashMap<String, (i64, i64)>,
) -> Vec<PlanFile> {
    let mut paths = Vec::new();
    for dir in dirs {
        collect_plan_markdown(dir, &mut paths, 0);
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            let modified = modified_nanos(&meta);
            let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
            let key = path.to_string_lossy().into_owned();
            let body = if known.get(&key) == Some(&(modified, size)) {
                None
            } else {
                Some(std::fs::read_to_string(&path).ok()?)
            };
            Some(PlanFile {
                path,
                modified,
                size,
                body,
            })
        })
        .collect()
}

async fn known_plan_files(
    pool: &SqlitePool,
) -> Result<std::collections::HashMap<String, KnownPlanFile>, String> {
    let rows: Vec<(String, i64, i64, Option<String>, i64)> =
        sqlx::query_as("SELECT path, modified, size, spec_id, stored FROM spec_plan_files")
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(path, modified, size, spec_id, stored)| {
            (
                path,
                KnownPlanFile {
                    modified,
                    size,
                    spec_id,
                    stored: stored != 0,
                },
            )
        })
        .collect())
}

async fn import_plan_dirs(
    pool: &SqlitePool,
    project_id: &str,
    dirs: &[PathBuf],
    user_plans_dir: Option<&Path>,
    live_sessions: &[String],
) -> Result<usize, String> {
    ensure_spec_schema(pool).await?;
    let known = known_plan_files(pool).await?;
    let stamps: std::collections::HashMap<String, (i64, i64)> = known
        .iter()
        .map(|(path, file)| (path.clone(), (file.modified, file.size)))
        .collect();
    let scan_dirs = dirs.to_vec();
    let files = tokio::task::spawn_blocking(move || scan_plan_files(&scan_dirs, &stamps))
        .await
        .map_err(|error| error.to_string())?;
    let mut stored = 0usize;
    // The newest finished file in the user plan folder, by modification time.
    let mut newest: Option<(i64, String)> = None;
    for file in &files {
        let key = file.path.to_string_lossy().into_owned();
        let (spec_id, was_stored) = match &file.body {
            None => {
                let Some(entry) = known.get(&key) else {
                    continue;
                };
                (entry.spec_id.clone(), entry.stored)
            }
            Some(body) => {
                let Some(outcome) = import_plan_file(pool, project_id, &file.path, body).await
                else {
                    continue;
                };
                sqlx::query(
                    "INSERT INTO spec_plan_files (path, modified, size, spec_id, stored)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(path) DO UPDATE SET modified = excluded.modified,
                        size = excluded.size, spec_id = excluded.spec_id,
                        stored = excluded.stored",
                )
                .bind(&key)
                .bind(file.modified)
                .bind(file.size)
                .bind(outcome.0.as_deref())
                .bind(i64::from(outcome.1))
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
                outcome
            }
        };
        if was_stored {
            stored += 1;
        }
        let Some(spec_id) = spec_id else {
            continue;
        };
        let in_user_dir = user_plans_dir.is_some_and(|dir| file.path.starts_with(dir));
        if in_user_dir
            && newest
                .as_ref()
                .is_none_or(|(time, _)| file.modified >= *time)
        {
            newest = Some((file.modified, spec_id));
        }
    }
    if let Some((_, spec_id)) = newest {
        for session in live_sessions {
            bind_session_implicit(pool, session, &spec_id).await?;
        }
    }
    Ok(stored)
}

/// Store one changed plan file and bind the sessions it names. Returns the
/// spec id (none for a fragment) and whether a revision was stored, or `None`
/// when the import failed and should be tried again on the next scan.
async fn import_plan_file(
    pool: &SqlitePool,
    project_id: &str,
    path: &Path,
    body: &str,
) -> Option<(Option<String>, bool)> {
    if !plan_file_is_finished(body) {
        return Some((None, false));
    }
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("plan");
    let spec_id = slug(stem);
    if spec_id.is_empty() {
        return Some((None, false));
    }
    let title = plan_title(body, stem);
    let hash = hash_bytes(body.trim().as_bytes());
    let session = format!("plan-{spec_id}");
    let stored = capture_finished_plan(
        pool,
        project_id,
        &spec_id,
        &title,
        "plan mode",
        body.trim(),
        &session,
        &hash,
        "plan_mode",
        true,
    )
    .await
    .ok()?
    .is_some();
    for extra in sessions_named_in(body) {
        bind_session_implicit(pool, &extra, &spec_id).await.ok()?;
    }
    Some((Some(spec_id), stored))
}

fn collect_plan_markdown(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth > 3 || out.len() >= MAX_PLAN_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if SKIP_PLAN_DIRS.contains(&name) {
                continue;
            }
            collect_plan_markdown(&path, out, depth + 1);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn plan_title(body: &str, stem: &str) -> String {
    let text = body.trim_start();
    if let Some(yaml) = text.strip_prefix("---") {
        if let Some(end) = yaml.find("\n---") {
            for line in yaml[..end].lines() {
                let Some(name) = line.trim().strip_prefix("name:") else {
                    continue;
                };
                let name = name.trim().trim_matches(['"', '\'']);
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    stem.to_string()
}

fn sessions_named_in(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim().trim_start_matches('-').trim();
        let Some(rest) = trimmed.strip_prefix("session:") else {
            continue;
        };
        let id = rest.trim().trim_matches(['"', '\'']);
        if !id.is_empty() && !found.iter().any(|item| item == id) {
            found.push(id.to_string());
        }
    }
    let mut rest = body;
    while let Some(start) = rest.find("axs_") {
        let token_start = &rest[start..];
        let end = token_start
            .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .unwrap_or(token_start.len());
        let token = &token_start[..end];
        if token.len() > 4 && !found.iter().any(|item| item == token) {
            found.push(token.to_string());
        }
        rest = &token_start[end..];
    }
    found
}

/// Project-relative plan folders for the IDEs Ax installs, plus the shared docs folder.
/// The home directory and the filesystem root are not in this list.
/// Plan directories for the IDEs Ax installs, plus `docs/plans`.
/// The home directory and `/` are never scan roots; only these folders are.
const PROJECT_PLAN_REL: &[&str] = &[
    ".cursor/plans",
    ".claude/plans",
    ".codex/plans",
    ".gemini/plans",
    ".gemini/antigravity/plans",
    ".kiro/plans",
    ".windsurf/plans",
    ".continue/plans",
    ".opencode/plans",
    ".vscode/plans",
    ".zed/plans",
    ".hermes/plans",
    "docs/plans",
];

/// Home-relative plan folders. `~/.cursor/plans` stays the live-session source.
const HOME_PLAN_REL: &[&str] = &[
    ".cursor/plans",
    ".claude/plans",
    ".codex/plans",
    ".gemini/plans",
    ".gemini/antigravity/plans",
    ".kiro/plans",
    ".codeium/windsurf/plans",
    ".continue/plans",
    ".config/opencode/plans",
    ".hermes/plans",
];

fn push_plan_dirs(root: &Path, relative: &[&str], dirs: &mut Vec<PathBuf>) {
    for rel in relative {
        dirs.push(root.join(rel));
    }
}

fn plan_directories(
    workspace_root: &Path,
    home: Option<&Path>,
    recent: &[PathBuf],
) -> (Vec<PathBuf>, Option<PathBuf>) {
    let mut roots = member_roots(workspace_root);
    for path in recent {
        if !is_broad_root(path) && !roots.iter().any(|root| root == path) {
            roots.push(path.clone());
        }
    }
    let mut dirs = Vec::new();
    for root in roots {
        if is_broad_root(&root) {
            continue;
        }
        push_plan_dirs(&root, PROJECT_PLAN_REL, &mut dirs);
    }
    let user_plans = home.map(|dir| dir.join(".cursor").join("plans"));
    if let Some(home) = home.filter(|dir| *dir != Path::new("/")) {
        push_plan_dirs(home, HOME_PLAN_REL, &mut dirs);
    }
    (dirs, user_plans)
}

fn member_roots(workspace_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![workspace_root.to_path_buf()];
    for name in ["ax.json", ".ax.json"] {
        let Ok(text) = std::fs::read_to_string(workspace_root.join(name)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(members) = value.get("members").and_then(|item| item.as_array()) else {
            continue;
        };
        for member in members {
            let Some(rel) = member.get("path").and_then(|item| item.as_str()) else {
                continue;
            };
            if rel.is_empty() {
                continue;
            }
            let joined = workspace_root.join(rel);
            if joined.is_dir() && !roots.iter().any(|root| root == &joined) {
                roots.push(joined);
            }
        }
        break;
    }
    roots
}

fn recent_project_paths() -> Vec<PathBuf> {
    let Some(home) = ax_utils::paths::home_dir() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(home.join(".ax").join("config.json")) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(recent) = value
        .pointer("/workspace/recent")
        .and_then(|item| item.as_array())
    else {
        return Vec::new();
    };
    recent
        .iter()
        .filter_map(|item| {
            let path = PathBuf::from(item.get("path")?.as_str()?);
            if path.is_dir() && !is_broad_root(&path) {
                Some(path)
            } else {
                None
            }
        })
        .take(20)
        .collect()
}

async fn live_bound_spec(pool: &SqlitePool, root: &Path) -> Option<String> {
    for session_id in live_session_ids(root) {
        if let Ok(Some(spec_id)) = bound_spec(pool, &session_id).await {
            return Some(spec_id);
        }
    }
    None
}

fn live_session_ids(root: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(text) = std::fs::read_to_string(root.join(".ax").join("turns").join("current.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(id) = value.get("session_id").and_then(|item| item.as_str()) {
                let id = id.trim();
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
    }
    if let Some(home) = ax_utils::paths::home_dir() {
        if let Ok(text) = std::fs::read_to_string(home.join(".ax").join("active-cursor-session")) {
            let id = text.trim();
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

fn is_broad_root(path: &Path) -> bool {
    if path == Path::new("/") {
        return true;
    }
    ax_utils::paths::home_dir().is_some_and(|home| path == home)
}

fn paths_named_in(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for raw in body.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`' | ','
            )
    }) {
        let token = raw.trim_matches(|c: char| matches!(c, '.' | ':' | ';' | '*' | '#'));
        if token.contains("://") || token.starts_with('/') || token.contains("..") {
            continue;
        }
        let Some((_, file)) = token.rsplit_once('/') else {
            continue;
        };
        if file.is_empty() || !file.contains('.') {
            continue;
        }
        if !token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+'))
        {
            continue;
        }
        found.push(token.to_string());
    }
    found.sort();
    found.dedup();
    found
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for character in text.chars().flat_map(|character| character.to_lowercase()) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

fn first_line(body: &str) -> String {
    body.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(180)
        .collect()
}

/// A revision body as stored and hashed: without trailing line breaks. The
/// Markdown copy adds one newline back, and a file read compares the same way.
fn stored_body(body: &str) -> &str {
    body.trim_end_matches(['\n', '\r'])
}

fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Hold this for the whole test when it swaps `AX_HOME_DIR` or depends on the home it names.
    pub(crate) static HOME_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    pub(crate) struct IsolatedHome {
        previous: Option<std::ffi::OsString>,
        dir: tempfile::TempDir,
    }

    impl IsolatedHome {
        pub(crate) fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("AX_HOME_DIR");
            std::env::set_var("AX_HOME_DIR", dir.path());
            Self { previous, dir }
        }

        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    impl Drop for IsolatedHome {
        fn drop(&mut self) {
            match self.previous.clone() {
                Some(value) => std::env::set_var("AX_HOME_DIR", value),
                None => std::env::remove_var("AX_HOME_DIR"),
            }
        }
    }

    struct Open {
        dir: tempfile::TempDir,
        pool: SqlitePool,
        _database: ax_db::Database,
    }

    async fn open() -> Open {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
        let database = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
            .await
            .unwrap();
        let pool = database.pool().clone();
        Open {
            dir,
            pool,
            _database: database,
        }
    }

    #[test]
    fn a_mention_of_plan_is_not_a_request() {
        assert!(!is_plan_request("We should plan the release later."));
        assert!(is_plan_request("Please make a plan for the registry."));
        assert!(is_plan_request("nieuwe spec: sla plannen op"));
    }

    #[tokio::test]
    async fn a_partial_stream_writes_no_revision() {
        let open = open().await;
        let pool = &open.pool;
        ensure_spec_schema(pool).await.unwrap();
        let stored = capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "half",
            "s",
            "t",
            "turn",
            false,
        )
        .await
        .unwrap();
        assert!(stored.is_none());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spec_revisions")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn a_finished_plan_is_a_draft_and_an_edit_keeps_the_first_version() {
        let open = open().await;
        let pool = &open.pool;
        let first = capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "version one",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(first.version, 1);
        assert_eq!(first.status, "draft");
        let second = capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "version two",
            "s",
            "t2",
            "turn",
            true,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(second.version, 2);
        let original = revision(pool, "registry", 1).await.unwrap().unwrap();
        assert_eq!(original.body, "version one");
    }

    #[tokio::test]
    async fn approval_writes_the_markdown_and_keeps_the_previous_body() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "one",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "two",
            "s",
            "t2",
            "turn",
            true,
        )
        .await
        .unwrap();
        let approved = approve_revision(pool, dir.path(), "registry", 2, &[])
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        let file = std::fs::read_to_string(dir.path().join("docs/specs/registry/SPEC.md")).unwrap();
        assert_eq!(file, "two\n");
        let first = revision(pool, "registry", 1).await.unwrap().unwrap();
        assert_eq!(first.body, "one");
        assert_eq!(first.status, "draft");
        let current = approved_revision(pool, "registry").await.unwrap().unwrap();
        assert_eq!(current.version, 2);
        assert_eq!(current.body, "two");
    }

    #[tokio::test]
    async fn the_approved_spec_markdown_is_not_captured_as_another_spec() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "one",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "registry", 1, &[])
            .await
            .unwrap();
        let changed = vec!["docs/specs/registry/SPEC.md".to_string()];
        for turn in ["t2", "t3", "t4"] {
            on_turn_finished(pool, dir.path(), "ax", "s", turn, "", "", &changed)
                .await
                .unwrap();
        }
        let ids: Vec<String> = list_specs(pool)
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(ids, vec!["registry".to_string()]);
        assert_eq!(
            bound_spec(pool, "s").await.unwrap().as_deref(),
            Some("registry")
        );
        assert_eq!(list_revisions(pool, "registry").await.unwrap().len(), 1);
        assert_eq!(delivered_version(pool, "s", "t4").await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn a_dirty_spec_file_adds_one_draft_and_keeps_the_binding() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "one",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        let other = dir.path().join("docs/specs/billing/SPEC.md");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "Charge once.\n").unwrap();
        let changed = vec!["docs/specs/billing/SPEC.md".to_string()];
        for turn in ["t2", "t3", "t4"] {
            on_turn_finished(pool, dir.path(), "ax", "s", turn, "", "", &changed)
                .await
                .unwrap();
        }
        // The spec id comes from its folder, and one body is one draft.
        assert_eq!(list_revisions(pool, "billing").await.unwrap().len(), 1);
        assert_eq!(
            bound_spec(pool, "s").await.unwrap().as_deref(),
            Some("registry")
        );
    }

    #[tokio::test]
    async fn a_linked_file_change_does_not_rewrite_the_approved_body() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "keep me",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let linked = dir.path().join("src/lib.rs");
        std::fs::write(&linked, "old").unwrap();
        approve_revision(pool, dir.path(), "registry", 1, &["src/lib.rs".into()])
            .await
            .unwrap();
        std::fs::write(&linked, "new").unwrap();
        assert!(check_linked_files(pool, dir.path(), "registry", "s", "t2")
            .await
            .unwrap());
        let approved = approved_revision(pool, "registry").await.unwrap().unwrap();
        assert_eq!(approved.body, "keep me");
        let context = spec_context_block(pool, dir.path(), "s").await;
        assert!(context.contains("approved: v1"));
        assert!(context.contains("drift: yes"));
        assert!(context.contains("hash: "));
    }

    #[tokio::test]
    async fn an_edited_markdown_file_is_a_conflict() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "approved text",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "registry", 1, &[])
            .await
            .unwrap();
        let path = dir.path().join("docs/specs/registry/SPEC.md");
        std::fs::write(&path, "edited text\n").unwrap();
        let hashes = sync_markdown(pool, dir.path(), "registry")
            .await
            .unwrap()
            .unwrap();
        assert_ne!(hashes.0, hashes.1);
        let approved = approved_revision(pool, "registry").await.unwrap().unwrap();
        assert_eq!(approved.body, "approved text");
        let context = spec_context_block(pool, dir.path(), "s").await;
        assert!(context.contains("delivery: conflict"));
        assert!(context.contains(&format!("approvedHash: {}", hashes.0)));
        assert!(context.contains(&format!("fileHash: {}", hashes.1)));
    }

    #[tokio::test]
    async fn an_approved_body_ending_in_a_newline_is_not_a_conflict() {
        let _lock = HOME_LOCK.lock().await;
        let _home = IsolatedHome::new();
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let plans = dir.path().join("docs/plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("x.md"), "# X\n\nKeep the file plan.\n").unwrap();
        on_turn_finished(
            pool,
            dir.path(),
            "ax",
            "s",
            "t1",
            "edit the plan",
            "done",
            &["docs/plans/x.md".to_string()],
        )
        .await
        .unwrap();
        let spec_id = markdown_spec_id("docs/plans/x.md");
        approve_revision(pool, dir.path(), &spec_id, 1, &[])
            .await
            .unwrap();
        assert_eq!(
            sync_markdown(pool, dir.path(), &spec_id).await.unwrap(),
            None
        );
        let context = spec_context_block(pool, dir.path(), "s").await;
        assert!(!context.contains("delivery: conflict"), "{context}");
        assert_eq!(list_revisions(pool, &spec_id).await.unwrap().len(), 1);
        // A body stored before newlines were trimmed is compared the same way.
        sqlx::query("UPDATE spec_revisions SET body = body || char(10), content_hash = ?1 WHERE spec_id = ?2")
            .bind(hash_text("# X\n\nKeep the file plan.\n"))
            .bind(&spec_id)
            .execute(pool)
            .await
            .unwrap();
        assert_eq!(
            sync_markdown(pool, dir.path(), &spec_id).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn plan_files_do_not_replace_an_explicit_binding() {
        let _lock = HOME_LOCK.lock().await;
        let home = IsolatedHome::new();
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "Keep the registry.",
            "axs_bound",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "registry", 1, &[])
            .await
            .unwrap();
        let user_plans = home.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&user_plans).unwrap();
        std::fs::write(
            user_plans.join("other.plan.md"),
            "# Other\n\nA plan for another project.\n",
        )
        .unwrap();
        let plans = dir.path().join("docs").join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(
            plans.join("notes.md"),
            "# Notes\n\nSeen in axs_bound earlier.\n",
        )
        .unwrap();
        let turns = dir.path().join(".ax").join("turns");
        std::fs::create_dir_all(&turns).unwrap();
        std::fs::write(
            turns.join("current.json"),
            "{\"session_id\":\"axs_bound\"}\n",
        )
        .unwrap();
        let context = spec_context_block(pool, dir.path(), "axs_bound").await;
        assert!(
            context.starts_with("spec: registry\napproved: v1"),
            "{context}"
        );
        assert_eq!(
            bound_spec(pool, "axs_bound").await.unwrap().as_deref(),
            Some("registry")
        );
    }

    #[tokio::test]
    async fn a_plan_file_binding_follows_the_newest_plan() {
        let _lock = HOME_LOCK.lock().await;
        let home = IsolatedHome::new();
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let user_plans = home.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&user_plans).unwrap();
        let first = user_plans.join("first.plan.md");
        std::fs::write(&first, "# First\n\nThe first plan.\n").unwrap();
        let turns = dir.path().join(".ax").join("turns");
        std::fs::create_dir_all(&turns).unwrap();
        std::fs::write(
            turns.join("current.json"),
            "{\"session_id\":\"axs_live\"}\n",
        )
        .unwrap();
        let context = spec_context_block(pool, dir.path(), "axs_live").await;
        assert!(context.starts_with("spec: first-plan\n"), "{context}");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&first)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        std::fs::write(
            user_plans.join("second.plan.md"),
            "# Second\n\nThe next plan.\n",
        )
        .unwrap();
        let context = spec_context_block(pool, dir.path(), "axs_live").await;
        assert!(context.starts_with("spec: second-plan\n"), "{context}");
    }

    #[tokio::test]
    async fn a_bound_turn_can_show_the_version_it_received() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "body",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        let approved = approve_revision(pool, dir.path(), "registry", 1, &[])
            .await
            .unwrap();
        record_delivery(
            pool,
            "s",
            "t9",
            "registry",
            approved.version,
            &approved.content_hash,
        )
        .await
        .unwrap();
        capture_finished_plan(
            pool,
            "ax",
            "registry",
            "Registry",
            "make a plan",
            "later",
            "s",
            "t2",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "registry", 2, &[])
            .await
            .unwrap();
        assert_eq!(delivered_version(pool, "s", "t9").await.unwrap(), Some(1));
        let context = spec_context_block(pool, dir.path(), "s").await;
        assert!(context.contains("approved: v2"));
        assert!(context.contains("pointer: ax spec show registry@2"));
    }

    #[tokio::test]
    async fn an_unbound_session_says_none() {
        let _lock = HOME_LOCK.lock().await;
        let _home = IsolatedHome::new();
        let open = open().await;
        let context = spec_context_block(&open.pool, open.dir.path(), "missing").await;
        assert_eq!(context, "none");
    }

    #[tokio::test]
    async fn an_approved_plan_file_stays_a_plan() {
        let open = open().await;
        let plans = open.dir.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("queue.md"), "# Queue\n\nKeep the first job.\n").unwrap();
        import_plan_mode_files(&open.pool, open.dir.path(), "ax")
            .await
            .unwrap();
        approve_revision(&open.pool, open.dir.path(), "queue", 1, &[])
            .await
            .unwrap();
        let specs = list_specs(&open.pool).await.unwrap();
        assert_eq!(
            specs.iter().find(|item| item.id == "queue").unwrap().kind,
            "plan"
        );
    }

    #[tokio::test]
    async fn a_live_session_plan_reaches_a_different_chat() {
        let _lock = HOME_LOCK.lock().await;
        let open = open().await;
        let home = IsolatedHome::new();
        let plans = home.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(
            plans.join("queue.plan.md"),
            "# Queue\n\nKeep the first job.\n",
        )
        .unwrap();
        let turns = open.dir.path().join(".ax").join("turns");
        std::fs::create_dir_all(&turns).unwrap();
        std::fs::write(
            turns.join("current.json"),
            "{\"session_id\":\"cursor-live\"}\n",
        )
        .unwrap();
        let context = spec_context_block(&open.pool, open.dir.path(), "axs-other").await;
        assert!(
            context.contains("spec: queue-plan\napproved: none\npointer: ax spec show queue-plan"),
            "{context}"
        );
        assert!(!context.contains("Keep the first job"), "{context}");
    }

    #[tokio::test]
    async fn a_finished_plan_file_is_a_draft_and_a_fragment_is_not() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let plans = dir.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("partial.md"), "still writing").unwrap();
        std::fs::write(
            plans.join("billing.md"),
            "# Billing\n\nKeep the first price.\n",
        )
        .unwrap();
        let stored = import_plan_mode_files(pool, dir.path(), "ax")
            .await
            .unwrap();
        assert_eq!(stored, 1);
        let again = import_plan_mode_files(pool, dir.path(), "ax")
            .await
            .unwrap();
        assert_eq!(again, 1);
        let specs = list_specs(pool).await.unwrap();
        let billing = specs.iter().find(|item| item.id == "billing").unwrap();
        assert_eq!(billing.latest_status, "draft");
        assert_eq!(billing.approved_version, None);
        assert_eq!(billing.revision_count, 1);
        assert!(billing.sessions.contains("plan-billing"));
        let history = list_revisions(pool, "billing").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].source, "plan_mode");
        assert!(history[0].body.contains("Keep the first price."));
        assert!(!dir.path().join("docs/specs/billing/SPEC.md").exists());
        let _lock = HOME_LOCK.lock().await;
        let _home = IsolatedHome::new();
        let context = spec_context_block(pool, dir.path(), "missing").await;
        assert_eq!(context, "none");
    }

    #[tokio::test]
    async fn a_named_code_file_change_is_drift_and_the_approved_body_stays() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let code = dir.path().join("src");
        std::fs::create_dir_all(&code).unwrap();
        std::fs::write(code.join("lib.rs"), "fn price() {}\n").unwrap();
        capture_finished_plan(
            pool,
            "ax",
            "billing",
            "Billing",
            "make a plan",
            "Keep src/lib.rs stable.",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "billing", 1, &[])
            .await
            .unwrap();
        std::fs::write(code.join("lib.rs"), "fn price() { 1 }\n").unwrap();
        let drifted = check_linked_files(pool, dir.path(), "billing", "s", "t2")
            .await
            .unwrap();
        assert!(drifted);
        let approved = approved_revision(pool, "billing").await.unwrap().unwrap();
        assert_eq!(approved.body, "Keep src/lib.rs stable.");
        let history = list_revisions(pool, "billing").await.unwrap();
        assert!(history.iter().any(|row| row.status == "change_requested"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_drift_checks_store_one_note_and_all_report_drift() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let code = dir.path().join("src");
        std::fs::create_dir_all(&code).unwrap();
        std::fs::write(code.join("lib.rs"), "fn price() {}\n").unwrap();
        capture_finished_plan(
            pool,
            "ax",
            "billing",
            "Billing",
            "make a plan",
            "Keep src/lib.rs stable.",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "billing", 1, &[])
            .await
            .unwrap();
        std::fs::write(code.join("lib.rs"), "fn price() { 1 }\n").unwrap();
        let tasks: Vec<_> = (0..12)
            .map(|index| {
                let pool = pool.clone();
                let root = dir.path().to_path_buf();
                tokio::spawn(async move {
                    check_linked_files(&pool, &root, "billing", "s", &format!("t{index}")).await
                })
            })
            .collect();
        for task in tasks {
            assert_eq!(task.await.unwrap(), Ok(true));
        }
        let notes: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM spec_revisions WHERE spec_id = 'billing' AND status = 'change_requested'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(notes, 1);
    }

    #[tokio::test]
    async fn a_failed_drift_check_is_reported_as_unknown_not_as_no_drift() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let _lock = HOME_LOCK.lock().await;
        let _home = IsolatedHome::new();
        capture_finished_plan(
            pool,
            "ax",
            "billing",
            "Billing",
            "make a plan",
            "Keep the ledger.",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "billing", 1, &[])
            .await
            .unwrap();
        sqlx::query("DROP TABLE spec_file_seals")
            .execute(pool)
            .await
            .unwrap();
        let context = spec_context_block(pool, dir.path(), "s").await;
        assert!(context.contains("drift: unknown"), "{context}");
    }

    async fn draft_billing(pool: &SqlitePool) {
        capture_finished_plan(
            pool,
            "ax",
            "billing",
            "Billing",
            "make a plan",
            "Keep the ledger.",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
    }

    async fn table_shape(
        pool: &SqlitePool,
        table: &str,
    ) -> Vec<(String, String, i64, Option<String>, i64)> {
        sqlx::query_as(&format!(
            "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('{table}') ORDER BY cid"
        ))
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn memory_pool() -> SqlitePool {
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_spec_tables_have_one_shape_whichever_module_creates_them() {
        let registry = memory_pool().await;
        ensure_spec_schema(&registry).await.unwrap();
        let hierarchy = memory_pool().await;
        crate::spec_hierarchy::ensure_hierarchy_schema(&hierarchy)
            .await
            .unwrap();
        for table in ["specs", "spec_revisions"] {
            assert_eq!(
                table_shape(&registry, table).await,
                table_shape(&hierarchy, table).await,
                "{table}"
            );
        }
    }

    #[tokio::test]
    async fn approving_with_a_duplicate_linked_file_stores_one_seal() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn a() {}\n").unwrap();
        draft_billing(pool).await;
        let links = ["src/a.rs".to_string(), "src/a.rs".to_string()];
        let approved = approve_revision(pool, dir.path(), "billing", 1, &links)
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        let seals: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM spec_file_seals WHERE spec_id = 'billing'")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(seals, 1);
    }

    #[tokio::test]
    async fn a_failed_markdown_write_leaves_the_revision_unapproved() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        draft_billing(pool).await;
        // A file where the spec folder should be makes the write fail.
        std::fs::create_dir_all(dir.path().join("docs/specs")).unwrap();
        std::fs::write(dir.path().join("docs/specs/billing"), "not a folder").unwrap();
        assert!(approve_revision(pool, dir.path(), "billing", 1, &[])
            .await
            .is_err());
        let status = revision(pool, "billing", 1).await.unwrap().unwrap().status;
        assert_eq!(status, "draft");
        assert_eq!(approved_revision(pool, "billing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_edit_appends_a_draft_and_leaves_the_approved_body() {
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        capture_finished_plan(
            pool,
            "ax",
            "billing",
            "Billing",
            "make a plan",
            "one",
            "s",
            "t1",
            "turn",
            true,
        )
        .await
        .unwrap();
        approve_revision(pool, dir.path(), "billing", 1, &[])
            .await
            .unwrap();
        let path = dir.path().join("docs/specs/billing/SPEC.md");
        let file_before = std::fs::read_to_string(&path).unwrap();
        let saved = save_revision(pool, "billing", 1, "two").await.unwrap();
        assert_eq!(saved.version, 2);
        assert_eq!(saved.status, "draft");
        assert_eq!(
            approved_revision(pool, "billing")
                .await
                .unwrap()
                .unwrap()
                .body,
            "one"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), file_before);
        let history = list_revisions(pool, "billing").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].based_on, Some(1));
        assert_eq!(history[1].source, "edit");
        assert!(history[1].created_at > 0);
        let again = save_revision(pool, "billing", 2, "two").await.unwrap();
        assert_eq!(again.version, 2);
        assert_eq!(list_revisions(pool, "billing").await.unwrap().len(), 2);
        assert!(save_revision(pool, "billing", 2, "   ").await.is_err());
        assert_eq!(list_revisions(pool, "billing").await.unwrap().len(), 2);
        let plans = dir.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("queue.md"), "# Queue\n\nKeep the first job.\n").unwrap();
        import_plan_mode_files(pool, dir.path(), "ax")
            .await
            .unwrap();
        let specs = list_specs(pool).await.unwrap();
        assert_eq!(
            specs.iter().find(|item| item.id == "queue").unwrap().kind,
            "plan"
        );
        assert_eq!(
            specs.iter().find(|item| item.id == "billing").unwrap().kind,
            "spec"
        );
    }

    #[tokio::test]
    async fn workspace_and_user_plans_keep_a_session() {
        let _home = HOME_LOCK.lock().await;
        let open = open().await;
        let pool = &open.pool;
        let dir = &open.dir;
        let home = tempfile::tempdir().unwrap();
        let member = dir.path().join("svc");
        std::fs::create_dir_all(member.join(".cursor").join("plans")).unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"members":[{"path":"svc"}]}"#,
        )
        .unwrap();
        std::fs::write(
            member.join(".cursor").join("plans").join("member.md"),
            "# Member\n\nKeep the member plan.\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("docs").join("plans")).unwrap();
        std::fs::write(
            dir.path().join("docs").join("plans").join("queue.md"),
            "# Queue\n\nKeep the first job.\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".claude").join("plans")).unwrap();
        std::fs::write(
            dir.path().join(".claude").join("plans").join("review.md"),
            "# Review\n\nKeep the Claude plan.\n",
        )
        .unwrap();
        std::fs::create_dir_all(home.path().join(".claude").join("plans")).unwrap();
        std::fs::write(
            home.path()
                .join(".claude")
                .join("plans")
                .join("claude.plan.md"),
            "---\nname: Claude plan\n---\n\n# Claude\n\nKeep the home Claude plan.\n",
        )
        .unwrap();
        let user_plans = home.path().join(".cursor").join("plans");
        std::fs::create_dir_all(&user_plans).unwrap();
        std::fs::write(user_plans.join("fragment.md"), "still writing").unwrap();
        let older = user_plans.join("older.plan.md");
        std::fs::write(&older, "# Older\n\nKeep the older plan.\n").unwrap();
        let open_plan = user_plans.join("edit_spec_history_6f184f74.plan.md");
        std::fs::write(
            &open_plan,
            "---\nname: Edit spec history\noverview: keep both lines\n---\n\n# Edit spec history\n\nKeep the open plan.\nsession: axs_from_file\n",
        )
        .unwrap();
        let older_file = std::fs::OpenOptions::new()
            .write(true)
            .open(&older)
            .unwrap();
        older_file
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        let (dirs, user) = plan_directories(dir.path(), Some(home.path()), &[]);
        assert!(dirs.iter().any(|path| {
            path.ends_with(std::path::Path::new("svc").join(".cursor").join("plans"))
        }));
        assert!(dirs.iter().any(|path| {
            path.ends_with(std::path::Path::new(".claude").join("plans"))
                && path.starts_with(dir.path())
        }));
        assert!(dirs.iter().any(|path| path.ends_with(
            std::path::Path::new(".codeium")
                .join("windsurf")
                .join("plans")
        )));
        assert!(dirs.iter().all(|path| path != Path::new("/")));
        assert!(dirs
            .iter()
            .all(|path| Some(path.as_path()) != ax_utils::paths::home_dir().as_deref()));
        let real_home = ax_utils::paths::home_dir().expect("home");
        let (real_dirs, real_user) = plan_directories(dir.path(), Some(&real_home), &[]);
        let cursor_plans = real_home.join(".cursor").join("plans");
        assert!(real_dirs.iter().any(|path| path == &cursor_plans));
        assert_eq!(real_user.as_deref(), Some(cursor_plans.as_path()));
        assert!(real_dirs
            .iter()
            .any(|path| path == &real_home.join(".claude").join("plans")));
        assert!(real_dirs.iter().all(|path| !is_broad_root(path)));
        let stored = import_plan_dirs(pool, "ax", &dirs, user.as_deref(), &["axs_live".into()])
            .await
            .unwrap();
        assert_eq!(stored, 6);
        let again = import_plan_dirs(pool, "ax", &dirs, user.as_deref(), &["axs_live".into()])
            .await
            .unwrap();
        assert_eq!(again, 6);
        let specs = list_specs(pool).await.unwrap();
        assert!(specs.iter().all(|item| item.id != "fragment"));
        let claude = specs
            .iter()
            .find(|item| item.title == "Claude plan")
            .unwrap();
        assert!(claude.sessions.contains(&format!("plan-{}", claude.id)));
        assert!(!claude.sessions.contains("axs_live"));
        assert!(specs.iter().any(|item| item.id == "review"));
        let open_spec = specs
            .iter()
            .find(|item| item.title == "Edit spec history")
            .unwrap();
        assert!(open_spec
            .sessions
            .split_whitespace()
            .any(|id| id == format!("plan-{}", open_spec.id)));
        assert!(open_spec.sessions.contains("axs_from_file"));
        assert!(open_spec.sessions.contains("axs_live"));
        assert_eq!(list_revisions(pool, &open_spec.id).await.unwrap().len(), 1);
        let older_spec = specs.iter().find(|item| item.id == "older-plan").unwrap();
        assert!(older_spec.sessions.contains("plan-older-plan"));
        assert!(!older_spec.sessions.contains("axs_live"));
        let saved = save_revision(
            pool,
            &open_spec.id,
            1,
            "Keep the open plan.\n\nA second line.",
        )
        .await
        .unwrap();
        assert_eq!(saved.version, 2);
        let after = list_specs(pool).await.unwrap();
        let open_after = after.iter().find(|item| item.id == open_spec.id).unwrap();
        assert!(open_after.sessions.contains("axs_live"));
        assert!(open_after
            .sessions
            .contains(&format!("plan-{}", open_spec.id)));
        let body = show_spec(pool, &format!("{}@1", open_spec.id))
            .await
            .unwrap();
        assert!(body.contains("Keep the open plan."));
        assert!(!body.contains("A second line."));
    }

    #[tokio::test]
    async fn the_list_uses_the_latest_revision_time_and_puts_the_newest_first() {
        let open = open().await;
        ensure_spec_schema(&open.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO specs (id, project_id, title, markdown_path, created_at) VALUES
             ('old', 'p', 'Old title', 'docs/specs/old/SPEC.md', 1000),
             ('fresh', 'p', 'Fresh title', 'docs/specs/fresh/SPEC.md', 2000),
             ('bare', 'p', 'Bare title', 'docs/specs/bare/SPEC.md', 9000)",
        )
        .execute(&open.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO spec_revisions
             (spec_id, version, status, body, content_hash, source, based_on, created_at) VALUES
             ('old', 1, 'draft', 'a', 'h', 'plan_mode', NULL, 4000),
             ('old', 2, 'draft', 'a2', 'h2', 'edit', 1, 5000),
             ('fresh', 1, 'draft', 'b', 'h', 'plan_mode', NULL, 3000)",
        )
        .execute(&open.pool)
        .await
        .unwrap();
        let specs = list_specs(&open.pool).await.unwrap();
        let ids: Vec<_> = specs.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["bare", "old", "fresh"]);
        assert_eq!(specs[0].updated_at, 9000);
        assert_eq!(specs[0].latest_version, 0);
        assert_eq!(specs[1].updated_at, 5000);
        assert_eq!(specs[1].latest_version, 2);
        assert_eq!(specs[1].latest_status, "draft");
        assert_eq!(specs[1].latest_source, "edit");
        assert_eq!(specs[1].latest_based_on, Some(1));
        assert_eq!(specs[2].updated_at, 3000);
        assert_eq!(specs[2].latest_version, 1);
        assert_eq!(specs[2].latest_source, "plan_mode");
        assert_eq!(specs[2].latest_based_on, None);
    }
}
