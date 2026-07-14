#[derive(Debug, Clone)]
pub struct ShipDaemonConfig {
    pub debounce_ms: u64,
}

impl Default for ShipDaemonConfig {
    fn default() -> Self {
        Self { debounce_ms: 300 }
    }
}
