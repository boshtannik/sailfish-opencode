//! opencode v2 message model — faithful translation of the official schema
//! (`packages/schema/src/session-message.ts`, `model.ts`, `tool.ts`,
//! `prompt-input.ts`, `session-error.ts` at upstream `8f7b012`).
//!
//! v1 had a `role`-tagged message plus a separate `Part` stream. v2 replaces
//! both: `Session.Message.Info` is a union by `type` and an assistant message
//! carries its parts inline as `content[]`. Additive — the v1 types stay for the
//! v1 adapter (`vault/План-v2.md` §10 decision #3).
//!
//! Scope is deliberately minimal (stream + history), per `vault/План-v2.md` §8:
//! shapes the UI reads now. Unknown discriminators parse into `Unknown`
//! (`#[serde(other)]`) so a newer server never drops a message; the event layer
//! (step 5) logs those once.

use serde::Deserialize;

/// Common `time` object (`created`, optional `completed`/`streamed`).
#[derive(Debug, Clone, Deserialize)]
pub struct MessageTime {
    pub created: u64,
    #[serde(default)]
    pub streamed: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub ran: Option<u64>,
}

/// `Session.Message.Info` — common base (`id`, `time`, `metadata?`) flattened
/// with the `type`-tagged body.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub time: MessageTime,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(flatten)]
    pub body: MessageBody,
}

/// The `type`-discriminated body of [`MessageInfo`]. `Unknown` catches any new
/// discriminator (serde `other`) so deserialisation never fails on a message
/// kind a future server adds.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum MessageBody {
    #[serde(rename = "agent-switched")]
    AgentSwitched {
        agent: String,
        #[serde(default)]
        previous: Option<String>,
    },
    #[serde(rename = "model-switched")]
    ModelSwitched {
        model: ModelRef,
        #[serde(default)]
        previous: Option<ModelRef>,
    },
    #[serde(rename = "location-switched")]
    LocationSwitched {
        location: serde_json::Value,
        #[serde(default)]
        previous: Option<serde_json::Value>,
    },
    #[serde(rename = "user")]
    User {
        text: String,
        #[serde(default)]
        files: Vec<FileAttachment>,
    },
    #[serde(rename = "synthetic")]
    Synthetic {
        text: String,
        #[serde(default)]
        description: Option<String>,
    },
    #[serde(rename = "system")]
    System {
        text: String,
        #[serde(default)]
        description: Option<String>,
    },
    #[serde(rename = "skill")]
    Skill {
        skill: String,
        name: String,
        text: String,
    },
    #[serde(rename = "shell")]
    Shell {
        shellID: String,
        command: String,
        status: ShellStatus,
        #[serde(default)]
        exit: Option<i64>,
        #[serde(default)]
        output: Option<String>,
        time: MessageTime,
    },
    #[serde(rename = "assistant")]
    Assistant(AssistantFields),
    #[serde(rename = "compaction")]
    Compaction {
        reason: CompactionReason,
        #[serde(flatten)]
        status: CompactionStatus,
    },
    #[serde(rename = "idle")]
    Idle {
        outcome: IdleOutcome,
        #[serde(default)]
        error: Option<SessionError>,
    },
    /// New/unrecognised discriminator from a newer server.
    #[serde(other)]
    Unknown,
}

/// Fields of an `assistant` message (flattened into [`MessageBody::Assistant`]).
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantFields {
    #[serde(default)]
    pub content: Vec<Content>,
    #[serde(default)]
    pub model: Option<ModelRef>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub finish: Option<String>,
    #[serde(default, deserialize_with = "crate::types::deserialize_lenient_f64_opt")]
    pub cost: Option<f64>,
    #[serde(default)]
    pub tokens: Option<TokenUsage>,
    #[serde(default)]
    pub error: Option<SessionError>,
}

/// `assistant.content[]` item — union by `type` (`text|reasoning|tool`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "reasoning")]
    Reasoning {
        text: String,
        #[serde(default)]
        time: Option<MessageTime>,
    },
    #[serde(rename = "tool")]
    Tool {
        id: String,
        name: String,
        state: ToolState,
        #[serde(default)]
        executed: Option<bool>,
        #[serde(default)]
        time: Option<MessageTime>,
    },
    #[serde(other)]
    Unknown,
}

/// Tool-call state — union by `status` (`streaming|running|completed|error`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status")]
pub enum ToolState {
    /// Raw partial tool-call JSON still streaming in (`input` is a string).
    #[serde(rename = "streaming")]
    Streaming { input: String },
    #[serde(rename = "running")]
    Running {
        input: serde_json::Value,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
    },
    #[serde(rename = "completed")]
    Completed {
        input: serde_json::Value,
        content: Vec<ToolContent>,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
    },
    #[serde(rename = "error")]
    Error {
        input: serde_json::Value,
        error: SessionError,
        #[serde(default)]
        content: Option<Vec<ToolContent>>,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
    },
    #[serde(other)]
    Unknown,
}

impl ToolState {
    /// Status for the chat feed: running | completed | error (mirrors the v1
    /// `types::tool::ToolState::ui_status`).
    pub fn ui_status(&self) -> &'static str {
        use crate::types::tool::{STATUS_COMPLETED, STATUS_ERROR, STATUS_RUNNING};
        match self {
            ToolState::Streaming { .. } | ToolState::Running { .. } => STATUS_RUNNING,
            ToolState::Completed { .. } => STATUS_COMPLETED,
            ToolState::Error { .. } => STATUS_ERROR,
            ToolState::Unknown => STATUS_RUNNING,
        }
    }

    /// The tool-call input, or `Null` while still streaming/unknown.
    pub fn input(&self) -> serde_json::Value {
        match self {
            ToolState::Running { input, .. }
            | ToolState::Completed { input, .. }
            | ToolState::Error { input, .. } => input.clone(),
            ToolState::Streaming { input } => serde_json::Value::String(input.clone()),
            ToolState::Unknown => serde_json::Value::Null,
        }
    }

    /// Short description for the inline tool row (mirrors the v1
    /// `types::tool::ToolState::description`): the human-relevant input field
    /// (command / path / pattern / …), else the streamed raw fragment, else the
    /// error message.
    pub fn description(&self) -> String {
        let input = self.input();
        if let Some(s) = input.as_str() {
            return s.to_string();
        }
        for key in crate::types::tool::LABEL_INPUT_KEYS {
            if let Some(s) = input.get(key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        match self {
            ToolState::Error { error, .. } => error.message.clone(),
            _ => String::new(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "file")]
    File {
        uri: String,
        mime: String,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// Compaction message state — union by `status` (`running|completed|failed`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status")]
pub enum CompactionStatus {
    #[serde(rename = "running")]
    Running { summary: String, recent: String },
    #[serde(rename = "completed")]
    Completed {
        summary: String,
        recent: String,
        #[serde(default)]
        model: Option<ModelRef>,
    },
    #[serde(rename = "failed")]
    Failed { error: SessionError },
    #[serde(other)]
    Unknown,
}

/// Why a compaction ran (`auto|manual`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionReason {
    Auto,
    Manual,
}

/// Shell message status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellStatus {
    Running,
    Exited,
    Timeout,
    Killed,
}

/// Terminal outcome of an `idle` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdleOutcome {
    Succeeded,
    Failed,
    Interrupted,
}

/// `Model.Ref {id, providerID, variant?}`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelRef {
    pub id: String,
    pub providerID: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// A prompt/message file attachment (`uri`, optional `name`/`description`).
#[derive(Debug, Clone, Deserialize)]
pub struct FileAttachment {
    pub uri: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `Session.StructuredError {type, message, status?, response?{body}}`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub status: Option<i64>,
}

/// `TokenUsage.Info {input, output, reasoning, cache{read,write}}`.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: Option<CacheUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheUsage {
    pub read: u64,
    pub write: u64,
}
