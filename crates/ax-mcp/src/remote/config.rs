use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConfig {
    pub public_url: String,
    pub oauth: OAuthConfig,
    pub projects: Vec<Project>,
    #[serde(default)]
    pub api_keys: Vec<ApiKey>,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// The header the proxy in front of Ax sets to the caller's address. Only
    /// with this set are the unauthenticated OAuth limits shared out per
    /// address; without it every caller shares one budget, since a caller can
    /// set either header itself when no proxy overwrites it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_address_header: Option<ClientAddressHeader>,
}

/// A request header that names the caller's address, set by a trusted proxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ClientAddressHeader {
    /// Cloudflare's `CF-Connecting-IP`, which Cloudflare always overwrites.
    #[serde(rename = "cf-connecting-ip")]
    CfConnectingIp,
    /// The last `X-Forwarded-For` entry, which the nearest proxy appends.
    #[serde(rename = "x-forwarded-for")]
    XForwardedFor,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthConfig {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub built_in: bool,
    pub issuer: String,
    pub jwks_uri: String,
    /// How many seconds the provider's clock may differ from this host's when
    /// a JWT's `exp`, `nbf` and `iat` are checked. `0` checks them exactly.
    #[serde(
        default = "default_clock_skew_seconds",
        skip_serializing_if = "is_default_clock_skew"
    )]
    pub clock_skew_seconds: u64,
}

/// The JWT clock leeway when the configuration names none.
pub const DEFAULT_CLOCK_SKEW_SECONDS: u64 = 60;
/// The largest leeway accepted; more would keep expired tokens usable.
pub const MAX_CLOCK_SKEW_SECONDS: u64 = 300;

fn default_clock_skew_seconds() -> u64 {
    DEFAULT_CLOCK_SKEW_SECONDS
}

fn is_default_clock_skew(seconds: &u64) -> bool {
    *seconds == DEFAULT_CLOCK_SKEW_SECONDS
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub root: PathBuf,
    pub subjects: Vec<String>,
    #[serde(default)]
    pub write_subjects: Vec<String>,
    /// Subjects that may call the remote admin tools. Must also be write subjects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admin_subjects: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKey {
    pub id: String,
    /// SHA-256 hex, never the original key. API keys are for IDE clients only.
    pub sha256: String,
    pub projects: Vec<String>,
    #[serde(default)]
    pub write: bool,
    /// Grants the remote admin tools (index, sync, logs). Requires `write`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub admin: bool,
}

pub use ax_remote_client::https_url;

impl RemoteConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut config: Self = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&mut self) -> Result<(), String> {
        let public = https_url(&self.public_url)?;
        if public.path() != "/" {
            return Err("public_url must be an HTTPS origin".into());
        }
        self.public_url = public.origin().ascii_serialization();
        https_url(&self.oauth.issuer)?;
        https_url(&self.oauth.jwks_uri)?;
        if self.oauth.clock_skew_seconds > MAX_CLOCK_SKEW_SECONDS {
            return Err(format!(
                "oauth.clock_skew_seconds may be at most {MAX_CLOCK_SKEW_SECONDS}"
            ));
        }
        if self.oauth.built_in
            && (self.oauth.issuer != format!("{}/oauth", self.public_url)
                || self.oauth.jwks_uri != format!("{}/oauth/jwks", self.public_url))
        {
            return Err("Built-in OAuth endpoints must match the public origin".into());
        }
        if self.projects.is_empty() {
            return Err("Register at least one project".into());
        }
        let mut ids = HashSet::new();
        for p in &mut self.projects {
            if p.id.is_empty()
                || p.id.len() > 64
                || !p
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                || !ids.insert(p.id.clone())
            {
                return Err("Project IDs must be unique URL-safe identifiers".into());
            }
            p.root = p
                .root
                .canonicalize()
                .map_err(|_| format!("Project {} root does not exist", p.id))?;
            if !p.root.join(".ax/ax.db").is_file() {
                return Err(format!("Project {} needs ax init", p.id));
            }
            if p.subjects.is_empty() || p.subjects.iter().any(|s| s.is_empty()) {
                return Err(format!("Project {} needs explicit OAuth subjects", p.id));
            }
            if p.write_subjects.iter().any(|s| !p.subjects.contains(s)) {
                return Err("Write subjects must also have context access".into());
            }
            if p.admin_subjects
                .iter()
                .any(|s| !p.write_subjects.contains(s))
            {
                return Err("Admin subjects must also have write access".into());
            }
        }
        let mut keys = HashSet::new();
        for key in &self.api_keys {
            if key.id.is_empty()
                || !keys.insert(&key.id)
                || key.sha256.len() != 64
                || hex::decode(&key.sha256).is_err()
                || key.projects.is_empty()
                || key.projects.iter().any(|p| !ids.contains(p))
            {
                return Err("Invalid API key hash, ID or project grant".into());
            }
            if key.admin && !key.write {
                return Err("Admin API keys must also have write access".into());
            }
        }
        // Store each origin the way a browser sends it in the Origin header
        // (lowercase host, default port dropped, no trailing slash).
        for origin in &mut self.allowed_origins {
            let u = https_url(origin)?;
            if u.path() != "/" {
                return Err("Allowed origins must be HTTPS origins".into());
            }
            *origin = u.origin().ascii_serialization();
        }
        Ok(())
    }

    pub fn metadata_url(&self) -> String {
        format!("{}/.well-known/oauth-protected-resource", self.public_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_origin_is_canonical_but_oauth_issuer_remains_exact() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".ax")).unwrap();
        std::fs::write(temp.path().join(".ax/ax.db"), "").unwrap();
        let mut config: RemoteConfig = serde_json::from_value(serde_json::json!({"public_url":"HTTPS://AX.example.com:443/","oauth":{"issuer":"https://Identity.Example/tenant/","jwks_uri":"https://Identity.Example/jwks"},"projects":[{"id":"demo","root":temp.path(),"subjects":["subject"]}]})).unwrap();
        config.validate().unwrap();
        assert_eq!(config.public_url, "https://ax.example.com");
        assert_eq!(config.oauth.issuer, "https://Identity.Example/tenant/");
        assert_eq!(config.projects[0].subjects, ["subject"]);
    }

    #[test]
    fn allowed_origins_are_stored_as_the_origin_a_browser_sends() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".ax")).unwrap();
        std::fs::write(temp.path().join(".ax/ax.db"), "").unwrap();
        let config = |origins: serde_json::Value| -> RemoteConfig {
            serde_json::from_value(serde_json::json!({"public_url":"https://ax.example.com","allowed_origins":origins,"oauth":{"issuer":"https://id.example/","jwks_uri":"https://id.example/jwks"},"projects":[{"id":"demo","root":temp.path(),"subjects":["subject"]}]})).unwrap()
        };
        let mut mixed = config(serde_json::json!([
            "https://App.Example.com",
            "https://app.example.com:443/",
            "https://tools.example.com:8443"
        ]));
        mixed.validate().unwrap();
        assert_eq!(
            mixed.allowed_origins,
            [
                "https://app.example.com",
                "https://app.example.com",
                "https://tools.example.com:8443"
            ]
        );
        for bad in [
            "https://app.example.com/path",
            "https://app.example.com/?q=1",
            "https://app.example.com/#frag",
            "http://app.example.com",
        ] {
            assert!(config(serde_json::json!([bad])).validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_clock_skew_defaults_to_a_minute_and_has_an_upper_bound() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".ax")).unwrap();
        std::fs::write(temp.path().join(".ax/ax.db"), "").unwrap();
        let config = |oauth: serde_json::Value| -> RemoteConfig {
            serde_json::from_value(serde_json::json!({"public_url":"https://ax.example.com","oauth":oauth,"projects":[{"id":"demo","root":temp.path(),"subjects":["subject"]}]})).unwrap()
        };
        let plain = config(
            serde_json::json!({"issuer":"https://id.example/","jwks_uri":"https://id.example/jwks"}),
        );
        assert_eq!(plain.oauth.clock_skew_seconds, DEFAULT_CLOCK_SKEW_SECONDS);
        let exact = config(
            serde_json::json!({"issuer":"https://id.example/","jwks_uri":"https://id.example/jwks","clock_skew_seconds":0}),
        );
        assert_eq!(exact.oauth.clock_skew_seconds, 0);
        let mut loose = config(
            serde_json::json!({"issuer":"https://id.example/","jwks_uri":"https://id.example/jwks","clock_skew_seconds":MAX_CLOCK_SKEW_SECONDS + 1}),
        );
        assert!(loose.validate().is_err());
    }

    #[test]
    fn the_client_address_header_is_named_explicitly_or_left_out() {
        let config = |extra: serde_json::Value| -> Result<RemoteConfig, serde_json::Error> {
            let mut value = serde_json::json!({"public_url":"https://ax.example.com","oauth":{"issuer":"https://id.example/","jwks_uri":"https://id.example/jwks"},"projects":[]});
            for (name, v) in extra.as_object().unwrap() {
                value[name] = v.clone();
            }
            serde_json::from_value(value)
        };
        let plain = config(serde_json::json!({})).unwrap();
        assert_eq!(plain.client_address_header, None);
        assert!(!serde_json::to_string(&plain)
            .unwrap()
            .contains("client_address_header"));
        assert_eq!(
            config(serde_json::json!({"client_address_header":"cf-connecting-ip"}))
                .unwrap()
                .client_address_header,
            Some(ClientAddressHeader::CfConnectingIp)
        );
        assert_eq!(
            config(serde_json::json!({"client_address_header":"x-forwarded-for"}))
                .unwrap()
                .client_address_header,
            Some(ClientAddressHeader::XForwardedFor)
        );
        assert!(config(serde_json::json!({"client_address_header":"x-real-ip"})).is_err());
    }
}
