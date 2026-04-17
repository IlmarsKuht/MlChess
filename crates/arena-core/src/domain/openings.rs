use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Variant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningSourceKind {
    Starter,
    FenList,
    PgnImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningPosition {
    pub id: Uuid,
    pub suite_id: Uuid,
    pub label: String,
    pub fen: String,
    pub variant: Variant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningSuite {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub source_kind: OpeningSourceKind,
    pub source_text: Option<String>,
    pub active: bool,
    pub starter: bool,
    pub positions: Vec<OpeningPosition>,
    pub created_at: DateTime<Utc>,
}