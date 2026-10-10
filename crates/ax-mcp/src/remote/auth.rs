use super::config::{Project, RemoteConfig};
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

pub const CONTEXT_SCOPE: &str = "ax:context";
pub const WRITE_SCOPE: &str = "ax:write";

#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub subject: Option<String>,
    pub scopes: HashSet<String>,
    pub key_projects: Vec<String>,
    pub key_write: bool,
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
}

#[derive(Clone)]
pub struct TokenVerifier {
    config: Arc<RemoteConfig>,
    keys: Arc<RwLock<(JwkSet, Instant)>>,
    client: reqwest::Client,
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
        let keys = fetch_keys(&client, &config.oauth.jwks_uri).await?;
        Ok(Self::with_keys(config, keys, client))
    }
    pub fn with_keys(config: Arc<RemoteConfig>, keys: JwkSet, client: reqwest::Client) -> Self {
        Self {
            config,
            keys: Arc::new(RwLock::new((keys, Instant::now()))),
            client,
        }
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
                return Ok(Principal {
                    id: format!("key:{}", key.id),
                    subject: None,
                    scopes,
                    key_projects: key.projects.clone(),
                    key_write: key.write,
                });
            }
        }
        // Never follow a token's jku/x5u or accept symmetric algorithms.
        let header = decode_header(token).map_err(|_| "Invalid access token")?;
        if header.alg != Algorithm::RS256 {
            return Err("Unsupported token algorithm".into());
        }
        let kid = header.kid.ok_or("Token needs a signing key ID")?;
        if self.keys.read().await.1.elapsed() > Duration::from_secs(300) {
            let mut keys = self.keys.write().await;
            if keys.1.elapsed() > Duration::from_secs(300) {
                *keys = (
                    fetch_keys(&self.client, &self.config.oauth.jwks_uri).await?,
                    Instant::now(),
                );
            }
        }
        let keys = self.keys.read().await;
        let jwk = keys.0.find(&kid).ok_or("Unknown signing key")?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| "Invalid signing key")?;
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
        })
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
