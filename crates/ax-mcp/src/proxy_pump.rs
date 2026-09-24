//! Line pump between the MCP client (stdio) and the daemon, surviving a daemon restart.

use std::future::Future;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::daemon_conn::DaemonSession;

pub const DAEMON_RESTARTED: &str = "ax daemon restarted; retry the call";

const FIRST_RETRY: Duration = Duration::from_millis(100);
const MAX_RETRY: Duration = Duration::from_secs(1);

type DaemonWriter = Box<dyn AsyncWrite + Unpin + Send>;

/// Lines from a reader, over a channel: `read_line` itself is not cancel-safe in `select!`.
fn spawn_line_reader<R>(mut reader: R) -> mpsc::Receiver<String>
where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(line).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

fn attach(session: DaemonSession) -> (mpsc::Receiver<String>, DaemonWriter) {
    let (reader, writer) = session.into_split();
    (spawn_line_reader(reader), writer)
}

fn parse(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim()).ok()
}

/// The id of a request that expects an answer (notifications have none).
fn request_id(line: &str) -> Option<Value> {
    let msg = parse(line)?;
    msg.get("method")?;
    msg.get("id").filter(|id| !id.is_null()).cloned()
}

fn response_id(msg: &Value) -> Option<&Value> {
    if msg.get("method").is_some() {
        return None;
    }
    msg.get("id")
}

async fn write_line<W: AsyncWrite + Unpin + ?Sized>(out: &mut W, line: &str) -> std::io::Result<()> {
    out.write_all(line.as_bytes()).await?;
    if !line.ends_with('\n') {
        out.write_all(b"\n").await?;
    }
    out.flush().await
}

/// One daemon line: side-channel lines go to stderr, the rest to the client.
async fn forward<W: AsyncWrite + Unpin>(
    client_out: &mut W,
    line: &str,
    pending: &mut Vec<Value>,
) -> Result<(), Stop> {
    let trimmed = line.trim();
    // Verbose MCP traces (daemon side-channel): stderr only — never Cursor stdout.
    if let Some(text) = crate::verbose::parse_ax_log_line(trimmed) {
        eprintln!("{text}");
        return Ok(());
    }
    let msg = parse(trimmed);
    if let Some(msg) = &msg {
        // Daemon hello handshake — not JSON-RPC; must not reach Cursor stdout.
        if msg.get("type").and_then(|t| t.as_str()) == Some("hello") && msg.get("jsonrpc").is_none() {
            return Ok(());
        }
        // Cursor Output surfaces process stderr more reliably than notification traffic.
        if msg.get("method").and_then(|m| m.as_str()) == Some("notifications/message") {
            if let Some(data) = msg.pointer("/params/data").and_then(|d| d.as_str()) {
                if trimmed.contains("\"ax-mcp\"") {
                    eprintln!("{data}");
                }
            }
        }
        if let Some(id) = response_id(msg) {
            pending.retain(|p| p != id);
        }
    }
    write_line(client_out, line).await.map_err(|_| Stop::ClientLeft)
}

async fn fail_pending<W: AsyncWrite + Unpin>(
    client_out: &mut W,
    pending: &mut Vec<Value>,
) -> Result<(), Stop> {
    for id in pending.drain(..) {
        let reply = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": DAEMON_RESTARTED },
        });
        write_line(client_out, &reply.to_string())
            .await
            .map_err(|_| Stop::ClientLeft)?;
    }
    Ok(())
}

async fn reconnect_within<C, F>(reconnect: &mut C, deadline: Duration) -> Result<DaemonSession, Stop>
where
    C: FnMut(u32) -> F,
    F: Future<Output = Option<DaemonSession>>,
{
    let give_up = tokio::time::Instant::now() + deadline;
    let mut delay = FIRST_RETRY;
    for attempt in 0.. {
        let now = tokio::time::Instant::now();
        if now >= give_up {
            break;
        }
        tokio::time::sleep(delay.min(give_up - now)).await;
        if let Some(session) = reconnect(attempt).await {
            return Ok(session);
        }
        delay = (delay * 2).min(MAX_RETRY);
    }
    Err(Stop::DaemonLost(format!(
        "ax daemon connection lost and no daemon came back within {}s",
        deadline.as_secs()
    )))
}

/// Forwards client lines to the daemon and daemon lines back. When the daemon goes away,
/// every unanswered request gets an error, and `reconnect(attempt)` is retried until it
/// yields a session or `deadline` passes. Returns `Ok` once the client has left: its input
/// closed or its output stopped accepting lines.
pub async fn pump<R, W, C, F>(
    client_in: R,
    client_out: W,
    first: DaemonSession,
    reconnect: C,
    deadline: Duration,
) -> Result<(), String>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
    C: FnMut(u32) -> F,
    F: Future<Output = Option<DaemonSession>>,
{
    match run(client_in, client_out, first, reconnect, deadline).await {
        Ok(()) | Err(Stop::ClientLeft) => Ok(()),
        Err(Stop::DaemonLost(reason)) => Err(reason),
    }
}

enum Stop {
    ClientLeft,
    DaemonLost(String),
}

async fn run<R, W, C, F>(
    client_in: R,
    mut client_out: W,
    first: DaemonSession,
    mut reconnect: C,
    deadline: Duration,
) -> Result<(), Stop>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
    C: FnMut(u32) -> F,
    F: Future<Output = Option<DaemonSession>>,
{
    let mut client_rx = spawn_line_reader(client_in);
    let (mut daemon_rx, mut daemon_tx) = attach(first);
    let mut pending: Vec<Value> = Vec::new();
    // A client line the lost daemon never accepted; it goes to the next daemon first.
    let mut unsent: Option<String> = None;
    loop {
        if let Some(line) = unsent.take() {
            if !send_to_daemon(&mut daemon_tx, line, &mut pending, &mut unsent).await {
                fail_pending(&mut client_out, &mut pending).await?;
                (daemon_rx, daemon_tx) = attach(reconnect_within(&mut reconnect, deadline).await?);
                continue;
            }
        }
        let alive = tokio::select! {
            line = client_rx.recv() => match line {
                None => return drain_after_client_left(&mut client_out, daemon_rx, daemon_tx, &mut pending).await,
                Some(line) => send_to_daemon(&mut daemon_tx, line, &mut pending, &mut unsent).await,
            },
            line = daemon_rx.recv() => match line {
                Some(line) => {
                    forward(&mut client_out, &line, &mut pending).await?;
                    true
                }
                None => false,
            },
        };
        if !alive {
            fail_pending(&mut client_out, &mut pending).await?;
            (daemon_rx, daemon_tx) = attach(reconnect_within(&mut reconnect, deadline).await?);
        }
    }
}

/// Returns false when the daemon refused the write; the line is then parked in `unsent`.
async fn send_to_daemon(
    daemon_tx: &mut DaemonWriter,
    line: String,
    pending: &mut Vec<Value>,
    unsent: &mut Option<String>,
) -> bool {
    if write_line(daemon_tx, &line).await.is_err() {
        *unsent = Some(line);
        return false;
    }
    if let Some(id) = request_id(&line) {
        pending.push(id);
    }
    true
}

/// The client closed its input: tell the daemon, deliver what it still sends, then stop.
async fn drain_after_client_left<W: AsyncWrite + Unpin>(
    client_out: &mut W,
    mut daemon_rx: mpsc::Receiver<String>,
    mut daemon_tx: DaemonWriter,
    pending: &mut Vec<Value>,
) -> Result<(), Stop> {
    let _ = daemon_tx.shutdown().await;
    while let Some(line) = daemon_rx.recv().await {
        forward(client_out, &line, pending).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};
    use tokio::io::{
        duplex, split, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf,
        WriteHalf,
    };
    use tokio::task::JoinHandle;

    const WAIT: Duration = Duration::from_secs(5);

    struct Peer {
        reader: BufReader<ReadHalf<DuplexStream>>,
        writer: WriteHalf<DuplexStream>,
    }

    impl Peer {
        async fn line(&mut self) -> Option<Value> {
            let mut line = String::new();
            let n = tokio::time::timeout(WAIT, self.reader.read_line(&mut line))
                .await
                .expect("timed out waiting for a line")
                .unwrap();
            (n > 0).then(|| serde_json::from_str(line.trim()).unwrap())
        }

        async fn send(&mut self, value: Value) {
            let mut text = value.to_string();
            text.push('\n');
            self.writer.write_all(text.as_bytes()).await.unwrap();
            self.writer.flush().await.unwrap();
        }
    }

    fn peer(io: DuplexStream) -> Peer {
        let (r, w) = split(io);
        Peer { reader: BufReader::new(r), writer: w }
    }

    fn daemon() -> (DaemonSession, Peer) {
        let (proxy_side, daemon_side) = duplex(64 * 1024);
        (DaemonSession::from_io(proxy_side), peer(daemon_side))
    }

    fn request(id: u64, name: &str) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name}})
    }

    fn result(id: &Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"result":{"ok":true}})
    }

    struct Harness {
        client: Peer,
        task: JoinHandle<Result<(), String>>,
        attempts: Arc<Mutex<u32>>,
    }

    /// `later` are the sessions `reconnect` hands out, in order; after that it yields none.
    fn start(first: DaemonSession, later: Vec<DaemonSession>, deadline: Duration) -> Harness {
        let (client_side, proxy_side) = duplex(64 * 1024);
        let (proxy_in, proxy_out) = split(proxy_side);
        let queue = Arc::new(Mutex::new(VecDeque::from(later)));
        let attempts = Arc::new(Mutex::new(0));
        let counter = Arc::clone(&attempts);
        let task = tokio::spawn(async move {
            pump(
                BufReader::new(proxy_in),
                proxy_out,
                first,
                move |_| {
                    *counter.lock().unwrap() += 1;
                    let next = queue.lock().unwrap().pop_front();
                    async move { next }
                },
                deadline,
            )
            .await
        });
        Harness { client: peer(client_side), task, attempts }
    }

    #[tokio::test]
    async fn d1_d7_a_call_after_the_daemon_restarted_is_served_by_the_new_one() {
        let (s1, mut d1) = daemon();
        let (s2, mut d2) = daemon();
        let mut h = start(s1, vec![s2], WAIT);

        h.client.send(request(1, "ax_status")).await;
        let req = d1.line().await.unwrap();
        d1.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 1);

        drop(d1);
        h.client.send(request(2, "ax_status")).await;
        let req = d2.line().await.unwrap();
        assert_eq!(req["id"], 2);
        d2.send(result(&req["id"])).await;
        let reply = h.client.line().await.unwrap();
        assert_eq!(reply["id"], 2);
        assert_eq!(reply["result"]["ok"], true);
    }

    #[tokio::test]
    async fn d2_an_unanswered_request_gets_an_error_with_its_own_id() {
        let (s1, mut d1) = daemon();
        let (s2, _d2) = daemon();
        let mut h = start(s1, vec![s2], WAIT);

        h.client.send(request(7, "ax_explore")).await;
        assert_eq!(d1.line().await.unwrap()["id"], 7);
        drop(d1);
        let reply = h.client.line().await.unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["error"]["code"], -32000);
        assert_eq!(reply["error"]["message"], DAEMON_RESTARTED);
    }

    #[tokio::test]
    async fn d2_an_answered_request_gets_no_second_reply() {
        let (s1, mut d1) = daemon();
        let (s2, mut d2) = daemon();
        let mut h = start(s1, vec![s2], WAIT);

        h.client.send(request(1, "ax_status")).await;
        let req = d1.line().await.unwrap();
        d1.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 1);
        drop(d1);
        h.client.send(request(2, "ax_status")).await;
        let req = d2.line().await.unwrap();
        d2.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 2, "no error for id 1 in between");
    }

    #[tokio::test]
    async fn d3_requests_sent_during_the_reconnect_arrive_in_order() {
        let (s1, d1) = daemon();
        let (s2, mut d2) = daemon();
        let mut h = start(s1, vec![s2], WAIT);

        drop(d1);
        for id in 10..13 {
            h.client.send(request(id, "ax_search")).await;
        }
        for id in 10..13 {
            assert_eq!(d2.line().await.unwrap()["id"], id);
        }
    }

    /// A daemon connection that never speaks and refuses every write.
    struct RefusingDaemon;

    impl tokio::io::AsyncRead for RefusingDaemon {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Pending
        }
    }

    impl tokio::io::AsyncWrite for RefusingDaemon {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn d3_a_request_the_dead_daemon_refused_goes_to_the_new_one_without_an_error() {
        let (s2, mut d2) = daemon();
        let mut h = start(DaemonSession::from_io(RefusingDaemon), vec![s2], WAIT);

        h.client.send(request(4, "ax_status")).await;
        let req = d2.line().await.unwrap();
        assert_eq!(req["id"], 4);
        d2.send(result(&req["id"])).await;
        let reply = h.client.line().await.unwrap();
        assert_eq!(reply["id"], 4);
        assert_eq!(reply["result"]["ok"], true, "no restart error for a request never sent");
    }

    #[tokio::test]
    async fn d4_a_notification_in_flight_gets_no_error() {
        let (s1, mut d1) = daemon();
        let (s2, mut d2) = daemon();
        let mut h = start(s1, vec![s2], WAIT);

        h.client
            .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        assert!(d1.line().await.unwrap().get("id").is_none());
        drop(d1);
        h.client.send(request(3, "ax_status")).await;
        let req = d2.line().await.unwrap();
        d2.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 3, "first client line is the reply");
    }

    #[tokio::test]
    async fn d5_no_daemon_within_the_deadline_answers_pending_and_fails() {
        let (s1, mut d1) = daemon();
        let mut h = start(s1, vec![], Duration::from_millis(400));

        h.client.send(request(5, "ax_status")).await;
        d1.line().await.unwrap();
        drop(d1);
        assert_eq!(h.client.line().await.unwrap()["error"]["code"], -32000);
        let outcome = tokio::time::timeout(WAIT, h.task).await.unwrap().unwrap();
        assert!(outcome.is_err());
        assert!(*h.attempts.lock().unwrap() >= 2, "retried before giving up");
    }

    #[tokio::test]
    async fn d6_the_client_closing_its_input_ends_the_pump_without_reconnecting() {
        let (s1, mut d1) = daemon();
        let mut h = start(s1, vec![], WAIT);

        h.client.send(request(1, "ax_status")).await;
        let req = d1.line().await.unwrap();
        h.client.writer.shutdown().await.unwrap();
        d1.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 1, "the last reply still arrives");
        assert!(d1.line().await.is_none(), "the daemon sees the client leave");
        drop(d1);
        let outcome = tokio::time::timeout(WAIT, h.task).await.unwrap().unwrap();
        assert_eq!(outcome, Ok(()));
        assert_eq!(*h.attempts.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn d6_a_client_that_stopped_reading_has_left_cleanly() {
        let (s1, mut d1) = daemon();
        let (client_side, proxy_side) = duplex(1024);
        let (proxy_in, _proxy_out) = split(proxy_side);
        let _client = client_side;
        let task = tokio::spawn(pump(
            BufReader::new(proxy_in),
            RefusingDaemon,
            s1,
            |_| async { None },
            WAIT,
        ));

        d1.send(json!({"jsonrpc":"2.0","method":"notifications/message","params":{}}))
            .await;
        let outcome = tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
        assert_eq!(outcome, Ok(()));
    }

    #[tokio::test]
    async fn the_hello_line_never_reaches_the_client() {
        let (s1, mut d1) = daemon();
        let mut h = start(s1, vec![], WAIT);

        d1.send(json!({"type":"hello","pid":1,"ax":"5.1.0","project":"/p","port":0}))
            .await;
        h.client.send(request(1, "ax_status")).await;
        let req = d1.line().await.unwrap();
        d1.send(result(&req["id"])).await;
        assert_eq!(h.client.line().await.unwrap()["id"], 1);
    }
}
