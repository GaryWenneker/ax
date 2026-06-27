//! Debounce and throttle utilities.

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

/// Debounce: only invoke after `delay` has passed since the last call.
pub async fn debounce<F, Fut>(delay: Duration, f: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    sleep(delay).await;
    f().await;
}

/// Throttle state for repeated calls.
pub struct Throttle {
    last: Mutex<Instant>,
    interval: Duration,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            last: Mutex::new(Instant::now() - interval),
            interval,
        }
    }

    pub async fn call<F, Fut>(&self, f: F) -> bool
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let mut last = self.last.lock().await;
        let now = Instant::now();
        if now.duration_since(*last) >= self.interval {
            *last = now;
            f().await;
            true
        } else {
            false
        }
    }
}
