use serde::Deserialize;

/// Session lifecycle state as the worker sees it (idle / busy / retry).
/// v2 reports it via the `session.status` event body.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "type")]
pub enum SessionStatus {
    #[default]
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "retry")]
    Retry {
        attempt: u32,
        message: String,
        next: u64,
    },
    #[serde(rename = "busy")]
    Busy,
}
