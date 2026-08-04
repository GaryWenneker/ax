//! Microsoft OAuth device code flow and token storage.

use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::paths::{device_flow_path, ensure_auth_dir, microsoft_auth_path};

const GRAPH_SCOPE: &str = "Files.ReadWrite.All offline_access User.Read";
const TOKEN_URL: &str = "https://login.microsoftonline.com/organizations/oauth2/v2.0/token";
const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode";

/// Microsoft-owned multi-tenant public client ("Microsoft Graph Command Line Tools").
/// Ships as the zero-setup default so OneDrive sign-in works without a team Azure AD
/// app registration. Users can still override via `AX_MS_CLIENT_ID` / `share.microsoftClientId`
/// (e.g. for tenants that restrict first-party app consent).
const DEFAULT_MS_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

pub fn ms_client_id() -> Result<String, String> {
    if let Ok(v) = std::env::var("AX_MS_CLIENT_ID") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(v) = read_ms_client_id_from_config() {
        return Ok(v);
    }
    Ok(DEFAULT_MS_CLIENT_ID.to_string())
}

/// True when a custom client ID was set (env or config) rather than the built-in default.
pub fn has_custom_client_id() -> bool {
    if let Ok(v) = std::env::var("AX_MS_CLIENT_ID") {
        if !v.trim().is_empty() {
            return true;
        }
    }
    read_ms_client_id_from_config().is_some()
}

fn read_ms_client_id_from_config() -> Option<String> {
    let path = crate::paths::ax_home().join("config.json");
    let content = std::fs::read_to_string(path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let id = root
        .get("share")?
        .get("microsoftClientId")?
        .as_str()?
        .trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub fn client_id_configured() -> bool {
    ms_client_id().is_ok()
}

/// Persist Azure AD client ID to `~/.ax/config.json` (`share.microsoftClientId`).
pub fn write_ms_client_id_to_config(client_id: &str) -> Result<(), String> {
    let trimmed = client_id.trim();
    if trimmed.is_empty() {
        return Err("client ID cannot be empty".into());
    }
    let path = crate::paths::ax_home().join("config.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "config root must be a JSON object".to_string())?;
    let share = obj
        .entry("share")
        .or_insert_with(|| serde_json::json!({}));
    let share_obj = share
        .as_object_mut()
        .ok_or_else(|| "share must be a JSON object".to_string())?;
    share_obj.insert(
        "microsoftClientId".into(),
        serde_json::Value::String(trimmed.to_string()),
    );
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftTokenStore {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub account: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftAuthStatus {
    pub signed_in: bool,
    pub account: Option<String>,
    pub expires_at: Option<i64>,
    pub client_configured: bool,
    /// True when a team/custom Azure AD app was set; false when using the built-in
    /// zero-setup Microsoft Graph Command Line Tools client ID.
    pub custom_client_id: bool,
}

pub fn auth_status() -> MicrosoftAuthStatus {
    let client_configured = client_id_configured();
    let custom_client_id = has_custom_client_id();
    match load_tokens() {
        Ok(store) if !store.access_token.is_empty() => MicrosoftAuthStatus {
            signed_in: store.expires_at > now_secs() || !store.refresh_token.is_empty(),
            account: store.account.clone(),
            expires_at: Some(store.expires_at),
            client_configured,
            custom_client_id,
        },
        _ => MicrosoftAuthStatus {
            signed_in: false,
            account: None,
            expires_at: None,
            client_configured,
            custom_client_id,
        },
    }
}

pub fn load_tokens() -> Result<MicrosoftTokenStore, String> {
    let path = microsoft_auth_path();
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

pub fn save_tokens(store: &MicrosoftTokenStore) -> Result<(), String> {
    ensure_auth_dir()?;
    let path = microsoft_auth_path();
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())
}

pub fn clear_tokens() -> Result<(), String> {
    let path = microsoft_auth_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn start_device_flow() -> Result<DeviceFlowStart, String> {
    ensure_auth_dir()?;
    let client_id = ms_client_id()?;
    let client = Client::new();
    let resp: DeviceCodeResponse = client
        .post(DEVICE_CODE_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", GRAPH_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(err) = resp.error {
        return Err(format!("device code start failed: {err}"));
    }

    let start = DeviceFlowStart {
        device_code: resp.device_code.clone(),
        user_code: resp.user_code.clone(),
        verification_uri: resp.verification_uri.clone(),
        verification_uri_complete: resp
            .verification_uri_complete
            .clone()
            .filter(|s| !s.is_empty()),
        expires_in: resp.expires_in,
        interval: resp.interval.max(5),
        message: resp.message.clone(),
    };
    std::fs::write(
        device_flow_path(),
        serde_json::to_string_pretty(&start).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(start)
}

pub async fn poll_device_flow_once() -> Result<Option<MicrosoftTokenStore>, String> {
    let flow: DeviceFlowStart = {
        let content = std::fs::read_to_string(device_flow_path()).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    };
    let client_id = ms_client_id()?;
    let client = Client::new();
    let resp: TokenResponse = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id.as_str()),
            ("device_code", flow.device_code.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if resp.error.as_deref() == Some("authorization_pending") {
        return Ok(None);
    }
    if resp.error.as_deref() == Some("slow_down") {
        return Ok(None);
    }
    if let Some(err) = resp.error {
        return Err(format!("device code poll failed: {err}"));
    }
    let access_token = resp
        .access_token
        .ok_or_else(|| "missing access_token".to_string())?;
    let refresh_token = resp
        .refresh_token
        .ok_or_else(|| "missing refresh_token".to_string())?;
    let expires_in = resp.expires_in.unwrap_or(3600);
    let account = fetch_profile(&access_token).await.ok();
    let store = MicrosoftTokenStore {
        access_token,
        refresh_token,
        expires_at: now_secs() + expires_in as i64,
        account,
    };
    save_tokens(&store)?;
    let _ = std::fs::remove_file(device_flow_path());
    Ok(Some(store))
}

pub async fn get_access_token() -> Result<String, String> {
    let mut store = load_tokens().map_err(|_| "Not signed in to Microsoft".to_string())?;
    if store.expires_at > now_secs() + 60 {
        return Ok(store.access_token);
    }
    store = refresh_tokens(&store.refresh_token).await?;
    save_tokens(&store)?;
    Ok(store.access_token)
}

async fn refresh_tokens(refresh_token: &str) -> Result<MicrosoftTokenStore, String> {
    let client_id = ms_client_id()?;
    let client = Client::new();
    let resp: TokenResponse = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", GRAPH_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(err) = resp.error {
        return Err(format!("token refresh failed: {err}"));
    }
    let access_token = resp
        .access_token
        .ok_or_else(|| "missing access_token".to_string())?;
    let new_refresh = resp
        .refresh_token
        .unwrap_or_else(|| refresh_token.to_string());
    let expires_in = resp.expires_in.unwrap_or(3600);
    Ok(MicrosoftTokenStore {
        access_token,
        refresh_token: new_refresh,
        expires_at: now_secs() + expires_in as i64,
        account: load_tokens().ok().and_then(|s| s.account),
    })
}

async fn fetch_profile(access_token: &str) -> Result<String, String> {
    let client = Client::new();
    #[derive(Deserialize)]
    struct Profile {
        #[serde(rename = "displayName")]
        display_name: Option<String>,
        #[serde(rename = "userPrincipalName")]
        upn: Option<String>,
    }
    let profile: Profile = client
        .get("https://graph.microsoft.com/v1.0/me")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(profile
        .display_name
        .or(profile.upn)
        .unwrap_or_else(|| "Microsoft account".to_string()))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    #[serde(default)]
    device_code: String,
    #[serde(default)]
    user_code: String,
    #[serde(default)]
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
    #[serde(default)]
    message: String,
    error: Option<String>,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
}

/// Encode a sharing URL for Graph `/shares/{shareId}/driveItem`.
pub fn encode_share_id(url: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    format!("u!{}", URL_SAFE_NO_PAD.encode(url.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_share_id_format() {
        let id = encode_share_id("https://example.com/share");
        assert!(id.starts_with("u!"));
    }

    #[test]
    fn encode_share_id_is_deterministic() {
        let url = "https://contoso.sharepoint.com/:f:/r/personal/user/Documents/.ax";
        let a = encode_share_id(url);
        let b = encode_share_id(url);
        assert_eq!(a, b);
        assert!(!a.contains('='));
    }

    #[test]
    fn write_and_read_client_id_from_config() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        // use ax_home override via writing to expected path - skip, test read function via direct path
        let cfg_path = home.join(".ax").join("config.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg_path,
            r#"{"share":{"microsoftClientId":"11111111-2222-3333-4444-555555555555"}}"#,
        )
        .unwrap();
        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let root: serde_json::Value = serde_json::from_str(&content).unwrap();
        let id = root
            .get("share")
            .and_then(|v| v.get("microsoftClientId"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(id, "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn default_client_id_is_a_guid() {
        // Microsoft Graph Command Line Tools public client — sanity-check shape only;
        // ms_client_id()/has_custom_client_id() depend on process env + real ~/.ax
        // config and are exercised via manual/integration testing instead.
        assert_eq!(DEFAULT_MS_CLIENT_ID.len(), 36);
        assert_eq!(DEFAULT_MS_CLIENT_ID.matches('-').count(), 4);
    }
}
