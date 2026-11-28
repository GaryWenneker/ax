//! `ax cursor auth` — save and restore Cursor subscription sessions.

use ax_agent::{
    active_profile_name, cursor_process_running, enrich_snapshot_metadata, jwt_issued_at,
    jwt_subject, list_cursor_auth_profiles, load_cursor_auth_profile,
    read_legacy_auth_json_snapshot, read_live_snapshot, save_cursor_auth_profile,
    use_cursor_auth_profile, CursorAuthSnapshot,
};

pub fn run_status(json: bool) -> Result<(), String> {
    let live = read_live_snapshot()?;
    let active = active_profile_name();
    let token = live
        .vscdb_keys
        .get("cursorAuth/accessToken")
        .cloned()
        .unwrap_or_else(|| live.auth_json.access_token.clone());
    let subject = jwt_subject(&token).unwrap_or_default();
    let issued = jwt_issued_at(&token);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "email": live.email,
                "membership": live.membership,
                "subscriptionStatus": live.subscription_status,
                "signUpType": live.sign_up_type,
                "subject": subject,
                "issuedAt": issued,
                "activeProfile": active,
                "cursorRunning": cursor_process_running(),
            }))
            .unwrap_or_default()
        );
        return Ok(());
    }

    println!("Cursor auth (live)");
    println!("  email:      {}", live.email);
    println!("  plan:       {}", live.membership);
    println!("  status:     {}", live.subscription_status);
    println!("  login:      {}", live.sign_up_type);
    println!("  subject:    {subject}");
    if let Some(ts) = issued {
        println!("  token from: {ts} (unix)");
    }
    if let Some(name) = active {
        println!("  ax profile: {name} (last applied)");
    }
    if cursor_process_running() {
        println!("  note:       Cursor.exe is running");
    }
    Ok(())
}

pub fn run_list(json: bool) -> Result<(), String> {
    let profiles = list_cursor_auth_profiles()?;
    let active = active_profile_name();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "active": active,
                "profiles": profiles,
            }))
            .unwrap_or_default()
        );
        return Ok(());
    }

    if profiles.is_empty() {
        println!("No saved profiles. Run: ax cursor auth save <name>");
        return Ok(());
    }

    println!("Saved Cursor auth profiles:");
    for p in profiles {
        let mark = if active.as_deref() == Some(p.name.as_str()) {
            " *"
        } else {
            ""
        };
        println!("  {}{} — {} ({})", p.name, mark, p.email, p.membership);
    }
    Ok(())
}

pub fn run_save(
    name: String,
    label: Option<String>,
    from_auth_json: bool,
    email: Option<String>,
    membership: Option<String>,
    subscription_status: Option<String>,
    sign_up_type: Option<String>,
) -> Result<(), String> {
    let mut snapshot = if from_auth_json {
        read_legacy_auth_json_snapshot(label.clone())?
    } else {
        let mut s = read_live_snapshot()?;
        s.label = label.clone();
        s
    };

    enrich_snapshot_metadata(
        &mut snapshot,
        email.as_deref(),
        membership.as_deref(),
        subscription_status.as_deref(),
        sign_up_type.as_deref(),
    );
    if snapshot.label.is_none() {
        snapshot.label = label.or_else(|| Some(name.clone()));
    }

    let meta = save_cursor_auth_profile(&name, snapshot)?;
    println!(
        "Saved profile '{}' — {} ({})",
        meta.name, meta.email, meta.membership
    );
    Ok(())
}

pub fn run_use(name: String, force: bool, json: bool) -> Result<(), String> {
    let snapshot = use_cursor_auth_profile(&name, force)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "profile": name,
                "email": snapshot.email,
                "membership": snapshot.membership,
                "restartRequired": true,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Applied profile '{name}' — {} ({})",
            snapshot.email, snapshot.membership
        );
        println!("Restart Cursor to pick up the new session.");
    }
    Ok(())
}

pub fn run_show(name: String, json: bool) -> Result<(), String> {
    let snapshot = load_cursor_auth_profile(&name)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&show_json(&name, &snapshot)).unwrap_or_default()
        );
    } else {
        println!("Profile '{name}'");
        println!("  email:      {}", snapshot.email);
        println!("  plan:       {}", snapshot.membership);
        println!("  status:     {}", snapshot.subscription_status);
        println!("  login:      {}", snapshot.sign_up_type);
        if let Some(label) = snapshot.label {
            println!("  label:      {label}");
        }
    }
    Ok(())
}

/// The JSON view of a saved profile for `ax cursor auth show --json`.
///
/// Built field by field so the saved access and refresh tokens never reach stdout.
fn show_json(name: &str, snapshot: &CursorAuthSnapshot) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "email": snapshot.email,
        "membership": snapshot.membership,
        "subscriptionStatus": snapshot.subscription_status,
        "signUpType": snapshot.sign_up_type,
        "label": snapshot.label,
        "savedAt": snapshot.saved_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_agent::cursor_auth::AuthJsonFile;
    use std::collections::HashMap;

    #[test]
    fn show_json_never_contains_tokens() {
        let mut keys = HashMap::new();
        keys.insert(
            "cursorAuth/accessToken".to_string(),
            "VSCDB-ACCESS-SECRET".to_string(),
        );
        keys.insert(
            "cursorAuth/refreshToken".to_string(),
            "VSCDB-REFRESH-SECRET".to_string(),
        );
        let snapshot = CursorAuthSnapshot {
            version: 1,
            saved_at: 42,
            label: Some("work".into()),
            email: "dev@example.com".into(),
            membership: "pro".into(),
            subscription_status: "active".into(),
            sign_up_type: "Auth_0".into(),
            scoped_profile: "default".into(),
            auth_json: AuthJsonFile {
                access_token: "JSON-ACCESS-SECRET".into(),
                refresh_token: "JSON-REFRESH-SECRET".into(),
            },
            vscdb_keys: keys,
        };
        let value = show_json("work", &snapshot);
        let text = value.to_string();
        for secret in [
            "JSON-ACCESS-SECRET",
            "JSON-REFRESH-SECRET",
            "VSCDB-ACCESS-SECRET",
            "VSCDB-REFRESH-SECRET",
        ] {
            assert!(!text.contains(secret), "token leaked: {text}");
        }
        assert_eq!(value["name"], "work");
        assert_eq!(value["email"], "dev@example.com");
        assert_eq!(value["membership"], "pro");
        assert_eq!(value["subscriptionStatus"], "active");
        assert_eq!(value["signUpType"], "Auth_0");
        assert_eq!(value["label"], "work");
        assert_eq!(value["savedAt"], 42);
    }
}
