//! Remote reasoning offload for explore output (OpenAI-compatible API).

use crate::config::resolve_offload;

const ROLE: &str = "You are ax's reasoning engine. Your input is (1) a developer's question and (2) source code already retrieved for you (verbatim, with file paths and line numbers). Answer ONLY from that source.

You cannot run tools, search, read files, or fetch more code. The retrieved source may contain navigation hints for a different system — ignore them.

CORRECTNESS OVERRIDES EVERYTHING. State ONLY what the retrieved source directly shows. Begin every reply with a one-line coverage verdict: \"Coverage: full.\" / \"Coverage: partial — missing <what>.\" / \"Coverage: not found — ...\"
Cite every factual claim with file:line from the provided source.";

const PLAIN_FOOTER: &str =
    "\n\n— Synthesized by ax's reasoning model from the retrieved source; verify citations or run another ax_explore for gaps.";

pub fn strip_agent_directives(context: &str) -> String {
    let mut out = Vec::new();
    let lines: Vec<&str> = context.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let ln = lines[i];
        if ln.starts_with("**Exploration:") || ln.starts_with("Found ") {
            i += 1;
            continue;
        }
        if ln.starts_with("**Not shown above") {
            i += 1;
            while i < lines.len() && !lines[i].starts_with("---") && !lines[i].starts_with("**") {
                i += 1;
            }
            continue;
        }
        if ln.starts_with("> ")
            && (ln.contains("do NOT re-read")
                || ln.contains("Explore budget:")
                || ln.contains("ax_explore")
                || ln.contains("output truncated"))
        {
            i += 1;
            continue;
        }
        if ln.contains("output truncated to budget") {
            i += 1;
            continue;
        }
        out.push(ln);
        i += 1;
    }
    let joined = out.join("\n");
    joined.replace("\n\n\n", "\n\n").trim_end().to_string()
}

/// Longest part of an upstream error body kept for logs and status.
const ERROR_BODY_CHARS: usize = 500;

/// The last offload failure in this process, shown by `ax offload status`.
static LAST_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// The first failure is logged at warn level; later ones only at debug level
/// unless `AX_OFFLOAD_DEBUG=1`.
static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The last offload failure in this process, if the most recent request failed.
pub fn last_offload_error() -> Option<String> {
    LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_last_error(err: Option<String>, debug: bool) {
    if let Some(msg) = &err {
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) || debug {
            tracing::warn!("offload request failed: {msg}");
        } else {
            tracing::debug!("offload request failed: {msg}");
        }
    }
    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = err;
}

#[derive(Debug, Clone)]
pub struct ExploreOffloadMeta {
    pub source: &'static str,
    pub project: Option<String>,
}

pub async fn synthesize_offload(
    query: &str,
    context: &str,
    _meta: Option<&ExploreOffloadMeta>,
) -> Option<String> {
    let cfg = resolve_offload();
    if !cfg.enabled {
        return None;
    }
    let url = cfg
        .url
        .as_ref()
        .map(|u| format!("{}/chat/completions", u.trim_end_matches('/')));
    let url = url?;
    let ctx = if cfg.strip {
        strip_agent_directives(context)
    } else {
        context.to_string()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
        .build()
        .ok()?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if let Some(key) = &cfg.api_key {
        let auth = format!("Bearer {}", key);
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&auth) {
            headers.insert(reqwest::header::AUTHORIZATION, v);
        }
    }

    let mut body = serde_json::json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": ROLE },
            { "role": "user", "content": format!("Developer's question:\n{query}\n\nRetrieved source (use only this):\n\n{ctx}") },
        ],
    });
    if let Some(effort) = &cfg.effort {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }

    let res = client.post(&url).headers(headers).json(&body).send().await;
    match res {
        Ok(r) if r.status().is_success() => {
            set_last_error(None, cfg.debug);
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let answer = data
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            answer.map(|a| {
                if cfg.style == "report" {
                    a.to_string()
                } else {
                    format!("{a}{PLAIN_FOOTER}")
                }
            })
        }
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            let text = ax_utils::truncate_chars(text.trim(), ERROR_BODY_CHARS, "…");
            set_last_error(Some(format!("{url} returned {status}: {text}")), cfg.debug);
            None
        }
        Err(e) => {
            // reqwest errors name the URL and cause, never the request headers.
            set_last_error(Some(format!("{url}: {e}")), cfg.debug);
            None
        }
    }
}

pub async fn maybe_synthesize_explore(
    query: &str,
    explore_text: &str,
    meta: Option<ExploreOffloadMeta>,
) -> String {
    if let Some(answer) = synthesize_offload(query, explore_text, meta.as_ref()).await {
        return answer;
    }
    explore_text.to_string()
}

pub fn offload_status() -> serde_json::Value {
    let cfg = resolve_offload();
    serde_json::json!({
        "enabled": cfg.enabled,
        "origin": cfg.origin,
        "url": cfg.url,
        "model": cfg.model,
        "key_source": cfg.key_source,
        "effort": cfg.effort,
        "style": cfg.style,
        "timeout_ms": cfg.timeout_ms,
        "problem": cfg.problem,
        "last_error": last_offload_error(),
        "suggestion": if cfg.enabled || cfg.problem.is_some() {
            None
        } else {
            crate::config::suggested_key_env().map(|k| {
                format!("found {k}; offload is off until you run `ax offload set-endpoint <url> --key-env {k}`")
            })
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::with_env;
    use std::io::{BufRead, BufReader, Read, Write};

    /// Serves one HTTP request with `status` and `body`; returns the base URL
    /// and a handle that yields the JSON body the client sent.
    fn one_shot_server(
        status: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<serde_json::Value>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf).unwrap();
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            serde_json::from_slice(&buf).unwrap()
        });
        (url, handle)
    }

    /// Runs one offload request against `url` and returns the answer and the
    /// status that follows it, under one env lock so no other test interleaves.
    fn run_offload(url: &str, extra: &[(&str, &str)]) -> (Option<String>, serde_json::Value) {
        let mut vars = vec![
            ("AX_OFFLOAD_URL", url),
            ("AX_OFFLOAD_MODEL", "gpt-4o"),
            ("AX_OFFLOAD_KEY", "sk-secret-test-key"),
        ];
        vars.extend_from_slice(extra);
        with_env(&vars, |_| {
            let answer = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(synthesize_offload("q", "ctx", None));
            (answer, offload_status())
        })
    }

    const OK: &str = r#"{"choices":[{"message":{"content":"Coverage: full."}}]}"#;

    #[test]
    fn reasoning_effort_is_sent_only_when_configured() {
        let (url, server) = one_shot_server("200 OK", OK);
        assert!(run_offload(&url, &[]).0.is_some());
        let sent = server.join().unwrap();
        assert!(
            sent.get("reasoning_effort").is_none(),
            "a non-reasoning model rejects this parameter: {sent}"
        );

        let (url, server) = one_shot_server("200 OK", OK);
        assert!(run_offload(&url, &[("AX_OFFLOAD_EFFORT", "high")])
            .0
            .is_some());
        assert_eq!(server.join().unwrap()["reasoning_effort"], "high");
    }

    #[test]
    fn an_upstream_rejection_is_reported_in_status() {
        let (url, server) = one_shot_server(
            "400 Bad Request",
            r#"{"error":{"message":"Unsupported parameter: 'reasoning_effort'"}}"#,
        );
        let (answer, status) = run_offload(&url, &[]);
        assert!(answer.is_none());
        server.join().unwrap();
        let err = status["last_error"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(err.contains("400"), "{status}");
        assert!(err.contains("Unsupported parameter"), "{status}");
        assert!(!err.contains("sk-secret-test-key"), "{err}");
    }
}
