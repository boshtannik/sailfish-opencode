//! opencode **v2** SSE event model (see `vault/Разведка-v2.md` §2–§3).
//!
//! Faithful translation of the official generated TS client
//! (`@opencode-ai/client@next`, `types.d.ts`, `V2Event`) — the source of truth
//! for the v2 event union. Frames arrive on `GET /api/event` as
//! `data: {envelope}`; the envelope (`EventEnvelope`) carries
//! `{id, type, created, data, location?, metadata?, durable?}`.
//!
//! Unlike v1's `GlobalEvent` (where the payload is nested under `payload` and
//! properties under `properties`), the v2 envelope flattens everything and puts
//! the event-specific body under `data`.
//!
//! `V2Event` keeps `type` as a plain `String`: exhaustive matching happens in
//! `event::dispatcher::dispatch_v2`, and an unrecognised type is not an error
//! (a newer server must never drop the frame — `event::sse` logs it once). This
//! mirrors the project rule: unknown server capabilities are surfaced via
//! `note_unimplemented`, never a parse failure.
//!
//! Only envelopes carry a `type` tag; the fields we actually read in the
//! streaming/history paths are typed, the rest stay as `serde_json::Value` to
//! stay faithful without inventing structure.

use serde::Deserialize;

/// A v2 SSE frame (`EventEnvelope`, `vault/Разведка-v2.md` §3).
///
/// `data` is a nested JSON object (not a JSON string). `durable` is present only
/// on durable events; `*.delta` frames are ephemeral and omit it.
#[derive(Debug, Clone, Deserialize)]
pub struct EventEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub location: Option<serde_json::Value>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub durable: Option<Durable>,
}

/// `durable: {aggregateID, seq, version}` — replay metadata of a durable event.
#[derive(Debug, Clone, Deserialize)]
pub struct Durable {
    #[serde(rename = "aggregateID")]
    pub aggregate_id: String,
    pub seq: u64,
    pub version: u64,
}

// ── `session.*` content stream (`data` bodies) ───────────────────────────
//
// Types below name the actual fields the client reads. Optional fields are
// `Option` so a newer server that adds/removes one keeps deserialising.

/// `session.step.started` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct StepStartedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<serde_json::Value>,
    #[serde(default)]
    pub snapshot: Option<String>,
}

/// `session.step.ended` / `session.step.failed` `data` (terminal step info).
#[derive(Debug, Clone, Deserialize)]
pub struct StepEndedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    #[serde(default)]
    pub finish: Option<String>,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
    #[serde(default)]
    pub cost: Option<serde_json::Value>,
    #[serde(default)]
    pub tokens: Option<serde_json::Value>,
}

/// `session.text.started` / `session.reasoning.started` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct TextStartedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub ordinal: i64,
}

/// `session.text.delta` / `session.reasoning.delta` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct TextDeltaData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub ordinal: i64,
    pub delta: String,
}

/// `session.text.ended` / `session.reasoning.ended` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct TextEndedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub ordinal: i64,
    pub text: String,
}

/// `session.tool.input.started` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInputStartedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    pub name: String,
}

/// `session.tool.input.delta` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInputDeltaData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    pub delta: String,
}

/// `session.tool.input.ended` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInputEndedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    pub text: String,
}

/// `session.tool.called` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCalledData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub executed: Option<bool>,
}

/// `session.tool.progress` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolProgressData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// `session.tool.success` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolSuccessData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// `session.tool.failed` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolFailedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "assistantMessageID")]
    pub assistant_message_id: String,
    pub id: String,
    #[serde(default)]
    pub error: serde_json::Value,
    #[serde(default)]
    pub content: Option<Vec<serde_json::Value>>,
}

/// `session.compaction.started` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct CompactionStartedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub reason: String,
    #[serde(default)]
    pub recent: String,
}

/// `session.compaction.delta` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct CompactionDeltaData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub text: String,
}

/// `session.compaction.ended` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct CompactionEndedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub reason: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub recent: String,
}

/// `session.compaction.failed` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct CompactionFailedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub reason: String,
    #[serde(default)]
    pub error: serde_json::Value,
}

/// `session.idle` / `session.status` — used by the main loop for turn end.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionIdleData {
    pub sessionID: String,
}

/// `session.model.selected` `data` — the session's current model.
/// The v2 server emits this only when the model is (re)selected; `GET
/// /api/session[/{id}]` does not carry the model, so this is how the UI learns
/// the active model for a session.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelSelectedData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub model: super::message::ModelRef,
}

/// `session.status` `data` (`status` is a tagged union `idle|busy|retry`).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionStatusData {
    pub sessionID: String,
    pub status: SessionStatusValue,
}

/// `session.status.status` — union by `type` (`idle|busy|retry`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionStatusValue {
    Idle,
    Busy,
    Retry {
        #[serde(default)]
        attempt: u32,
        #[serde(default)]
        message: String,
        #[serde(default)]
        next: u64,
    },
    #[serde(other)]
    Unknown,
}

/// `permission.asked` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionAskedData {
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
    pub source: Option<super::permission::PermissionSource>,
    #[serde(default)]
    pub message: Option<String>,
}

/// `permission.replied` `data`.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionRepliedData {
    pub requestID: String,
}

/// `form.created` `data` (`{form: Form.Info}`).
#[derive(Debug, Clone, Deserialize)]
pub struct FormCreatedData {
    pub form: super::form::FormInfo,
}

/// `form.replied` / `form.cancelled` `data`.
///
/// The server sends the settled form's `id` (and, for `form.replied`, the
/// answer that was stored). `sessionID` is optional: the official TUI only
/// reads `data.id`, and a global form carries no session.
#[derive(Debug, Clone, Deserialize)]
pub struct FormResolvedData {
    pub id: String,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub answer: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_parses_without_durable() {
        let env: EventEnvelope = serde_json::from_value(json!({
            "id": "evt_1",
            "type": "session.text.delta",
            "created": 1710000000000u64,
            "data": {
                "sessionID": "ses_1",
                "assistantMessageID": "msg_1",
                "ordinal": 0,
                "delta": "hi"
            }
        }))
        .expect("envelope parses");
        assert_eq!(env.kind, "session.text.delta");
        assert!(env.durable.is_none());
        let d: TextDeltaData = serde_json::from_value(env.data).unwrap();
        assert_eq!(d.ordinal, 0);
        assert_eq!(d.delta, "hi");
    }

    #[test]
    fn envelope_parses_with_durable() {
        let env: EventEnvelope = serde_json::from_value(json!({
            "id": "evt_2",
            "type": "session.created",
            "created": 5,
            "durable": { "aggregateID": "ses_1", "seq": 1, "version": 1 },
            "data": { "sessionID": "ses_1", "projectID": "p", "slug": "s" }
        }))
        .expect("envelope parses");
        let dur = env.durable.expect("durable present");
        assert_eq!(dur.seq, 1);
        assert_eq!(dur.aggregate_id, "ses_1");
    }

    #[test]
    fn unknown_type_keeps_parsing() {
        let env: EventEnvelope = serde_json::from_value(json!({
            "id": "evt_3",
            "type": "some.future.event",
            "created": 1,
            "data": { "whatever": 1 }
        }))
        .expect("unknown kind still parses");
        assert_eq!(env.kind, "some.future.event");
    }

    #[test]
    fn session_status_retry_parses() {
        let s: SessionStatusData = serde_json::from_value(json!({
            "sessionID": "ses_1",
            "status": { "type": "retry", "attempt": 2, "message": "quota", "next": 99 }
        }))
        .unwrap();
        assert!(matches!(
            s.status,
            SessionStatusValue::Retry { attempt: 2, .. }
        ));
    }

    #[test]
    fn server_connected_still_parses() {
        let env: EventEnvelope = serde_json::from_value(json!({
            "id": "evt_0",
            "type": "server.connected",
            "data": {}
        }))
        .expect("server.connected parses");
        assert_eq!(env.kind, "server.connected");
    }
}
