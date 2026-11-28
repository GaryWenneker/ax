//! Which build may run background writers.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildStamp {
    pub version: String,
    pub mtime_ms: u64,
    pub len: u64,
}

pub fn current_build_stamp(version: &str) -> BuildStamp {
    let file = std::env::current_exe().ok().and_then(|path| {
        let meta = std::fs::metadata(path).ok()?;
        let mtime_ms = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        Some((mtime_ms, meta.len()))
    });
    let (mtime_ms, len) = file.unwrap_or((0, 0));
    BuildStamp {
        version: version.to_string(),
        mtime_ms,
        len,
    }
}

/// The numeric `major.minor.patch…` parts of a version, without any
/// `-pre` or `+build` suffix. `None` when a part is not a number.
fn version_parts(v: &str) -> Option<Vec<u64>> {
    let core = v.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or("");
    if core.is_empty() {
        return None;
    }
    core.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

/// True when `mine` is an older build than `other`: a lower release
/// version, or the same version with an earlier executable file time. The
/// file time only decides between builds of the same version, because it is
/// the install time, not the build order.
pub fn stamp_is_older(mine: &BuildStamp, other: &BuildStamp) -> bool {
    if let (Some(a), Some(b)) = (version_parts(&mine.version), version_parts(&other.version)) {
        if a != b {
            return a < b;
        }
    }
    mine.mtime_ms > 0 && other.mtime_ms > mine.mtime_ms
}

pub fn background_writers_allowed(mine: &BuildStamp, registered: &[BuildStamp]) -> bool {
    if mine.mtime_ms == 0 {
        return true;
    }
    !registered.iter().any(|other| stamp_is_older(mine, other))
}

pub fn stale_mcp_lines(mine: &BuildStamp, rows: &[(i64, BuildStamp)]) -> Vec<String> {
    rows.iter()
        .filter(|(_, stamp)| stamp_is_older(stamp, mine))
        .map(|(pid, _)| format!("MCP pid {pid} is an older binary. Restart that MCP server."))
        .collect()
}

pub async fn register_build(
    pool: &SqlitePool,
    role: &str,
    pid: i64,
    stamp: &BuildStamp,
) -> Result<(), AxError> {
    let seen_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO ax_builds (role, pid, version, mtime_ms, len, seen_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(role, pid) DO UPDATE SET
            version = excluded.version,
            mtime_ms = excluded.mtime_ms,
            len = excluded.len,
            seen_at = excluded.seen_at",
    )
    .bind(role)
    .bind(pid)
    .bind(&stamp.version)
    .bind(stamp.mtime_ms as i64)
    .bind(stamp.len as i64)
    .bind(seen_at)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

/// False when a running process has registered a newer build. A row whose
/// process has exited is removed, so a build that ran once and quit cannot
/// keep older builds from writing.
pub async fn writers_allowed(pool: &SqlitePool, mine: &BuildStamp) -> Result<bool, AxError> {
    writers_allowed_with(pool, mine, pid_alive).await
}

fn pid_alive(pid: i64) -> bool {
    u32::try_from(pid).is_ok_and(ax_utils::is_pid_alive)
}

async fn writers_allowed_with(
    pool: &SqlitePool,
    mine: &BuildStamp,
    alive: impl Fn(i64) -> bool,
) -> Result<bool, AxError> {
    if mine.mtime_ms == 0 {
        return Ok(true);
    }
    let own = i64::from(std::process::id());
    for (role, pid, stamp) in all_rows(pool).await? {
        if !stamp_is_older(mine, &stamp) {
            continue;
        }
        // Only rows that would block are checked, so the 200 ms watch poll
        // does not probe every pid each time.
        if pid == own || alive(pid) {
            return Ok(false);
        }
        sqlx::query("DELETE FROM ax_builds WHERE role = ? AND pid = ?")
            .bind(&role)
            .bind(pid)
            .execute(pool)
            .await
            .map_err(db_err)?;
    }
    Ok(true)
}

pub async fn mcp_stamps(pool: &SqlitePool) -> Result<Vec<(i64, BuildStamp)>, AxError> {
    let rows: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT pid, version, mtime_ms, len FROM ax_builds WHERE role = 'mcp' ORDER BY pid",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(pid, version, mtime_ms, len)| {
            (
                pid,
                BuildStamp {
                    version,
                    mtime_ms: mtime_ms.max(0) as u64,
                    len: len.max(0) as u64,
                },
            )
        })
        .collect())
}

async fn all_rows(pool: &SqlitePool) -> Result<Vec<(String, i64, BuildStamp)>, AxError> {
    let rows: Vec<(String, i64, String, i64, i64)> =
        sqlx::query_as("SELECT role, pid, version, mtime_ms, len FROM ax_builds")
            .fetch_all(pool)
            .await
            .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(role, pid, version, mtime_ms, len)| {
            (
                role,
                pid,
                BuildStamp {
                    version,
                    mtime_ms: mtime_ms.max(0) as u64,
                    len: len.max(0) as u64,
                },
            )
        })
        .collect())
}

fn db_err(err: sqlx::Error) -> AxError {
    AxError::Database(DatabaseError::new(err.to_string()))
}
