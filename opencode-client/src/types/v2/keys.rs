//! JSON field-name and event-type constants for the opencode **v2** wire format.
//!
//! Same rule as `types/keys.rs`: logic never compares or indexes JSON by a raw
//! string literal, because a typo in a bare `"..."` is invisible to the
//! compiler while a typo in a `keys::` constant is a compile error. Keep these
//! in sync with the `serde` attributes in `types/v2/*` and with the live
//! server (opencode 1.19.0 bundles the exact event names — see
//! `vault/План-v2.md` §12).
//!
//! Event-type constants are `&str` so they can be used directly as `match`
//! patterns in `event::dispatcher::dispatch_v2`.
//!
//! Target server: opencode **2.0.32** (the real OpenCode 2; see
//! `vault/Разведка-v2.md` §10). 2.0.32 emits the plain `session.*` event names
//! below, with no `session.next.` bridge prefix and no v1 routes.

// ── `EventEnvelope` fields (`vault/Разведка-v2.md` §3) ───────────────────
pub const ID: &str = "id";
pub const TYPE: &str = "type";
pub const DATA: &str = "data";
pub const LOCATION: &str = "location";
pub const METADATA: &str = "metadata";
pub const DURABLE: &str = "durable";
pub const AGGREGATE_ID: &str = "aggregateID";
pub const SEQ: &str = "seq";
pub const VERSION: &str = "version";

// ── v2 `data` body field names ───────────────────────────────────────────
pub const SESSION_ID: &str = "sessionID";
pub const MESSAGE_ID: &str = "messageID";
pub const ASSISTANT_MESSAGE_ID: &str = "assistantMessageID";
pub const ORDINAL: &str = "ordinal";
pub const DELTA: &str = "delta";
pub const TEXT: &str = "text";
pub const NAME: &str = "name";
pub const PROVIDER_ID: &str = "providerID";
pub const MODEL: &str = "model";
pub const ERROR: &str = "error";
pub const MESSAGE: &str = "message";
pub const REQUEST_ID: &str = "requestID";
pub const INTERRUPTED: &str = "interrupted";
pub const INPUT: &str = "input";
pub const CONTENT: &str = "content";
pub const PROMPT: &str = "prompt";
pub const AGENTS: &str = "agents";

// ── request-body fields ──────────────────────────────────────────────────
pub const TITLE: &str = "title";
pub const AGENT: &str = "agent";
pub const DECISION: &str = "decision";
pub const BEFORE: &str = "before";
pub const REASON: &str = "reason";

// ── permission-reply decision values (`{decision}` body) ─────────────────
pub const DECISION_ONCE: &str = "once";
pub const DECISION_ALWAYS: &str = "always";
pub const DECISION_REJECT: &str = "reject";

// ── v1-shaped provider/model catalog (built for the v1 UI) ────────────────
pub const PROVIDERS: &str = "providers";
pub const MODELS: &str = "models";

// ── Permission source discriminators ─────────────────────────────────────
pub const SOURCE_TOOL: &str = "tool";
pub const SOURCE_UNKNOWN: &str = "unknown";

/// Fallback tool title shown when neither the call input nor the tool name is
/// known (mirrors the v1 dispatcher's default).
pub const DEFAULT_TOOL_TITLE: &str = "tool";

// ── Event types: live agent-loop stream. opencode 2.0.32 emits these plain
// `session.*` names (no `session.next.` bridge prefix). ───────────────────
pub const STEP_STARTED: &str = "session.step.started";
pub const STEP_ENDED: &str = "session.step.ended";
pub const STEP_STREAMED: &str = "session.step.streamed";
pub const STEP_FAILED: &str = "session.step.failed";
pub const TEXT_STARTED: &str = "session.text.started";
pub const TEXT_DELTA: &str = "session.text.delta";
pub const TEXT_ENDED: &str = "session.text.ended";
pub const REASONING_STARTED: &str = "session.reasoning.started";
pub const REASONING_DELTA: &str = "session.reasoning.delta";
pub const REASONING_ENDED: &str = "session.reasoning.ended";
pub const TOOL_INPUT_STARTED: &str = "session.tool.input.started";
pub const TOOL_INPUT_DELTA: &str = "session.tool.input.delta";
pub const TOOL_INPUT_ENDED: &str = "session.tool.input.ended";
pub const TOOL_CALLED: &str = "session.tool.called";
pub const TOOL_PROGRESS: &str = "session.tool.progress";
pub const TOOL_SUCCESS: &str = "session.tool.success";
pub const TOOL_FAILED: &str = "session.tool.failed";
pub const COMPACTION_STARTED: &str = "session.compaction.started";
pub const COMPACTION_DELTA: &str = "session.compaction.delta";
pub const COMPACTION_ENDED: &str = "session.compaction.ended";
pub const COMPACTION_FAILED: &str = "session.compaction.failed";
pub const SYNTHETIC: &str = "session.synthetic";
pub const REVERT_STAGED: &str = "session.revert.staged";
pub const REVERT_CLEARED: &str = "session.revert.cleared";
pub const REVERT_COMMITTED: &str = "session.revert.committed";
pub const SHELL_STARTED: &str = "session.shell.started";
pub const SHELL_ENDED: &str = "session.shell.ended";

// ── Session/message lifecycle events ────────────────────────────────────
pub const RENAMED: &str = "session.renamed";
pub const MOVED: &str = "session.moved";
pub const AGENT_SELECTED: &str = "session.agent.selected";
pub const MODEL_SELECTED: &str = "session.model.selected";
pub const MESSAGE_CONTENT_UPDATED: &str = "session.message.content.updated";
pub const USAGE_RECORDED: &str = "session.usage.recorded";
pub const USAGE_UPDATED: &str = "session.usage.updated";
pub const EXECUTION_STARTED: &str = "session.execution.started";
pub const EXECUTION_FAILED: &str = "session.execution.failed";
pub const EXECUTION_INTERRUPTED: &str = "session.execution.interrupted";
pub const EXECUTION_SUCCEEDED: &str = "session.execution.succeeded";
pub const DELETED: &str = "session.deleted";
pub const IDLE: &str = "session.idle";
pub const STATUS: &str = "session.status";

// ── Catalog / config invalidation events (2.0.32) ───────────────────────
// The server emits these when a config-backed catalog changes (a new model,
// provider, agent, skill, …). We recognise them so they are not reported as
// unknown; the UI re-reads the affected list lazily (no live-accumulator
// effect here yet).
pub const PROJECT_UPDATED: &str = "project.updated";
pub const INTEGRATION_UPDATED: &str = "integration.updated";
pub const PROVIDER_UPDATED: &str = "provider.updated";
pub const MODEL_UPDATED: &str = "model.updated";
pub const AGENT_UPDATED: &str = "agent.updated";
pub const COMMAND_UPDATED: &str = "command.updated";
pub const SKILL_UPDATED: &str = "skill.updated";
pub const WEBSEARCH_UPDATED: &str = "websearch.updated";
pub const REFERENCE_UPDATED: &str = "reference.updated";
pub const PLUGIN_UPDATED: &str = "plugin.updated";

// ── Permissions / questions / forms / server ─────────────────────────────
pub const PERMISSION_ASKED: &str = "permission.asked";
pub const PERMISSION_REPLIED: &str = "permission.replied";
pub const QUESTION_ASKED: &str = "question.asked";
pub const QUESTION_REPLIED: &str = "question.replied";
pub const QUESTION_REJECTED: &str = "question.rejected";
pub const FORM_CREATED: &str = "form.created";
pub const FORM_REPLIED: &str = "form.replied";
pub const FORM_CANCELLED: &str = "form.cancelled";
pub const SERVER_CONNECTED: &str = "server.connected";
