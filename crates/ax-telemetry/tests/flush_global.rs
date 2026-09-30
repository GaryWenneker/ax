use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::TryLockError;
use std::time::Duration;

/// Accepts one request, reports whether the global telemetry lock was free while it
/// was being handled, and answers 200.
fn spawn_ingest() -> (String, mpsc::Receiver<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/ingest", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
        }
        let lock_free = !matches!(
            ax_telemetry::telemetry().try_lock(),
            Err(TryLockError::WouldBlock)
        );
        tx.send(lock_free).unwrap();
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
    });
    (url, rx)
}

#[test]
fn flush_global_sends_without_holding_the_telemetry_lock() {
    let home = tempfile::tempdir().unwrap();
    let (url, lock_free) = spawn_ingest();
    std::env::set_var("AX_HOME_DIR", home.path());
    std::env::set_var("AX_TELEMETRY", "1");
    std::env::set_var("AX_TELEMETRY_ENDPOINT", &url);

    {
        let mut t = ax_telemetry::telemetry().lock().unwrap();
        t.set_enabled(true, "test");
        t.record_usage("cli_command", "index", true, None);
        t.persist_sync();
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(ax_telemetry::flush_global(5_000));

    let observed = lock_free
        .recv_timeout(Duration::from_secs(5))
        .expect("the batch reached the ingest endpoint");
    assert!(observed, "telemetry lock was held while the batch was sent");

    let queue = ax_telemetry::telemetry().lock().unwrap().queue_path();
    let remaining = std::fs::read_to_string(&queue).unwrap_or_default();
    assert!(remaining.trim().is_empty(), "sent lines stay queued: {remaining}");
}
