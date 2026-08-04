//! Optional share-token gate for `ax share` / LAN exposure.

use axum::extract::Request;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

pub async fn share_token_middleware(req: Request, next: Next) -> Response {
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
        if let Ok(val) =
            format!("ax_share={expected}; Path=/; HttpOnly; SameSite=Lax").parse()
        {
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
