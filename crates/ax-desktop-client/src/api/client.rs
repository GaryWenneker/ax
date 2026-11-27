//! HTTP client for the embedded ax-web `/api` surface.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::sse::{consume_sse_blocking, SseEvent};
use super::types::*;

/// How long a long-running command (LSP enrich, reconcile, savings import,
/// ship commands) may take before the client
/// gives up; the server keeps working either way.
const LONG_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
/// Most events a live SSE stream (MCP trace, ship events) holds for a page
/// that is not reading it. When the buffer is full the stream thread waits,
/// so a hidden page cannot grow memory without limit.
pub const STREAM_BUFFER: usize = 256;
/// Time allowed to open a connection, for ordinary requests and streams.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone)]
pub struct ApiClient {
    base: String,
    /// Ordinary requests: a total timeout covering the whole response.
    http: Client,
    /// Server-sent event streams: only a connect timeout, so a long-lived
    /// stream is never cut while events keep arriving.
    stream_http: Client,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_request_timeout(base_url, std::time::Duration::from_secs(120))
    }

    /// Client whose ordinary requests give up after `timeout`.
    pub fn with_request_timeout(base_url: impl Into<String>, timeout: std::time::Duration) -> Self {
        let http = Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client");
        let stream_http = Client::builder()
            .timeout(None)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest stream client");
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http,
            stream_http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", self.base, path)
        }
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let res = self
            .http
            .get(self.url(path))
            .send()
            .with_context(|| format!("GET {path}"))?;
        let status = res.status();
        let text = res.text().unwrap_or_default();
        if !status.is_success() {
            return Err(api_error(&text, status.as_u16()));
        }
        serde_json::from_str(&text).with_context(|| format!("decode GET {path}"))
    }

    pub fn post_json<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        self.post_json_with(path, body, None)
    }

    /// POST with its own total timeout instead of the client's default.
    fn post_json_with<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        timeout: Option<std::time::Duration>,
    ) -> Result<T> {
        let mut req = self.http.post(self.url(path)).json(body);
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        let res = req.send().with_context(|| format!("POST {path}"))?;
        let status = res.status();
        let text = res.text().unwrap_or_default();
        if !status.is_success() {
            return Err(api_error(&text, status.as_u16()));
        }
        serde_json::from_str(&text).with_context(|| format!("decode POST {path}"))
    }

    pub fn put_json<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let res = self
            .http
            .put(self.url(path))
            .json(body)
            .send()
            .with_context(|| format!("PUT {path}"))?;
        let status = res.status();
        let text = res.text().unwrap_or_default();
        if !status.is_success() {
            return Err(api_error(&text, status.as_u16()));
        }
        serde_json::from_str(&text).with_context(|| format!("decode PUT {path}"))
    }

    // --- Graph / browse ---

    pub fn stats(&self) -> Result<Stats> {
        self.get_json("/api/stats")
    }

    pub fn version(&self) -> Result<VersionInfo> {
        self.get_json("/api/version")
    }

    pub fn nodes(
        &self,
        q: &str,
        kind: &str,
        lang: &str,
        limit: i64,
        offset: i64,
    ) -> Result<NodePage> {
        let mut sp = vec![format!("limit={limit}"), format!("offset={offset}")];
        if !q.is_empty() {
            sp.push(format!("q={}", urlencoding::encode(q)));
        }
        if !kind.is_empty() {
            sp.push(format!("kind={}", urlencoding::encode(kind)));
        }
        if !lang.is_empty() {
            sp.push(format!("lang={}", urlencoding::encode(lang)));
        }
        self.get_json(&format!("/api/nodes?{}", sp.join("&")))
    }

    pub fn node_detail(&self, id: &str) -> Result<NodeDetail> {
        self.get_json(&format!("/api/node/{}", urlencoding::encode(id)))
    }

    pub fn files(
        &self,
        q: &str,
        lang: &str,
        prefix: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<FilePage> {
        let mut sp = vec![format!("limit={limit}"), format!("offset={offset}")];
        if !q.is_empty() {
            sp.push(format!("q={}", urlencoding::encode(q)));
        }
        if !lang.is_empty() {
            sp.push(format!("lang={}", urlencoding::encode(lang)));
        }
        if let Some(p) = prefix {
            if !p.is_empty() {
                sp.push(format!("prefix={}", urlencoding::encode(p)));
            }
        }
        self.get_json(&format!("/api/files?{}", sp.join("&")))
    }

    pub fn file_roots(&self) -> Result<FileRootsPage> {
        self.get_json("/api/files/roots")
    }

    pub fn search(&self, q: &str, limit: i64) -> Result<SearchPage> {
        self.get_json(&format!(
            "/api/search?q={}&limit={limit}",
            urlencoding::encode(q)
        ))
    }

    pub fn source(&self, path: &str, start: Option<i64>, end: Option<i64>) -> Result<SourceSlice> {
        let mut sp = format!("path={}", urlencoding::encode(path));
        if let Some(s) = start {
            sp.push_str(&format!("&start={s}"));
        }
        if let Some(e) = end {
            sp.push_str(&format!("&end={e}"));
        }
        self.get_json(&format!("/api/source?{sp}"))
    }

    pub fn unresolved_summary(&self) -> Result<UnresolvedSummary> {
        self.get_json("/api/unresolved/summary")
    }

    pub fn unresolved(
        &self,
        q: &str,
        kind: &str,
        limit: i64,
        offset: i64,
    ) -> Result<UnresolvedPage> {
        let mut sp = vec![format!("limit={limit}"), format!("offset={offset}")];
        if !q.is_empty() {
            sp.push(format!("q={}", urlencoding::encode(q)));
        }
        if !kind.is_empty() {
            sp.push(format!("kind={}", urlencoding::encode(kind)));
        }
        self.get_json(&format!("/api/unresolved?{}", sp.join("&")))
    }

    pub fn reconcile_unresolved(&self) -> Result<ReconcileResult> {
        self.post_json_with(
            "/api/unresolved/reconcile",
            &serde_json::json!({}),
            Some(LONG_COMMAND_TIMEOUT),
        )
    }

    pub fn lsp_status(&self) -> Result<LspStatus> {
        self.get_json("/api/lsp/status")
    }

    pub fn lsp_enrich(&self, limit: i64) -> Result<LspEnrichResponse> {
        // A first enrich on a large workspace can take minutes.
        self.post_json_with(
            "/api/lsp/enrich",
            &serde_json::json!({ "limit": limit }),
            Some(LONG_COMMAND_TIMEOUT),
        )
    }

    pub fn graph(&self, limit: i64, recompute: bool) -> Result<GraphPayload> {
        let mut path = format!("/api/graph?limit={limit}");
        if recompute {
            path.push_str("&recompute=true");
        }
        self.get_json(&path)
    }

    /// Stream graph SSE events on a background thread. Returns a receiver of events.
    pub fn stream_graph(
        &self,
        limit: i64,
        recompute: bool,
    ) -> Receiver<Result<GraphStreamEvent, String>> {
        let (tx, rx) = mpsc::channel();
        let client = self.clone();
        thread::spawn(move || {
            let mut path = format!("/api/graph/stream?limit={limit}");
            if recompute {
                path.push_str("&recompute=true");
            }
            if let Err(e) = client.stream_graph_inner(&path, &tx) {
                let _ = tx.send(Err(e.to_string()));
            }
        });
        rx
    }

    fn stream_graph_inner(
        &self,
        path: &str,
        tx: &Sender<Result<GraphStreamEvent, String>>,
    ) -> Result<()> {
        let res = self
            .stream_http
            .get(self.url(path))
            .header("Accept", "text/event-stream")
            .send()
            .with_context(|| format!("GET {path}"))?;
        if !res.status().is_success() {
            // Fallback to one-shot graph.
            let qs = path.split('?').nth(1).unwrap_or("limit=100");
            let limit: i64 = qs
                .split('&')
                .find_map(|p| p.strip_prefix("limit=")?.parse().ok())
                .unwrap_or(100);
            let recompute = qs.contains("recompute=true");
            let payload = self.graph(limit, recompute)?;
            let _ = tx.send(Ok(GraphStreamEvent::Meta {
                meta: GraphStreamMeta {
                    total_nodes: payload.total_nodes,
                    truncated: payload.truncated,
                    node_count: payload.nodes.len() as i64,
                    edge_count: payload.edges.len() as i64,
                },
            }));
            let _ = tx.send(Ok(GraphStreamEvent::Nodes {
                nodes: payload.nodes,
            }));
            let _ = tx.send(Ok(GraphStreamEvent::Edges {
                edges: payload.edges,
            }));
            let _ = tx.send(Ok(GraphStreamEvent::Done));
            return Ok(());
        }
        consume_sse_blocking(res, |ev| {
            if ev.data.trim().is_empty() {
                return true;
            }
            match serde_json::from_str::<GraphStreamEvent>(&ev.data) {
                Ok(parsed) => {
                    let done = matches!(parsed, GraphStreamEvent::Done);
                    let _ = tx.send(Ok(parsed));
                    !done
                }
                Err(e) => {
                    // Tolerate unknown keepalives / meta without type.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&ev.data) {
                        if v.get("type").and_then(|t| t.as_str()) == Some("meta") {
                            if let Ok(meta) = serde_json::from_value::<GraphStreamMeta>(v) {
                                let _ = tx.send(Ok(GraphStreamEvent::Meta { meta }));
                            }
                        } else {
                            let _ = tx.send(Err(format!("graph SSE: {e}")));
                        }
                    }
                    true
                }
            }
        })
    }

    // --- Memory ---

    pub fn memories(&self, limit: i64, offset: i64) -> Result<MemoryPage> {
        self.get_json(&format!("/api/memory?limit={limit}&offset={offset}"))
    }

    // --- Usage / savings / pricing ---

    pub fn savings(
        &self,
        period: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<SavingsSummary> {
        let mut path = format!("/api/usage/savings?period={}", urlencoding::encode(period));
        if let Some(f) = from {
            path.push_str(&format!("&from={}", urlencoding::encode(f)));
        }
        if let Some(t) = to {
            path.push_str(&format!("&to={}", urlencoding::encode(t)));
        }
        self.get_json(&path)
    }

    pub fn import_savings(&self) -> Result<SavingsImportResult> {
        self.post_json_with(
            "/api/usage/savings/import",
            &serde_json::json!({}),
            Some(LONG_COMMAND_TIMEOUT),
        )
    }

    pub fn pricing_catalog(&self, source: Option<&str>) -> Result<PricingCatalogResponse> {
        let path = match source {
            Some(s) => format!("/api/usage/pricing?source={}", urlencoding::encode(s)),
            None => "/api/usage/pricing".into(),
        };
        self.get_json(&path)
    }

    pub fn pricing_history(&self, model: &str, days: i64) -> Result<Vec<PricingHistoryPoint>> {
        self.get_json(&format!(
            "/api/usage/pricing/history?model={}&source=openrouter&days={days}",
            urlencoding::encode(model)
        ))
    }

    pub fn sync_pricing(&self, force: bool) -> Result<PricingSyncReport> {
        self.post_json(
            "/api/usage/pricing/sync",
            &serde_json::json!({ "force": force }),
        )
    }

    // --- Ship ---

    pub fn ship_status(&self) -> Result<ShipStatus> {
        self.get_json("/api/ship/status")
    }

    pub fn ship_config(&self) -> Result<ShipConfigResponse> {
        self.get_json("/api/ship/config")
    }

    pub fn save_ship_config(&self, config: &ShipConfig) -> Result<serde_json::Value> {
        // Merge into the full server config so we do not wipe unrelated sections.
        let mut full: serde_json::Value = self.get_json("/api/ship/config")?;
        let cfg = full
            .get_mut("config")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let mut cfg = cfg;
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert(
                "ship".into(),
                serde_json::to_value(&config.ship).unwrap_or_default(),
            );
            obj.insert(
                "ui".into(),
                serde_json::to_value(&config.ui).unwrap_or_default(),
            );
        }
        self.put_json("/api/ship/config", &cfg)
    }

    pub fn ship_command(&self, cmd: &str) -> Result<serde_json::Value> {
        self.post_json_with(
            "/api/ship/command",
            &serde_json::json!({ "cmd": cmd }),
            Some(LONG_COMMAND_TIMEOUT),
        )
    }

    pub fn stream_ship_events(&self) -> Receiver<Result<serde_json::Value, String>> {
        let (tx, rx) = mpsc::sync_channel(STREAM_BUFFER);
        let client = self.clone();
        thread::spawn(move || {
            loop {
                let res = match client
                    .stream_http
                    .get(client.url("/api/ship/events"))
                    .header("Accept", "text/event-stream")
                    .send()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                };
                if !res.status().is_success() {
                    let _ = tx.send(Err(format!("ship events HTTP {}", res.status())));
                    thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                let _ = consume_sse_blocking(res, |ev| {
                    if ev.data.trim().is_empty() {
                        return true;
                    }
                    match serde_json::from_str::<serde_json::Value>(&ev.data) {
                        Ok(v) => tx.send(Ok(v)).is_ok(),
                        Err(e) => tx.send(Err(e.to_string())).is_ok(),
                    }
                });
                // reconnect
                thread::sleep(std::time::Duration::from_secs(1));
            }
        });
        rx
    }

    // --- Policy ---

    pub fn policy_rules(&self) -> Result<PolicyRulesPage> {
        // Prefer list endpoint shapes used by web UI.
        match self.get_json::<PolicyRulesPage>("/api/policy/rules") {
            Ok(p) => Ok(p),
            Err(_) => {
                let raw: serde_json::Value = self.get_json("/api/policy/rules")?;
                let rules = raw
                    .get("rules")
                    .cloned()
                    .or_else(|| raw.as_array().cloned().map(serde_json::Value::Array))
                    .unwrap_or(serde_json::json!([]));
                Ok(PolicyRulesPage {
                    rules: serde_json::from_value(rules).unwrap_or_default(),
                })
            }
        }
    }

    pub fn policy_skills(&self) -> Result<PolicySkillsPage> {
        match self.get_json::<PolicySkillsPage>("/api/policy/skills") {
            Ok(p) => Ok(p),
            Err(_) => {
                let raw: serde_json::Value = self.get_json("/api/policy/skills")?;
                let skills = raw
                    .get("skills")
                    .cloned()
                    .or_else(|| raw.as_array().cloned().map(serde_json::Value::Array))
                    .unwrap_or(serde_json::json!([]));
                Ok(PolicySkillsPage {
                    skills: serde_json::from_value(skills).unwrap_or_default(),
                })
            }
        }
    }

    // --- MCP trace ---

    pub fn mcp_trace_path(&self) -> Result<McpTracePath> {
        self.get_json("/api/usage/mcp-trace/path")
    }

    pub fn stream_mcp_trace(&self) -> Receiver<Result<(String, String), String>> {
        let (tx, rx) = mpsc::sync_channel(STREAM_BUFFER);
        let client = self.clone();
        thread::spawn(move || loop {
            let res = match client
                .stream_http
                .get(client.url("/api/usage/mcp-trace/events"))
                .header("Accept", "text/event-stream")
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
            };
            if !res.status().is_success() {
                let _ = tx.send(Err(format!("mcp-trace HTTP {}", res.status())));
                thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            let _ =
                consume_sse_blocking(res, |ev: SseEvent| tx.send(Ok((ev.event, ev.data))).is_ok());
            thread::sleep(std::time::Duration::from_secs(1));
        });
        rx
    }

    pub fn agent_sessions(&self) -> Result<serde_json::Value> {
        self.get_json("/api/agent/sessions")
            .or_else(|_| Ok(serde_json::json!({ "sessions": [] })))
    }
}

fn api_error(text: &str, status: u16) -> anyhow::Error {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return anyhow!("{err}");
        }
    }
    anyhow!("HTTP {status}: {text}")
}

/// Fire-and-forget background fetch helper used by pages.
pub fn spawn_fetch<T, F>(f: F) -> Receiver<Result<T, String>>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f().map_err(|e| e.to_string()));
    });
    rx
}

pub type SharedClient = Arc<ApiClient>;

#[cfg(test)]
mod timeout_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    /// Serves every connection with `respond` on its own thread.
    fn serve(respond: fn(TcpStream)) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                thread::spawn(move || respond(stream));
            }
        });
        format!("http://{addr}")
    }

    fn read_request(stream: &mut TcpStream) {
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
    }

    #[test]
    fn sse_stream_is_not_cut_by_the_request_timeout() {
        let base = serve(|mut s| {
            read_request(&mut s);
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
            );
            let _ = s.write_all(b"event: line\ndata: first\n\n");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(900));
            let _ = s.write_all(b"event: line\ndata: second\n\n");
            let _ = s.flush();
            thread::sleep(Duration::from_secs(5));
        });
        let client = ApiClient::with_request_timeout(base, Duration::from_millis(300));
        let rx = client.stream_mcp_trace();
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(first.1, "first");
        let second = rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(second.1, "second", "stream was cut and reconnected");
    }

    fn flood(mut s: TcpStream) {
        read_request(&mut s);
        let _ = s.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
        );
        for i in 0..5_000 {
            if write!(s, "event: line\ndata: {{\"n\":{i}}}\n\n").is_err() {
                return;
            }
        }
        let _ = s.flush();
        thread::sleep(Duration::from_secs(5));
    }

    fn queued<T>(rx: &Receiver<T>) -> usize {
        thread::sleep(Duration::from_millis(800));
        std::iter::from_fn(|| rx.try_recv().ok()).count()
    }

    #[test]
    fn unread_mcp_trace_stream_stays_bounded() {
        let client = ApiClient::new(serve(flood));
        let rx = client.stream_mcp_trace();
        let n = queued(&rx);
        assert!(n > 0, "stream delivered nothing");
        // Draining frees slots while the stream thread refills them, so allow some slack.
        assert!(n <= 2 * STREAM_BUFFER, "{n} events queued while unread");
    }

    #[test]
    fn unread_ship_event_stream_stays_bounded() {
        let client = ApiClient::new(serve(flood));
        let rx = client.stream_ship_events();
        let n = queued(&rx);
        assert!(n > 0, "stream delivered nothing");
        // Draining frees slots while the stream thread refills them, so allow some slack.
        assert!(n <= 2 * STREAM_BUFFER, "{n} events queued while unread");
    }

    #[test]
    fn lsp_enrich_outlives_the_request_timeout() {
        let base = serve(|mut s| {
            read_request(&mut s);
            thread::sleep(Duration::from_millis(900));
            let body = r#"{"ok":true,"report":null,"error":null}"#;
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        let client = ApiClient::with_request_timeout(base, Duration::from_millis(300));
        let res = client.lsp_enrich(10);
        assert!(res.is_ok(), "{:?}", res.err());
    }
}
