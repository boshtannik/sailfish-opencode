pub mod config;
pub mod event;
pub mod keys;
pub mod session;
pub mod tool;
pub mod v2;

pub use config::Config;
pub use tool::ToolState;
pub use v2::permission::{PermissionReplyKind, PermissionRequest, PermissionSource};


use serde::Deserialize;

/// Lenient f64: the server sometimes sends an ISO-8601 timestamp string where a
/// number is expected. A failed parse used to drop the whole SSE event silently.
pub fn deserialize_lenient_f64<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match &serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    })
}

/// Lenient optional f64 (same story, nullable).
pub fn deserialize_lenient_f64_opt<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match &serde_json::Value::deserialize(d)? {
        serde_json::Value::Null => None,
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    })
}
