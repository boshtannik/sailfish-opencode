//! opencode v2 session model — faithful translation of `Session.Info`
//! (`packages/schema/src/session.ts:31`, upstream `8f7b012`).
//!
//! Additive alongside the v1 `Session`; the v1 adapter keeps its own shape.

use serde::Deserialize;

use super::message::{IdleOutcome, ModelRef, TokenUsage};

/// A fork origin (`{sessionID, boundary}`).
#[derive(Debug, Clone, Deserialize)]
pub struct Fork {
    #[serde(rename = "sessionID")]
    pub sessionID: String,
    pub boundary: serde_json::Value,
}

/// `Session.Info` as returned by `GET /api/session/{id}` (inside `{data: …}`).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub parentID: Option<String>,
    #[serde(default)]
    pub fork: Option<Fork>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<ModelRef>,
    #[serde(default, deserialize_with = "crate::types::deserialize_lenient_f64")]
    pub cost: f64,
    pub tokens: TokenUsage,
    #[serde(default)]
    pub outcome: Option<IdleOutcome>,
    pub time: SessionTime,
    #[serde(default)]
    pub title: Option<String>,
    pub location: Location,
    #[serde(default)]
    pub subpath: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub revert: Option<SessionRevert>,
}

/// Session timestamps (`created`/`updated` always present; the rest optional).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionTime {
    pub created: u64,
    pub updated: u64,
    #[serde(default)]
    pub idle: Option<u64>,
    #[serde(default)]
    pub viewed: Option<u64>,
    #[serde(default)]
    pub archived: Option<u64>,
}

/// Where a session lives (`{directory}`).
#[derive(Debug, Clone, Deserialize)]
pub struct Location {
    pub directory: String,
}

/// Session revert marker `{messageID, partID?, snapshot?, files?}`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionRevert {
    #[serde(rename = "messageID")]
    pub messageID: String,
    #[serde(default)]
    pub partID: Option<String>,
    #[serde(default)]
    pub snapshot: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
}
