//! Save and restore Cursor IDE auth sessions (state.vscdb + auth.json).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const AUTH_KEYS: &[&str] = &[
    "cursorAuth/accessToken",
    "cursorAuth/refreshToken",
    "cursorAuth/stripeMembershipType",
    "cursorAuth/stripeSubscriptionStatus",
    "cursorAuth/cachedEmail",
    "cursorAuth/cachedSignUpType",
    "cursorAuth/cachedScopedProfile",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthJsonFile {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorAuthSnapshot {
    pub version: u32,
    pub saved_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub email: String,
    pub membership: String,
    pub subscription_status: String,
    pub sign_up_type: String,
    pub scoped_profile: String,
    pub auth_json: AuthJsonFile,
    #[serde(default)]
    pub vscdb_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProfileMeta {
    pub name: String,
    pub label: String,
    pub email: String,
    pub membership: String,
    pub saved_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    profiles: Vec<String>,
}

pub fn cursor_roaming_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Cursor"))
}

pub fn auth_json_path() -> Option<PathBuf> {
    cursor_roaming_dir().map(|d| d.join("auth.json"))
}

pub fn state_vscdb_path() -> Option<PathBuf> {
    cursor_roaming_dir().map(|d| d.join("User").join("globalStorage").join("state.vscdb"))
}

pub fn profiles_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ax").join("cursor-auth"))
}

fn manifest_path() -> Option<PathBuf> {
    profiles_dir().map(|d| d.join("manifest.json"))
}

fn profile_path(name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    profiles_dir()
        .map(|d| d.join(format!("{name}.json")))
        .ok_or_else(|| "no home directory".into())
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name cannot be empty".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("profile name may only contain letters, numbers, hyphens, and underscores".into());
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn cursor_process_running() -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq Cursor.exe", "/NH"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| l.to_ascii_lowercase().contains("cursor.exe"))
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .arg("-x")
            .arg("Cursor")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

fn open_vscdb(path: &Path) -> Result<Connection, String> {
    Connection::open(path).map_err(|e| format!("open state.vscdb: {e}"))
}

fn read_vscdb_keys(conn: &Connection) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for key in AUTH_KEYS {
        let val: Result<String, rusqlite::Error> = conn.query_row(
            "SELECT value FROM ItemTable WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );
        match val {
            Ok(v) => {
                out.insert((*key).into(), v);
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                out.insert((*key).into(), String::new());
            }
            Err(e) => return Err(format!("read {key}: {e}")),
        }
    }
    Ok(out)
}

fn write_vscdb_keys(conn: &Connection, keys: &HashMap<String, String>) -> Result<(), String> {
    for key in AUTH_KEYS {
        let Some(val) = keys.get(*key) else {
            continue;
        };
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, val],
        )
        .map_err(|e| format!("write {key}: {e}"))?;
    }
    Ok(())
}

fn read_auth_json(path: &Path) -> Result<AuthJsonFile, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read auth.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse auth.json: {e}"))
}

fn write_auth_json(path: &Path, auth: &AuthJsonFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(auth).map_err(|e| e.to_string())? + "\n";
    fs::write(path, text).map_err(|e| format!("write auth.json: {e}"))
}

pub fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let mut padded = payload.replace('-', "+").replace('_', "/");
    match padded.len() % 4 {
        2 => padded.push_str("=="),
        3 => padded.push('='),
        _ => {}
    }
    let bytes = base64_decode(&padded).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    // Minimal base64 decoder — avoids extra dependency.
    const TABLE: [i8; 128] = {
        let mut t = [-1i8; 128];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i as i8;
            i += 1;
        }
        let mut j = 0u8;
        while j < 26 {
            t[(b'a' + j) as usize] = (26 + j) as i8;
            j += 1;
        }
        let mut k = 0u8;
        while k < 10 {
            t[(b'0' + k) as usize] = (52 + k) as i8;
            k += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };

    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        let val = if b < 128 { TABLE[b as usize] } else { -1 };
        if val < 0 {
            continue;
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn jwt_subject(token: &str) -> Option<String> {
    decode_jwt_payload(token)
        .and_then(|v| v.get("sub").and_then(|s| s.as_str()).map(str::to_string))
}

pub fn jwt_issued_at(token: &str) -> Option<u64> {
    decode_jwt_payload(token)
        .and_then(|v| v.get("time").and_then(|t| t.as_str()).and_then(|s| s.parse().ok()))
}

fn snapshot_from_parts(
    vscdb_keys: HashMap<String, String>,
    auth_json: AuthJsonFile,
    label: Option<String>,
) -> CursorAuthSnapshot {
    CursorAuthSnapshot {
        version: 1,
        saved_at: now_unix(),
        label,
        email: vscdb_keys
            .get("cursorAuth/cachedEmail")
            .cloned()
            .unwrap_or_default(),
        membership: vscdb_keys
            .get("cursorAuth/stripeMembershipType")
            .cloned()
            .unwrap_or_default(),
        subscription_status: vscdb_keys
            .get("cursorAuth/stripeSubscriptionStatus")
            .cloned()
            .unwrap_or_default(),
        sign_up_type: vscdb_keys
            .get("cursorAuth/cachedSignUpType")
            .cloned()
            .unwrap_or_default(),
        scoped_profile: vscdb_keys
            .get("cursorAuth/cachedScopedProfile")
            .cloned()
            .unwrap_or_default(),
        auth_json,
        vscdb_keys,
    }
}

pub fn read_live_snapshot() -> Result<CursorAuthSnapshot, String> {
    let vscdb = state_vscdb_path().ok_or("no Cursor data directory")?;
    if !vscdb.exists() {
        return Err(format!("state.vscdb not found: {}", vscdb.display()));
    }
    let conn = open_vscdb(&vscdb)?;
    let vscdb_keys = read_vscdb_keys(&conn)?;

    let auth_path = auth_json_path().ok_or("no Cursor data directory")?;
    let auth_json = if auth_path.exists() {
        read_auth_json(&auth_path)?
    } else {
        let access = vscdb_keys
            .get("cursorAuth/accessToken")
            .cloned()
            .ok_or("no access token in state.vscdb or auth.json")?;
        let refresh = vscdb_keys
            .get("cursorAuth/refreshToken")
            .cloned()
            .unwrap_or_else(|| access.clone());
        AuthJsonFile {
            access_token: access,
            refresh_token: refresh,
        }
    };

    Ok(snapshot_from_parts(vscdb_keys, auth_json, None))
}

pub fn read_legacy_auth_json_snapshot(label: Option<String>) -> Result<CursorAuthSnapshot, String> {
    let auth_path = auth_json_path().ok_or("no Cursor data directory")?;
    let auth_json = read_auth_json(&auth_path)?;
    let mut vscdb_keys = HashMap::new();
    vscdb_keys.insert(
        "cursorAuth/accessToken".into(),
        auth_json.access_token.clone(),
    );
    vscdb_keys.insert(
        "cursorAuth/refreshToken".into(),
        auth_json.refresh_token.clone(),
    );

    if let Some(sub) = jwt_subject(&auth_json.access_token) {
        if sub.starts_with("github|") {
            vscdb_keys.insert("cursorAuth/cachedSignUpType".into(), "Github".into());
        } else if sub.starts_with("auth0|") {
            vscdb_keys.insert("cursorAuth/cachedSignUpType".into(), "Auth_0".into());
        }
    }

    Ok(snapshot_from_parts(vscdb_keys, auth_json, label))
}

pub fn write_live_snapshot(snapshot: &CursorAuthSnapshot) -> Result<(), String> {
    let vscdb = state_vscdb_path().ok_or("no Cursor data directory")?;
    if !vscdb.exists() {
        return Err(format!("state.vscdb not found: {}", vscdb.display()));
    }
    let conn = open_vscdb(&vscdb)?;
    write_vscdb_keys(&conn, &snapshot.vscdb_keys)?;

    if let Some(path) = auth_json_path() {
        write_auth_json(&path, &snapshot.auth_json)?;
    }
    Ok(())
}

fn load_manifest() -> Manifest {
    let Some(path) = manifest_path() else {
        return Manifest::default();
    };
    if !path.exists() {
        return Manifest::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_manifest(manifest: &Manifest) -> Result<(), String> {
    let dir = profiles_dir().ok_or("no home directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = manifest_path().ok_or("no home directory")?;
    fs::write(
        &path,
        serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())
}

pub fn save_profile(name: &str, mut snapshot: CursorAuthSnapshot) -> Result<SavedProfileMeta, String> {
    validate_name(name)?;
    let dir = profiles_dir().ok_or("no home directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    snapshot.saved_at = now_unix();
    let path = profile_path(name)?;
    fs::write(
        &path,
        serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    let mut manifest = load_manifest();
    if !manifest.profiles.iter().any(|p| p == name) {
        manifest.profiles.push(name.to_string());
        manifest.profiles.sort();
    }
    manifest.active = Some(name.to_string());
    save_manifest(&manifest)?;

    Ok(SavedProfileMeta {
        name: name.to_string(),
        label: snapshot.label.clone().unwrap_or_else(|| name.to_string()),
        email: snapshot.email.clone(),
        membership: snapshot.membership.clone(),
        saved_at: snapshot.saved_at,
    })
}

pub fn load_profile(name: &str) -> Result<CursorAuthSnapshot, String> {
    validate_name(name)?;
    let path = profile_path(name)?;
    if !path.exists() {
        return Err(format!("profile '{name}' not found — run `ax cursor auth save {name}` first"));
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("parse profile: {e}"))
}

pub fn list_profiles() -> Result<Vec<SavedProfileMeta>, String> {
    let manifest = load_manifest();
    let mut out = Vec::new();
    for name in &manifest.profiles {
        if let Ok(snapshot) = load_profile(name) {
            out.push(SavedProfileMeta {
                name: name.clone(),
                label: snapshot.label.clone().unwrap_or_else(|| name.clone()),
                email: snapshot.email,
                membership: snapshot.membership,
                saved_at: snapshot.saved_at,
            });
        }
    }
    Ok(out)
}

pub fn active_profile_name() -> Option<String> {
    load_manifest().active
}

pub fn use_profile(name: &str, force: bool) -> Result<CursorAuthSnapshot, String> {
    validate_name(name)?;
    if cursor_process_running() && !force {
        return Err(
            "Cursor is running — close Cursor first, or pass --force (restart Cursor after switch)"
                .into(),
        );
    }
    let snapshot = load_profile(name)?;
    write_live_snapshot(&snapshot)?;

    let mut manifest = load_manifest();
    manifest.active = Some(name.to_string());
    save_manifest(&manifest)?;

    Ok(snapshot)
}

pub fn enrich_snapshot_metadata(
    snapshot: &mut CursorAuthSnapshot,
    email: Option<&str>,
    membership: Option<&str>,
    subscription_status: Option<&str>,
    sign_up_type: Option<&str>,
) {
    if let Some(v) = email {
        snapshot.email = v.to_string();
        snapshot
            .vscdb_keys
            .insert("cursorAuth/cachedEmail".into(), v.to_string());
    }
    if let Some(v) = membership {
        snapshot.membership = v.to_string();
        snapshot
            .vscdb_keys
            .insert("cursorAuth/stripeMembershipType".into(), v.to_string());
    }
    if let Some(v) = subscription_status {
        snapshot.subscription_status = v.to_string();
        snapshot
            .vscdb_keys
            .insert("cursorAuth/stripeSubscriptionStatus".into(), v.to_string());
    }
    if let Some(v) = sign_up_type {
        snapshot.sign_up_type = v.to_string();
        snapshot
            .vscdb_keys
            .insert("cursorAuth/cachedSignUpType".into(), v.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_sample_jwt_payload() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dGVzdCIsInRpbWUiOiIxMjM0NTY3ODkifQ.sig";
        let payload = decode_jwt_payload(token).expect("payload");
        assert_eq!(payload["sub"], "github|test");
        assert_eq!(payload["time"], "123456789");
    }

    #[test]
    fn validate_profile_names() {
        assert!(validate_name("enterprise").is_ok());
        assert!(validate_name("pro-plus").is_ok());
        assert!(validate_name("bad name").is_err());
    }
}
