//! Turn history: related past turns for preflight and "when did I change X" (docs/specs/turn-memory-outcomes.md).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::turns::{turn_outcome, OUTCOME_MARKER, TURN_KIND};
use crate::types::MemoryRow;

/// A `ax_history` query: a file path, symbol name, or free text.
#[derive(Debug, Clone, Default)]
pub struct HistoryQuery {
    pub query: String,
    pub since_ms: Option<i64>,
    pub limit: usize,
}

/// One dated action: an agent turn or a git commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at_ms: i64,
    /// `turn` or `commit`.
    pub kind: String,
    /// Memory id for a turn, short hash for a commit.
    pub id: String,
    pub title: String,
    pub outcome: Option<String>,
    pub files: Vec<String>,
    pub commits: Vec<String>,
}

/// A prompt-matched turn must share this many significant words with the prompt.
const MIN_SHARED_WORDS: usize = 2;
const MIN_WORD_CHARS: usize = 4;
const RECALL_CANDIDATES: usize = 40;
const BLOCK_OUTCOME_CHARS: usize = 120;
const BLOCK_FILES: usize = 3;
const LISTING_FILES: usize = 10;

const STOP_WORDS: &[&str] = &[
    "about", "after", "again", "also", "alle", "alles", "been", "before", "could", "commits",
    "deze", "does", "done", "even", "files", "from", "gewoon", "have", "hebben", "into", "just",
    "like", "maar", "make", "moet", "more", "naar", "need", "nog", "omdat", "outcome", "please",
    "should", "some", "than", "that", "their", "them", "then", "there", "they", "this", "waar",
    "wanneer", "want", "welke", "what", "when", "where", "which", "will", "with", "wordt",
    "worden", "would", "zijn", "zodat",
];

const HISTORY_PHRASES: &[&str] = &[
    "wanneer heb ik",
    "wanneer hebben we",
    "wanneer is",
    "wat heb ik aangepast",
    "wat heb ik veranderd",
    "wat heb ik gewijzigd",
    "when did i",
    "when did we",
    "what did i change",
    "what did we change",
];

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

fn significant_words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= MIN_WORD_CHARS && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

fn shared_words(words: &BTreeSet<String>, row: &MemoryRow) -> usize {
    let own = significant_words(&format!("{} {}", row.title, row.body));
    words.intersection(&own).count()
}

/// Ids of turn memories that `recall` returns for `text`.
async fn recalled_turn_ids(pool: &SqlitePool, text: &str) -> Result<Vec<String>, AxError> {
    Ok(crate::store::recall(pool, text, RECALL_CANDIDATES)
        .await?
        .into_iter()
        .filter(|m| m.memory.kind == TURN_KIND)
        .map(|m| m.memory.id)
        .collect())
}

/// Past turns related to the current work: they changed one of `files`, or match `prompt`.
pub async fn related_turns(
    pool: &SqlitePool,
    prompt: &str,
    files: &[String],
    limit: usize,
) -> Result<Vec<MemoryRow>, AxError> {
    let words = significant_words(prompt);
    let recalled = if words.len() >= MIN_SHARED_WORDS {
        recalled_turn_ids(pool, prompt).await?
    } else {
        Vec::new()
    };
    // Only recalled rows can fail the word rule, so `limit + recalled` candidates always suffice.
    let candidates = limit.saturating_add(recalled.len());
    let rows =
        crate::store::matching_turn_rows(pool, None, &recalled, files, None, candidates).await?;
    Ok(rows
        .into_iter()
        .filter(|row| {
            row.files.iter().any(|f| files.contains(f))
                || (recalled.contains(&row.id) && shared_words(&words, row) >= MIN_SHARED_WORDS)
        })
        .take(limit)
        .collect())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn block_entry(row: &MemoryRow) -> String {
    let mut line = format!("- {} {}", format_when(row.created_at), one_line(&row.title));
    if let Some(outcome) = turn_outcome(&row.body) {
        line.push_str(&format!(
            ". Outcome: {}",
            clip(&one_line(outcome), BLOCK_OUTCOME_CHARS)
        ));
    }
    if !row.files.is_empty() {
        let shown: Vec<&str> = row
            .files
            .iter()
            .take(BLOCK_FILES)
            .map(String::as_str)
            .collect();
        line.push_str(&format!(". Files: {}", shown.join(", ")));
    }
    line.push('\n');
    line
}

/// `<ax_turn_history>` block for preflight; empty when there are no rows.
pub fn format_turn_history_block(rows: &[MemoryRow], max_chars: usize) -> String {
    const OPEN: &str =
        "<ax_turn_history>Past agent turns related to this work, newest first. Call ax_history for more.\n";
    const CLOSE: &str = "</ax_turn_history>";
    let mut block = OPEN.to_string();
    let mut entries = 0;
    for row in rows {
        let entry = block_entry(row);
        let len = block.chars().count() + entry.chars().count() + CLOSE.chars().count();
        if len > max_chars {
            break;
        }
        block.push_str(&entry);
        entries += 1;
    }
    if entries == 0 {
        return String::new();
    }
    block.push_str(CLOSE);
    block
}

/// Whether the prompt asks when something was changed.
pub fn is_history_question(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    HISTORY_PHRASES.iter().any(|p| lower.contains(p))
}

fn looks_like_path(query: &str) -> bool {
    !query.contains(char::is_whitespace) && (query.contains('/') || query.contains('.'))
}

/// Indexed files the query names: an exact or trailing path match, or the file of a symbol.
async fn resolve_paths(pool: &SqlitePool, query: &str) -> Result<Vec<String>, AxError> {
    let suffix = crate::store::path_suffix_pattern(query);
    let mut paths: Vec<String> = sqlx::query_scalar(
        "SELECT path FROM files WHERE path = ? OR path LIKE ? ESCAPE '\\' ORDER BY path LIMIT 20",
    )
    .bind(query)
    .bind(suffix)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    if paths.is_empty() && !query.contains(char::is_whitespace) {
        paths = sqlx::query_scalar(
            "SELECT DISTINCT file_path FROM nodes WHERE name = ? ORDER BY file_path LIMIT 20",
        )
        .bind(query)
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
    }
    Ok(paths)
}

fn turn_commits(body: &str) -> Vec<String> {
    let before_outcome = body.split(OUTCOME_MARKER).next().unwrap_or("");
    let Some((_, section)) = before_outcome.split_once("\n\nCommits:") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(str::to_string)
        .collect()
}

fn turn_entry(row: &MemoryRow) -> HistoryEntry {
    HistoryEntry {
        at_ms: row.created_at,
        kind: TURN_KIND.to_string(),
        id: row.id.clone(),
        title: row.title.clone(),
        outcome: turn_outcome(&row.body).map(str::to_string),
        files: row.files.clone(),
        commits: turn_commits(&row.body),
    }
}

/// Commits touching `pathspecs`, newest first. Empty outside a git repository.
async fn git_commits(
    root: &Path,
    pathspecs: &[String],
    since_ms: Option<i64>,
    limit: usize,
) -> Vec<HistoryEntry> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.current_dir(root).args([
        "log",
        "-n",
        &limit.to_string(),
        "--format=%x00%h%x09%ct%x09%s",
        "--name-only",
    ]);
    if let Some(since) = since_ms {
        cmd.arg(format!("--since=@{}", since.div_euclid(1000)));
    }
    cmd.arg("--").args(pathspecs);
    let Ok(output) = cmd.output().await else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter_map(|chunk| {
            let mut lines = chunk.lines();
            let mut head = lines.next()?.splitn(3, '\t');
            let (hash, secs, subject) = (head.next()?, head.next()?, head.next()?);
            Some(HistoryEntry {
                at_ms: secs.parse::<i64>().ok()? * 1000,
                kind: "commit".to_string(),
                id: hash.to_string(),
                title: subject.to_string(),
                outcome: None,
                files: lines
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string)
                    .collect(),
                commits: Vec::new(),
            })
        })
        .collect()
}

/// Turns and commits that touched the query, newest first.
pub async fn history(
    pool: &SqlitePool,
    root: &Path,
    query: &HistoryQuery,
) -> Result<Vec<HistoryEntry>, AxError> {
    let text = query.query.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let limit = if query.limit == 0 { 20 } else { query.limit };
    let paths = resolve_paths(pool, text).await?;
    let mut entries: Vec<HistoryEntry> = if !paths.is_empty() || looks_like_path(text) {
        let pathspecs = if paths.is_empty() {
            vec![text.to_string()]
        } else {
            paths.clone()
        };
        let mut files = paths.clone();
        files.push(text.to_string());
        let turns =
            crate::store::matching_turn_rows(pool, query.since_ms, &[], &files, Some(text), limit)
                .await?;
        let mut found: Vec<HistoryEntry> = turns.iter().map(turn_entry).collect();
        found.extend(git_commits(root, &pathspecs, query.since_ms, limit).await);
        found
    } else {
        let words = significant_words(text);
        let recalled = recalled_turn_ids(pool, text).await?;
        let turns = crate::store::matching_turn_rows(
            pool,
            query.since_ms,
            &recalled,
            &[],
            None,
            recalled.len(),
        )
        .await?;
        turns
            .iter()
            .filter(|row| shared_words(&words, row) >= 1)
            .map(turn_entry)
            .collect()
    };
    entries.sort_by_key(|e| std::cmp::Reverse(e.at_ms));
    entries.truncate(limit);
    Ok(entries)
}

/// One turn with its full outcome.
pub async fn history_entry(pool: &SqlitePool, id: &str) -> Result<Option<HistoryEntry>, AxError> {
    Ok(crate::store::get(pool, id)
        .await?
        .filter(|row| row.kind == TURN_KIND)
        .map(|row| turn_entry(&row)))
}

fn listing_entry(entry: &HistoryEntry, outcome_chars: usize) -> String {
    let mut text = format!(
        "{} {} {}: {}\n",
        format_when(entry.at_ms),
        entry.kind,
        entry.id,
        one_line(&entry.title)
    );
    if let Some(outcome) = &entry.outcome {
        text.push_str(&format!("  Outcome: {}\n", clip(outcome, outcome_chars)));
    }
    if !entry.files.is_empty() {
        let shown: Vec<&str> = entry
            .files
            .iter()
            .take(LISTING_FILES)
            .map(String::as_str)
            .collect();
        text.push_str(&format!("  Files: {}", shown.join(", ")));
        if entry.files.len() > shown.len() {
            text.push_str(&format!(" (+{} more)", entry.files.len() - shown.len()));
        }
        text.push('\n');
    }
    if !entry.commits.is_empty() {
        text.push_str(&format!("  Commits: {}\n", entry.commits.join("; ")));
    }
    text
}

/// Plain-text history listing; outcomes are cut to `outcome_chars`.
pub fn format_history(entries: &[HistoryEntry], outcome_chars: usize) -> String {
    if entries.is_empty() {
        return "No matching turns or commits.".to_string();
    }
    entries
        .iter()
        .map(|e| listing_entry(e, outcome_chars))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Local midnight of a `YYYY-MM-DD` date, in epoch milliseconds.
pub fn parse_since(date: &str) -> Option<i64> {
    use chrono::TimeZone;
    let day = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let midnight = day.and_hms_opt(0, 0, 0)?;
    chrono::Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.timestamp_millis())
}

/// `YYYY-MM-DD HH:MM` in local time.
pub fn format_when(ms: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms).single() {
        Some(local) => local.format("%Y-%m-%d %H:%M").to_string(),
        None => chrono::DateTime::from_timestamp_millis(ms)
            .map(|utc| utc.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default(),
    }
}
