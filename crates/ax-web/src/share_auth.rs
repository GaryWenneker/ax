//! Optional share-token gate for `ax share` / LAN exposure.

use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;

pub async fn share_token_middleware(req: Request, next: Next) -> Response {
    let client_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(token) =
        client_token.filter(|token| token.starts_with("axc_") || token.starts_with("axn_"))
    {
        let db_path = match ax_global_db::global_db_path() {
            Ok(p) => p,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let db = match ax_global_db::open_and_init(&db_path).await {
            Ok(db) => db,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let principal = match ax_global_db::access::authenticate_client_token(&db, token).await {
            Ok(Some(p)) => p,
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({"error":"Invalid client credential"})),
                )
                    .into_response()
            }
        };
        // Only explicitly scoped APIs are available to client credentials.
        // Access management performs its more specific role checks inside its router.
        let path = req.uri().path();
        let scope = http_scope(path, req.method() == axum::http::Method::GET);
        if scope.is_none_or(|scope| !principal.allows(scope)) {
            let _ = ax_global_db::access::audit(
                &db,
                Some(&principal.member_id),
                Some(&principal.client_id),
                "authorization.denied",
                "http",
                path,
                &serde_json::json!({}),
            )
            .await;
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({"error":"Insufficient client scope"})),
            )
                .into_response();
        }
        return next.run(req).await;
    }
    let Some(expected) = std::env::var("AX_SHARE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
    else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    if path.starts_with("/assets/")
        || path == "/manifest.webmanifest"
        || path == "/sw.js"
        || path == "/favicon.ico"
        || path == "/api/share/status"
        || path.starts_with("/api/auth/microsoft")
    {
        return next.run(req).await;
    }

    let query_token = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("token"), Some(v)) => Some(urlencoding_decode(v)),
                _ => None,
            }
        })
    });

    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let cookie_token = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| {
            c.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("ax_share=").map(|s| s.to_string())
            })
        });

    let provided = query_token.or(header_token).or(cookie_token);
    if provided.as_deref() == Some(expected.as_str()) {
        let mut res = next.run(req).await;
        if let Ok(val) = format!("ax_share={expected}; Path=/; HttpOnly; SameSite=Lax").parse() {
            res.headers_mut().append(header::SET_COOKIE, val);
        }
        return res;
    }

    ax_usage::log_share(None, "auth fail");
    crate::share_api::unauthorized_gate_html()
}

fn urlencoding_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(b) =
                    u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
                {
                    out.push(b as char);
                    i += 3;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

fn http_scope(path: &str, read: bool) -> Option<&'static str> {
    if path == "/api/access/team" || path.starts_with("/api/access/team/") {
        return Some("admin:team");
    }
    if path == "/api/access/nodes" || path.starts_with("/api/access/nodes/") {
        return Some("admin:nodes");
    }
    if path.starts_with("/api/access/") {
        return Some("admin:clients");
    }
    if path == "/api/memory" || path.starts_with("/api/memory/") {
        return Some(if read { "memory:read" } else { "memory:write" });
    }
    if path.starts_with("/api/policy/share/") {
        return Some(if read { "share:read" } else { "share:write" });
    }
    if read
        && [
            "/api/stats",
            "/api/nodes",
            "/api/edges",
            "/api/files",
            "/api/search",
            "/api/graph",
            "/api/insights",
        ]
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return Some("context:read");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clients_only_reach_scoped_api_paths() {
        assert_eq!(http_scope("/api/memory/entries", true), Some("memory:read"));
        assert_eq!(
            http_scope("/api/memory/entries", false),
            Some("memory:write")
        );
        assert_eq!(http_scope("/api/access/team/u1", false), Some("admin:team"));
        assert_eq!(
            http_scope("/api/access/credentials/c1/revoke", false),
            Some("admin:clients")
        );
        assert_eq!(http_scope("/api/agent/terminal", false), None);
        assert_eq!(http_scope("/api/nodes", false), None);
    }
}
