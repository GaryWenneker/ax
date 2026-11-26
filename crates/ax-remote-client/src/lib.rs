//! Client side of Ax remote MCP: browser sign-in (OAuth with PKCE) and the token
//! store. It has no dependency on the indexing engine, so a small tool such as
//! `ax-admin` can sign in to a remote Ax without linking it.
pub mod client;
pub mod credentials;

use url::Url;

pub fn https_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Invalid HTTPS URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Expected an HTTPS URL without credentials, query or fragment".into());
    }
    Ok(url)
}
