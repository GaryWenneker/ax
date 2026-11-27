use super::config::{Project, RemoteConfig};
use axum::http::{header, HeaderMap};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

/// Extract a bearer token from `Authorization`. A present but non-Bearer header is malformed,
/// not "missing", so callers must not treat it as an unauthenticated request.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    if token.is_empty() || token.bytes().any(|b| b.is_ascii_whitespace()) {
        return None;
    }
    Some(token)
}

pub const CONTEXT_SCOPE: &str = "ax:context";
pub const WRITE_SCOPE: &str = "ax:write";
pub const ADMIN_SCOPE: &str = "ax:admin";

#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub subject: Option<String>,
    pub scopes: HashSet<String>,
    pub key_projects: Vec<String>,
    pub key_write: bool,
    pub key_admin: bool,
}

impl Principal {
    pub fn permits(&self, project: &Project) -> bool {
        self.scopes.contains(CONTEXT_SCOPE)
            && match &self.subject {
                Some(sub) => project.subjects.contains(sub),
                None => self.key_projects.contains(&project.id),
            }
    }
    pub fn can_write(&self, project: &Project) -> bool {
        self.permits(project)
            && self.scopes.contains(WRITE_SCOPE)
            && match &self.subject {
                Some(sub) => project.write_subjects.contains(sub),
                None => self.key_write,
            }
    }
    /// Admin tools need write access, the admin scope and an explicit admin grant.
    pub fn can_admin(&self, project: &Project) -> bool {
        self.can_write(project)
            && self.scopes.contains(ADMIN_SCOPE)
            && match &self.subject {
                Some(sub) => project.admin_subjects.contains(sub),
                None => self.key_admin,
            }
    }
}

/// Age after which the cached JWKS is refreshed in the background.
const KEY_MAX_AGE: Duration = Duration::from_secs(300);
/// Least time between two JWKS fetches, so a failing or unknown-`kid` storm does
/// not hammer the identity provider.
const KEY_REFRESH_BACKOFF: Duration = Duration::from_secs(30);

/// The identity provider's signing keys and when they were last fetched.
struct KeyCache {
    set: JwkSet,
    fetched: Instant,
}

/// One JWKS fetch at a time, and when the last one started.
#[derive(Default)]
struct RefreshState {
    last_attempt: Option<Instant>,
}

#[derive(Clone)]
pub struct TokenVerifier {
    config: Arc<RemoteConfig>,
    keys: Arc<RwLock<KeyCache>>,
    refresh: Arc<tokio::sync::Mutex<RefreshState>>,
    client: reqwest::Client,
    built_in: Option<super::oauth::OAuthStore>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    scope: String,
}

impl TokenVerifier {
    pub async fn discover(config: Arc<RemoteConfig>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        if config.oauth.built_in {
            let store = super::oauth::OAuthStore::open(&config.public_url).await?;
            store.native_client().await?;
            let mut verifier = Self::with_keys(config, JwkSet { keys: vec![] }, client);
            verifier.built_in = Some(store);
            return Ok(verifier);
        }
        let keys = fetch_keys(&client, &config.oauth.jwks_uri).await?;
        Ok(Self::with_keys(config, keys, client))
    }
    pub fn with_keys(config: Arc<RemoteConfig>, keys: JwkSet, client: reqwest::Client) -> Self {
        Self {
            config,
            keys: Arc::new(RwLock::new(KeyCache {
                set: keys,
                fetched: Instant::now(),
            })),
            refresh: Arc::default(),
            client,
            built_in: None,
        }
    }
    pub fn oauth_store(&self) -> Option<super::oauth::OAuthStore> {
        self.built_in.clone()
    }
    pub async fn verify(&self, token: &str) -> Result<Principal, String> {
        if token.is_empty() || token.len() > 16384 {
            return Err("Invalid access token".into());
        }
        let hash = Sha256::digest(token.as_bytes());
        for key in &self.config.api_keys {
            let expected = hex::decode(&key.sha256).map_err(|_| "Invalid API key configuration")?;
            if bool::from(hash.as_slice().ct_eq(&expected)) {
                let mut scopes = HashSet::from([CONTEXT_SCOPE.to_string()]);
                if key.write {
                    scopes.insert(WRITE_SCOPE.to_string());
                }
                if key.admin {
                    scopes.insert(ADMIN_SCOPE.to_string());
                }
                return Ok(Principal {
                    id: format!("key:{}", key.id),
                    subject: None,
                    scopes,
                    key_projects: key.projects.clone(),
                    key_write: key.write,
                    key_admin: key.admin,
                });
            }
        }
        if let Some(store) = &self.built_in {
            return store.verify(token).await;
        }
        // Never follow a token's jku/x5u or accept symmetric algorithms.
        let header = decode_header(token).map_err(|_| "Invalid access token")?;
        if header.alg != Algorithm::RS256 {
            return Err("Unsupported token algorithm".into());
        }
        let kid = header.kid.ok_or("Token needs a signing key ID")?;
        let (stale, known) = {
            let keys = self.keys.read().await;
            (
                keys.fetched.elapsed() > KEY_MAX_AGE,
                keys.set.find(&kid).is_some(),
            )
        };
        if !known {
            // The provider may have rotated its key: fetch now, at most once per back-off.
            self.refresh_keys(false).await;
        } else if stale {
            // The cached keys still verify; refresh without making this request wait.
            let this = self.clone();
            tokio::spawn(async move { this.refresh_keys(true).await });
        }
        let key = {
            let keys = self.keys.read().await;
            let jwk = keys.set.find(&kid).ok_or("Unknown signing key")?;
            DecodingKey::from_jwk(jwk).map_err(|_| "Invalid signing key")?
        };
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 0;
        validation.validate_nbf = true;
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.set_issuer(&[&self.config.oauth.issuer]);
        validation.set_audience(&[&self.config.public_url]);
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|_| "Invalid or expired access token")?
            .claims;
        if claims.sub.is_empty() {
            return Err("Missing token subject".into());
        }
        Ok(Principal {
            id: format!("oauth:{}", claims.sub),
            subject: Some(claims.sub),
            scopes: claims.scope.split_whitespace().map(str::to_owned).collect(),
            key_projects: Vec::new(),
            key_write: false,
            key_admin: false,
        })
    }
}

impl TokenVerifier {
    /// Fetches the JWKS unless another fetch runs (`only_if_idle`) or one started
    /// within [`KEY_REFRESH_BACKOFF`]. The key cache lock is never held across the
    /// network call, and a failed fetch keeps the cached keys.
    async fn refresh_keys(&self, only_if_idle: bool) {
        let mut state = if only_if_idle {
            match self.refresh.try_lock() {
                Ok(state) => state,
                Err(_) => return,
            }
        } else {
            self.refresh.lock().await
        };
        if only_if_idle && self.keys.read().await.fetched.elapsed() <= KEY_MAX_AGE {
            return;
        }
        if state
            .last_attempt
            .is_some_and(|at| at.elapsed() < KEY_REFRESH_BACKOFF)
        {
            return;
        }
        state.last_attempt = Some(Instant::now());
        match fetch_keys(&self.client, &self.config.oauth.jwks_uri).await {
            Ok(set) => {
                *self.keys.write().await = KeyCache {
                    set,
                    fetched: Instant::now(),
                };
            }
            Err(e) => tracing::warn!("OAuth signing key refresh failed; keeping cached keys: {e}"),
        }
    }
}

async fn fetch_keys(client: &reqwest::Client, url: &str) -> Result<JwkSet, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Cannot reach OAuth signing keys")?
        .error_for_status()
        .map_err(|_| "Cannot fetch OAuth signing keys")?;
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        return Err("Signing key response too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Cannot read OAuth signing keys")?
    {
        if chunk.len() > (1024usize * 1024).saturating_sub(bytes.len()) {
            return Err("Signing key response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid OAuth signing keys".into())
}

#[cfg(test)]
mod admin_grant {
    use super::*;

    fn project(admin_subjects: &[&str]) -> Project {
        Project {
            id: "alpha".into(),
            root: "/tmp/alpha".into(),
            subjects: vec!["ann".into(), "bob".into()],
            write_subjects: vec!["ann".into(), "bob".into()],
            admin_subjects: admin_subjects.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn principal(
        subject: Option<&str>,
        scopes: &[&str],
        key_write: bool,
        key_admin: bool,
    ) -> Principal {
        Principal {
            id: "test".into(),
            subject: subject.map(str::to_owned),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            key_projects: vec!["alpha".into()],
            key_write,
            key_admin,
        }
    }

    const ALL: &[&str] = &[CONTEXT_SCOPE, WRITE_SCOPE, ADMIN_SCOPE];

    #[test]
    fn a_key_needs_the_scope_the_grant_and_write_access() {
        let project = project(&[]);
        assert!(principal(None, ALL, true, true).can_admin(&project));
        // The admin scope alone is not a grant.
        assert!(!principal(None, ALL, true, false).can_admin(&project));
        // The grant without the scope is not enough either.
        assert!(!principal(None, &[CONTEXT_SCOPE, WRITE_SCOPE], true, true).can_admin(&project));
        // Admin never exists without write access.
        assert!(!principal(None, ALL, false, true).can_admin(&project));
        assert!(!principal(None, &[CONTEXT_SCOPE, ADMIN_SCOPE], true, true).can_admin(&project));
        // A key for another project has no access at all.
        let mut other = principal(None, ALL, true, true);
        other.key_projects = vec!["beta".into()];
        assert!(!other.can_admin(&project));
    }

    #[test]
    fn a_subject_needs_the_scope_and_a_place_in_admin_subjects() {
        let project = project(&["ann"]);
        assert!(principal(Some("ann"), ALL, false, false).can_admin(&project));
        // bob may write but is not an admin subject.
        assert!(principal(Some("bob"), ALL, false, false).can_write(&project));
        assert!(!principal(Some("bob"), ALL, false, false).can_admin(&project));
        // ann without the admin scope in her token.
        assert!(
            !principal(Some("ann"), &[CONTEXT_SCOPE, WRITE_SCOPE], false, false)
                .can_admin(&project)
        );
        // The key flag means nothing for a subject.
        assert!(!principal(Some("bob"), ALL, true, true).can_admin(&project));
        // An unknown subject.
        assert!(!principal(Some("eve"), ALL, false, false).can_admin(&project));
    }
}

#[cfg(test)]
mod mcp_auth {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn malformed_authorization_is_rejected_not_anonymous() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer_token(&headers), None);
        for value in [
            "Basic axc_test",
            "Bearer",
            "Bearer ",
            "Bearer  ",
            "Bearer \tsecret",
            "bearer secret",
            "Token secret",
        ] {
            headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
            assert_eq!(bearer_token(&headers), None, "value={value}");
        }
    }

    #[test]
    fn valid_bearer_still_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-ide-key"),
        );
        assert_eq!(bearer_token(&headers), Some("test-ide-key"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer axo_access-token"),
        );
        assert_eq!(bearer_token(&headers), Some("axo_access-token"));
    }
}

#[cfg(test)]
mod key_refresh {
    use super::*;
    use crate::remote::config::OAuthConfig;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SigningKey {
        kid: &'static str,
        pem: String,
        jwks: serde_json::Value,
    }

    fn signing_key(kid: &'static str) -> SigningKey {
        use base64::Engine;
        let key = openssl::rsa::Rsa::generate(2048).unwrap();
        let b64 = |v: Vec<u8>| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
        let jwks = serde_json::json!({"keys": [{
            "kty": "RSA", "alg": "RS256", "use": "sig", "kid": kid,
            "n": b64(key.n().to_vec()), "e": b64(key.e().to_vec())
        }]});
        let pem = String::from_utf8(
            openssl::pkey::PKey::from_rsa(key)
                .unwrap()
                .private_key_to_pem_pkcs8()
                .unwrap(),
        )
        .unwrap();
        SigningKey { kid, pem, jwks }
    }

    fn token(key: &SigningKey) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(key.kid.into());
        let claims = serde_json::json!({
            "sub": "ann", "iss": "https://issuer.example.com/", "aud": "https://ax.example.com",
            "exp": crate::remote::credentials::now() + 3600, "scope": CONTEXT_SCOPE
        });
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(key.pem.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    /// How the fake JWKS endpoint answers.
    #[derive(Clone)]
    enum Jwks {
        Fails,
        Slow(Duration, serde_json::Value),
        Serves(serde_json::Value),
    }

    /// A JWKS endpoint on loopback; returns its URL and a hit counter.
    async fn jwks_server(mode: Jwks) -> (String, Arc<AtomicUsize>) {
        use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let app = Router::new().route(
            "/jwks",
            get(move || {
                let mode = mode.clone();
                counter.fetch_add(1, Ordering::SeqCst);
                async move {
                    match mode {
                        Jwks::Fails => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                        Jwks::Slow(delay, body) => {
                            tokio::time::sleep(delay).await;
                            Json(body).into_response()
                        }
                        Jwks::Serves(body) => Json(body).into_response(),
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/jwks", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, hits)
    }

    fn verifier(jwks_uri: String, cached: &SigningKey, age: Duration) -> TokenVerifier {
        let config = Arc::new(RemoteConfig {
            public_url: "https://ax.example.com".into(),
            oauth: OAuthConfig {
                built_in: false,
                issuer: "https://issuer.example.com/".into(),
                jwks_uri,
            },
            projects: vec![],
            api_keys: vec![],
            allowed_origins: vec![],
        });
        let verifier = TokenVerifier::with_keys(
            config,
            serde_json::from_value(cached.jwks.clone()).unwrap(),
            reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        );
        verifier.keys.try_write().unwrap().fetched = Instant::now() - age;
        verifier
    }

    const STALE: Duration = Duration::from_secs(301);

    #[tokio::test]
    async fn a_failed_refresh_keeps_verifying_with_the_cached_keys() {
        let key = signing_key("k1");
        let (url, hits) = jwks_server(Jwks::Fails).await;
        let verifier = verifier(url, &key, STALE);
        for _ in 0..3 {
            let principal = verifier.verify(&token(&key)).await;
            assert!(principal.is_ok(), "valid token refused: {principal:?}");
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "a failed refresh is retried after a back-off, not on every request"
        );
    }

    #[tokio::test]
    async fn a_slow_key_endpoint_does_not_hold_up_verification() {
        let key = signing_key("k1");
        let (url, _) = jwks_server(Jwks::Slow(Duration::from_secs(3), key.jwks.clone())).await;
        let verifier = verifier(url, &key, STALE);
        let started = Instant::now();
        let mut calls = Vec::new();
        for _ in 0..4 {
            let verifier = verifier.clone();
            let token = token(&key);
            calls.push(tokio::spawn(async move { verifier.verify(&token).await }));
        }
        for call in calls {
            assert!(call.await.unwrap().is_ok());
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "verification waited {:?} for the key refresh",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_rotated_key_is_fetched_on_first_use_and_unknown_kids_are_rate_limited() {
        let old = signing_key("old");
        let new = signing_key("new");
        let (url, hits) = jwks_server(Jwks::Serves(new.jwks.clone())).await;
        let verifier = verifier(url, &old, Duration::ZERO);
        let principal = verifier.verify(&token(&new)).await;
        assert!(principal.is_ok(), "rotated key refused: {principal:?}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let stranger = signing_key("stranger");
        for _ in 0..5 {
            assert_eq!(
                verifier.verify(&token(&stranger)).await.unwrap_err(),
                "Unknown signing key"
            );
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "unknown key ids fetched the key set again within the back-off"
        );
    }
}
