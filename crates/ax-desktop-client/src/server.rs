//! Embed `ax_web::serve_with` on a dedicated Tokio runtime thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use ax_web::ServeOptions;

pub struct EmbeddedServer {
    shutdown: Arc<AtomicBool>,
    _handle: JoinHandle<()>,
}

impl EmbeddedServer {
    pub fn start(root: PathBuf, port: u16, bind: String) -> Result<Self, String> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = shutdown.clone();

        let handle = std::thread::Builder::new()
            .name("ax-web-embed".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("ax-web-rt")
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("ax-desktop: tokio runtime failed: {e}");
                        return;
                    }
                };

                rt.block_on(async move {
                    let opts = ServeOptions {
                        root,
                        port,
                        open: false,
                        bind,
                        share_token: None,
                    };
                    let serve = ax_web::serve_with(opts);
                    tokio::select! {
                        res = serve => {
                            if let Err(e) = res {
                                eprintln!("ax-desktop: embedded ax-web exited: {e}");
                            }
                        }
                        _ = wait_shutdown(flag) => {
                            eprintln!("ax-desktop: embedded ax-web shutting down.");
                        }
                    }
                });
            })
            .map_err(|e| format!("failed to spawn server thread: {e}"))?;

        Ok(Self {
            shutdown,
            _handle: handle,
        })
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn wait_shutdown(flag: Arc<AtomicBool>) {
    while !flag.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
