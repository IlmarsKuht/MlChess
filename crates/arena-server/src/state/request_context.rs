use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestContext {
    pub(crate) request_id: Uuid,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) client_route: Option<String>,
    pub(crate) client_ts: Option<String>,
    pub(crate) method: String,
    pub(crate) route: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MoveDebugContext {
    pub(crate) request_id: Option<Uuid>,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) ws_connection_id: Option<Uuid>,
    pub(crate) intent_id: Uuid,
    pub(crate) move_uci: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestJournalEntry {
    pub(crate) request_id: Uuid,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) client_route: Option<String>,
    pub(crate) client_ts: Option<String>,
    pub(crate) method: String,
    pub(crate) route: String,
    pub(crate) status_code: u16,
    pub(crate) match_id: Option<Uuid>,
    pub(crate) tournament_id: Option<Uuid>,
    pub(crate) game_id: Option<Uuid>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) completed_at: DateTime<Utc>,
    pub(crate) duration_ms: i64,
    pub(crate) error_text: Option<String>,
}
