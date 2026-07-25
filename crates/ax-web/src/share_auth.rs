//! Optional share-token gate for `ax share` / LAN exposure.

use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn share_token_middleware(req: Request, next: Next) -> Response {
    let Some(expected) = std::env::var("AX_SHARE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
    else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    // Allow PWA assets without token so install still works after first auth.
    if path.starts_with("/assets/")
        || path == "/manifest.webmanifest"
        || path == "/sw.js"
        || path == "/favicon.ico"
    {
        return next.run(req).await;
    }

    let query_token = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("token"), Some(v)) => Some(v.to_string()),
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
        // Persist token for subsequent SPA navigations.
        if let Ok(val) = format!(
            "ax_share={expected}; Path=/; HttpOnly; SameSite=Lax"
        )
        .parse()
        {
            res.headers_mut().append(header::SET_COOKIE, val);
        }
        return res;
    }

    (
        StatusCode::UNAUTHORIZED,
        "ax share: missing or invalid token (pass ?token=… or Authorization: Bearer …)",
    )
        .into_response()
}
