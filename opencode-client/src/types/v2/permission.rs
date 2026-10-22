//! opencode v2 permission model (see `vault/Разведка-v2.md` §1.5).
//!
//! The 2.0.32 OpenAPI describes three shapes we need:
//!   - `Permission.Request` — a pending request (the `permission.asked` payload);
//!   - `Permission.Reply` — the decision (`once|always|reject`);
//!   - `PermissionSaved.Info` — a stored "always allow" rule.
//!
//! The reply body of v2 is `{decision, message?}` (v1 used `{response}`).

use serde::{Deserialize, Serialize};

/// A permission request awaiting a user decision (`Permission.Request`).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Human-readable explanation from the server ("Run `rm -rf /`?").
    #[serde(default)]
    pub message: Option<String>,
}

/// Where a permission request originated (`Permission.Source`:
/// `{type:"tool", messageID, id}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "messageID", default)]
    pub message_id: String,
    #[serde(default)]
    pub id: String,
}

/// A stored permission rule (`PermissionSaved.Info`) — the "always allow" the
/// server remembers for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPermission {
    pub id: String,
    #[serde(rename = "projectID", default)]
    pub project_id: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub time: Option<SavedPermissionTime>,
}

/// Timestamps of a saved rule (`{created, updated}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPermissionTime {
    #[serde(default)]
    pub created: f64,
    #[serde(default)]
    pub updated: f64,
}

/// The user's decision on a permission request (`Permission.Reply`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReplyKind {
    Once,
    Always,
    Reject,
}

impl PermissionReplyKind {
    /// Parse the QML-supplied reply name (`once` | `always` | `reject`).
    /// Serde is the single source of the names.
    pub fn from_name(name: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
    }
}
