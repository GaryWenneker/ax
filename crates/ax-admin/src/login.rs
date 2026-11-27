//! Browser sign-in (OAuth with PKCE). The real backend is the same code that
//! `ax mcp remote login` uses; tokens stay in the OS credential store.
use crate::mcp::{TokenFuture, TokenSource};
use std::{future::Future, pin::Pin, sync::Arc};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq)]
pub struct LoginStarted {
    pub id: String,
    pub authorization_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoginState {
    Pending,
    Connected,
    Failed(String),
}

pub trait LoginBackend: Send + Sync {
    /// `admin` also asks for the admin scope (which implies write access).
    fn start(
        &self,
        url: String,
        client_id: String,
        write: bool,
        admin: bool,
    ) -> BoxFuture<'_, Result<LoginStarted, String>>;
    /// `None` when the attempt is unknown or has expired.
    fn status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<LoginState>>;
    fn cancel<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>>;
    /// Removes stored tokens for an address and revokes them when possible.
    fn logout<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<(), String>>;
    fn tokens(&self, url: &str) -> Arc<dyn TokenSource>;
}

pub struct BrowserLogin;

struct StoredTokens(String);

impl TokenSource for StoredTokens {
    fn token(&self, after_rejection: bool) -> TokenFuture<'_> {
        Box::pin(async move {
            if after_rejection {
                // The server refused the stored token: get a new one now.
                ax_remote_client::client::refresh_access_token(&self.0).await
            } else {
                // Refreshes by itself when the token is about to expire.
                ax_remote_client::client::access_token(&self.0).await
            }
        })
    }
    fn refreshable(&self) -> bool {
        true
    }
}

impl LoginBackend for BrowserLogin {
    fn start(
        &self,
        url: String,
        client_id: String,
        write: bool,
        admin: bool,
    ) -> BoxFuture<'_, Result<LoginStarted, String>> {
        Box::pin(async move {
            // Discovery refuses anything but an https:// address.
            let started =
                ax_remote_client::client::start_login_scoped(url, client_id, write, admin).await?;
            Ok(LoginStarted {
                id: started.id,
                authorization_url: started.authorization_url,
            })
        })
    }
    fn status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<LoginState>> {
        Box::pin(async move {
            let status = ax_remote_client::client::login_status(id).await?;
            Some(match status.state.as_str() {
                "connected" => LoginState::Connected,
                "failed" => {
                    LoginState::Failed(status.message.unwrap_or_else(|| "Sign-in failed".into()))
                }
                _ => LoginState::Pending,
            })
        })
    }
    fn cancel<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(ax_remote_client::client::cancel_login(id))
    }
    fn logout<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(ax_remote_client::client::logout(url))
    }
    fn tokens(&self, url: &str) -> Arc<dyn TokenSource> {
        Arc::new(StoredTokens(url.to_owned()))
    }
}
