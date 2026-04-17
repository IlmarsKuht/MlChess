use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Standard,
    Chess960,
}

impl Variant {
    pub fn is_chess960(self) -> bool {
        matches!(self, Self::Chess960)
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProtocol {
    Uci,
}



#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    #[serde(default = "default_supported_variants")]
    pub supported_variants: Vec<Variant>,
}

impl AgentCapabilities {
    pub fn supports_variant(&self, variant: Variant) -> bool {
        self.supported_variants.contains(&variant)
    }
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            supported_variants: default_supported_variants(),
        }
    }
}

fn default_supported_variants() -> Vec<Variant> {
    vec![Variant::Standard, Variant::Chess960]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub protocol: AgentProtocol,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub documentation: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVersion {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub agent_id: Uuid,
    pub version: String,
    pub active: bool,
    pub executable_path: String,
    pub working_directory: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub capabilities: AgentCapabilities,
    pub declared_name: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub documentation: Option<String>,
    pub created_at: DateTime<Utc>,
}