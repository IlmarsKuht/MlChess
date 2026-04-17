use std::{collections::HashMap, sync::Arc};

use uuid::Uuid;

use crate::match_runtime::types::HumanGameHandle;

#[derive(Clone, Default)]
pub(crate) struct HumanGameStore {
    sessions: Arc<tokio::sync::RwLock<HashMap<Uuid, HumanGameHandle>>>,
}

impl HumanGameStore {
    pub(crate) async fn insert(&self, match_id: Uuid, session: HumanGameHandle) {
        self.sessions.write().await.insert(match_id, session);
    }

    pub(crate) async fn get(&self, match_id: Uuid) -> Option<HumanGameHandle> {
        self.sessions.read().await.get(&match_id).cloned()
    }

    pub(crate) async fn remove(&self, match_id: Uuid) -> Option<HumanGameHandle> {
        self.sessions.write().await.remove(&match_id)
    }
}
