//! Per-connection record of which policy bodies preflight already delivered.
//!
//! The daemon shares one engine across connections, so state is keyed by
//! connection id (stdio uses 0). A new chat or a long pause starts over,
//! because the agent's context no longer holds the earlier bodies.

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const DEFAULT_TTL: Duration = Duration::from_secs(1800);

#[derive(Debug, Default)]
struct Entry {
    client_name: String,
    chat_id: Option<String>,
    delivered: HashMap<String, u64>,
    last_seen: Option<Instant>,
    chat_session: Option<String>,
}

#[derive(Debug, Default)]
pub struct PolicySessions {
    entries: HashMap<u64, Entry>,
    active: u64,
}

/// What preflight needs to know about the current connection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionView {
    pub client_name: String,
    pub delivered: HashMap<String, u64>,
}

impl PolicySessions {
    pub fn set_active(&mut self, connection: u64) {
        self.active = connection;
    }

    /// `initialize` starts a fresh agent context.
    pub fn begin(&mut self, client_name: &str) {
        self.entries.insert(
            self.active,
            Entry {
                client_name: client_name.to_string(),
                ..Entry::default()
            },
        );
    }

    pub fn view(&mut self, chat_id: Option<&str>, now: Instant, ttl: Duration) -> SessionView {
        let entry = self.entries.entry(self.active).or_default();
        let expired = entry
            .last_seen
            .is_some_and(|seen| now.duration_since(seen) > ttl);
        let chat_changed = entry.chat_id.as_deref() != chat_id;
        if expired || chat_changed {
            entry.delivered.clear();
            entry.chat_id = chat_id.map(str::to_string);
        }
        entry.last_seen = Some(now);
        SessionView {
            client_name: entry.client_name.clone(),
            delivered: entry.delivered.clone(),
        }
    }

    pub fn record(&mut self, delivered: impl IntoIterator<Item = (String, u64)>) {
        self.entries
            .entry(self.active)
            .or_default()
            .delivered
            .extend(delivered);
    }

    /// The chat session this connection used last.
    pub fn connection_session(&self) -> Option<String> {
        self.entries.get(&self.active)?.chat_session.clone()
    }

    pub fn remember_session(&mut self, id: &str) {
        self.entries.entry(self.active).or_default().chat_session = Some(id.to_string());
    }

    pub fn end(&mut self, connection: u64) {
        self.entries.remove(&connection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(60);

    fn delivered(pairs: &[(&str, u64)]) -> HashMap<String, u64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn records_and_returns_delivered_for_same_chat() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.begin("cursor-vscode");
        assert_eq!(s.view(Some("chat-1"), t, TTL).delivered, HashMap::new());
        s.record([("rule:a".to_string(), 7)]);
        let v = s.view(Some("chat-1"), t + Duration::from_secs(5), TTL);
        assert_eq!(v.delivered, delivered(&[("rule:a", 7)]));
        assert_eq!(v.client_name, "cursor-vscode");
    }

    #[test]
    fn new_chat_resets_delivered() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.begin("cursor");
        s.view(Some("chat-1"), t, TTL);
        s.record([("rule:a".to_string(), 7)]);
        assert!(s.view(Some("chat-2"), t, TTL).delivered.is_empty());
    }

    #[test]
    fn ttl_expiry_resets_delivered() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.begin("cursor");
        s.view(None, t, TTL);
        s.record([("rule:a".to_string(), 7)]);
        assert!(s
            .view(None, t + TTL + Duration::from_secs(1), TTL)
            .delivered
            .is_empty());
    }

    #[test]
    fn initialize_resets_delivered() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.begin("cursor");
        s.view(None, t, TTL);
        s.record([("rule:a".to_string(), 7)]);
        s.begin("claude-code");
        let v = s.view(None, t, TTL);
        assert!(v.delivered.is_empty());
        assert_eq!(v.client_name, "claude-code");
    }

    #[test]
    fn connections_are_isolated() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.set_active(1);
        s.begin("cursor");
        s.view(None, t, TTL);
        s.record([("rule:a".to_string(), 7)]);
        s.set_active(2);
        s.begin("cursor");
        assert!(s.view(None, t, TTL).delivered.is_empty());
        s.set_active(1);
        assert_eq!(s.view(None, t, TTL).delivered, delivered(&[("rule:a", 7)]));
    }

    #[test]
    fn chat_session_is_per_connection_and_reset_by_initialize() {
        let mut s = PolicySessions::default();
        s.set_active(1);
        s.begin("cursor");
        assert_eq!(s.connection_session(), None);
        s.remember_session("axs_one");
        s.set_active(2);
        assert_eq!(s.connection_session(), None);
        s.remember_session("axs_two");
        s.set_active(1);
        assert_eq!(s.connection_session().as_deref(), Some("axs_one"));
        s.remember_session("axs_three");
        assert_eq!(s.connection_session().as_deref(), Some("axs_three"));
        s.begin("cursor");
        assert_eq!(s.connection_session(), None);
        s.set_active(2);
        s.end(2);
        assert_eq!(s.connection_session(), None);
    }

    #[test]
    fn missing_initialize_still_tracks() {
        let mut s = PolicySessions::default();
        let t = Instant::now();
        s.view(None, t, TTL);
        s.record([("rule:a".to_string(), 7)]);
        assert_eq!(s.view(None, t, TTL).delivered, delivered(&[("rule:a", 7)]));
    }
}
