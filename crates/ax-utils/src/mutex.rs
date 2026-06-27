//! Async mutex wrapper with with_lock pattern.

use std::future::Future;
use tokio::sync::Mutex as TokioMutex;

pub struct AsyncMutex<T> {
    inner: TokioMutex<T>,
}

impl<T> AsyncMutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: TokioMutex::new(value),
        }
    }

    pub async fn with_lock<F, Fut, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> Fut,
        Fut: Future<Output = R>,
    {
        let mut guard = self.inner.lock().await;
        f(&mut *guard).await
    }

    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
        self.inner.lock().await
    }

    pub fn try_lock(&self) -> Result<tokio::sync::MutexGuard<'_, T>, tokio::sync::TryLockError> {
        self.inner.try_lock()
    }
}
