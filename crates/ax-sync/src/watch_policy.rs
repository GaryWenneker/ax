//! Watch policy — which events trigger sync.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind { Create, Modify, Remove, Rename }

pub fn should_sync(kind: WatchEventKind) -> bool {
    matches!(kind, WatchEventKind::Modify | WatchEventKind::Remove | WatchEventKind::Rename | WatchEventKind::Create)
}