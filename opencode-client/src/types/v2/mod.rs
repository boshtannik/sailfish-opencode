//! opencode **v2** server types (see `vault/Разведка-v2.md` §1).
//!
//! Additive module: the v1 types in the parent (`types/message.rs`,
//! `types/session.rs`, …) stay for the v1 adapter, which lives on
//! (`vault/План-v2.md` §10 decision #3). Steps 5–6 (events, forms) parse into
//! these as the v2 path is wired up.

pub mod event;
pub mod form;
pub mod message;
pub mod permission;
pub mod session;

pub use event::EventEnvelope;
pub use form::{Field, FormInfo, FormState};
pub use message::{
    AssistantFields, CompactionReason, CompactionStatus, Content, FileAttachment, IdleOutcome,
    MessageBody, MessageInfo, MessageTime, ModelRef, SessionError, ShellStatus, ToolContent,
    ToolState, TokenUsage,
};
pub use permission::{PermissionRequest, PermissionSource};
pub use session::{Fork, Location, SessionInfo, SessionRevert, SessionTime};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(v: serde_json::Value) -> MessageInfo {
        serde_json::from_value(v).expect("message deserialises")
    }

    #[test]
    fn user_message_parses() {
        let m = msg(json!({
            "id": "msg_1", "type": "user", "time": {"created": 5}, "text": "hi"
        }));
        assert_eq!(m.id, "msg_1");
        assert!(matches!(m.body, MessageBody::User { ref text, .. } if text == "hi"));
    }

    #[test]
    fn assistant_message_with_content_parses() {
        let m = msg(json!({
            "id": "msg_2", "type": "assistant", "time": {"created": 6},
            "content": [
                {"type": "text", "text": "answer"},
                {"type": "reasoning", "text": "thinking"},
                {"type": "tool", "id": "t1", "name": "bash",
                 "state": {"status": "completed", "input": {"command": "ls"},
                           "content": [{"type": "text", "text": "out"}]}}
            ]
        }));
        let MessageBody::Assistant(fields) = m.body else {
            panic!("expected assistant");
        };
        let content = fields.content;
        assert_eq!(content.len(), 3);
        assert!(matches!(content[0], Content::Text { .. }));
        assert!(matches!(content[1], Content::Reasoning { .. }));
        assert!(matches!(
            content[2],
            Content::Tool { ref state, .. } if matches!(state, ToolState::Completed { .. })
        ));
    }

    #[test]
    fn streaming_tool_state_has_string_input() {
        let m = msg(json!({
            "id": "msg_3", "type": "assistant", "time": {"created": 7},
            "content": [
                {"type": "tool", "id": "t1", "name": "edit",
                 "state": {"status": "streaming", "input": "{\"file\":"}}
            ]
        }));
        let MessageBody::Assistant(fields) = m.body else { panic!() };
        let content = fields.content;
        assert!(matches!(
            content[0],
            Content::Tool { state: ToolState::Streaming { ref input }, .. } if input == "{\"file\":"
        ));
    }

    #[test]
    fn compaction_message_parses_with_status_and_reason() {
        let m = msg(json!({
            "id": "msg_4", "type": "compaction", "time": {"created": 8},
            "status": "completed", "reason": "auto", "summary": "S", "recent": "R"
        }));
        assert!(matches!(
            m.body,
            MessageBody::Compaction {
                status: CompactionStatus::Completed { .. },
                reason: CompactionReason::Auto
            }
        ));
    }

    #[test]
    fn unknown_message_type_is_caught_not_dropped() {
        let m = msg(json!({
            "id": "msg_5", "type": "future-thing", "time": {"created": 9}
        }));
        assert!(matches!(m.body, MessageBody::Unknown));
    }

    #[test]
    fn session_info_parses_env_shape() {
        let s: SessionInfo = serde_json::from_value(json!({
            "id": "ses_1", "projectID": "p", "cost": 0.5,
            "tokens": {"input": 10, "output": 20, "reasoning": 0},
            "time": {"created": 1, "updated": 2},
            "title": "T", "location": {"directory": "/home"}
        }))
        .expect("session deserialises");
        assert_eq!(s.id, "ses_1");
        assert_eq!(s.title.as_deref(), Some("T"));
    }

    #[test]
    fn permission_request_parses() {
        let p: PermissionRequest = serde_json::from_value(json!({
            "id": "per_1", "sessionID": "ses_1", "action": "bash",
            "resources": ["ls"],
            "source": {"type": "tool", "messageID": "msg_1", "id": "call_1"}
        }))
        .expect("permission deserialises");
        assert_eq!(p.action, "bash");
        assert!(matches!(p.source, Some(PermissionSource::Tool { .. })));
    }

    #[test]
    fn form_parses_fields_union() {
        let f: FormInfo = serde_json::from_value(json!({
            "id": "frm_1", "sessionID": "ses_1", "title": "Ask",
            "fields": [
                {"type": "string", "id": "a", "label": "A"},
                {"type": "boolean", "id": "b", "label": "B"}
            ]
        }))
        .expect("form deserialises");
        assert_eq!(f.fields.len(), 2);
        assert!(matches!(f.fields[0], Field::String { .. }));
        assert!(matches!(f.fields[1], Field::Boolean { .. }));
    }
}

