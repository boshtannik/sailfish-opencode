//! opencode v2 permission model (see `vault/Разведка-v2.md` §1.5).
//!
//! v1 permission replies used `{response}`; v2 uses `{decision}` and adds an
//! optional `message`. `PermissionReplyKind` (v1) already serialises to the same
//! `once|always|reject` names, so the reply body reuses it (see the v2 adapter).

use serde::Deserialize;

/// A permission request awaiting a user decision.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub action: String,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub save: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub source: Option<PermissionSource>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Where a permission request originated (`{type:"tool", messageID, id}`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PermissionSource {
    #[serde(rename = "tool")]
    Tool {
        #[serde(rename = "messageID")]
        message_id: String,
        id: String,
    },
    #[serde(other)]
    Unknown,
}
