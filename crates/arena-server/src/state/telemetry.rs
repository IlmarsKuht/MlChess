use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Default)]
pub(crate) struct LiveMetricsStore {
    pub(crate) published_events: Arc<AtomicU64>,
    pub(crate) replay_requests: Arc<AtomicU64>,
    pub(crate) replay_events_served: Arc<AtomicU64>,
    pub(crate) snapshot_fallbacks: Arc<AtomicU64>,
    pub(crate) restored_matches: Arc<AtomicU64>,
    pub(crate) websocket_connections: Arc<AtomicU64>,
    pub(crate) move_intent_errors: Arc<AtomicU64>,
    pub(crate) timeout_fires: Arc<AtomicU64>,
}

impl LiveMetricsStore {
    pub(crate) fn snapshot(&self) -> LiveMetricsSnapshot {
        LiveMetricsSnapshot {
            published_events: self.published_events.load(Ordering::Relaxed),
            replay_requests: self.replay_requests.load(Ordering::Relaxed),
            replay_events_served: self.replay_events_served.load(Ordering::Relaxed),
            snapshot_fallbacks: self.snapshot_fallbacks.load(Ordering::Relaxed),
            restored_matches: self.restored_matches.load(Ordering::Relaxed),
            websocket_connections: self.websocket_connections.load(Ordering::Relaxed),
            move_intent_errors: self.move_intent_errors.load(Ordering::Relaxed),
            timeout_fires: self.timeout_fires.load(Ordering::Relaxed),
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct LiveMetricsSnapshot {
    pub(crate) published_events: u64,
    pub(crate) replay_requests: u64,
    pub(crate) replay_events_served: u64,
    pub(crate) snapshot_fallbacks: u64,
    pub(crate) restored_matches: u64,
    pub(crate) websocket_connections: u64,
    pub(crate) move_intent_errors: u64,
    pub(crate) timeout_fires: u64,
}
