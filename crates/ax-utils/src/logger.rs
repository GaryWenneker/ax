//! Structured logging.

use std::sync::{OnceLock, RwLock};

pub trait Logger: Send + Sync {
    fn debug(&self, msg: &str);
    fn info(&self, msg: &str);
    fn warn(&self, msg: &str);
    fn error(&self, msg: &str);
}

struct DefaultLogger;

impl Logger for DefaultLogger {
    fn debug(&self, msg: &str) {
        tracing::debug!(target: "ax", "{msg}");
    }
    fn info(&self, msg: &str) {
        tracing::info!(target: "ax", "{msg}");
    }
    fn warn(&self, msg: &str) {
        tracing::warn!(target: "ax", "{msg}");
    }
    fn error(&self, msg: &str) {
        tracing::error!(target: "ax", "{msg}");
    }
}

static LOGGER: OnceLock<RwLock<Box<dyn Logger>>> = OnceLock::new();

pub fn set_logger(logger: Box<dyn Logger>) {
    let lock = LOGGER.get_or_init(|| RwLock::new(Box::new(DefaultLogger)));
    if let Ok(mut guard) = lock.write() {
        *guard = logger;
    }
}

pub fn get_logger() -> &'static RwLock<Box<dyn Logger>> {
    LOGGER.get_or_init(|| RwLock::new(Box::new(DefaultLogger)))
}

pub fn log_debug(msg: &str) {
    if let Ok(guard) = get_logger().read() {
        guard.debug(msg);
    }
}

pub fn log_info(msg: &str) {
    if let Ok(guard) = get_logger().read() {
        guard.info(msg);
    }
}

pub fn log_warn(msg: &str) {
    if let Ok(guard) = get_logger().read() {
        guard.warn(msg);
    }
}

pub fn log_error(msg: &str) {
    if let Ok(guard) = get_logger().read() {
        guard.error(msg);
    }
}
