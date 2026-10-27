//! JSON field-name and event-type constants for the opencode **v2** wire format.
//!
//! Same rule as `types/keys.rs`: logic never compares or indexes JSON by a raw
//! string literal, because a typo in a bare `"..."` is invisible to the
//! compiler while a typo in a `keys::` constant is a compile error. Keep these
//! in sync with the `serde` attributes in `types/v2/*` and with the live
//! server (opencode 2.1.4 bundles the exact event names — see
//! `vault/Разведка-v2.md` §10).
//!
//! Event-type constants are `&str` so they can be used directly as `match`
//! patterns in `event::dispatcher::dispatch_v2`.
//!
//! Target server: opencode **2.1.4** (the real OpenCode 2; see
//! `vault/Разведка-v2.md` §10). 2.1.x emits the plain `session.*` event names
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
pub const AGENTS: &str = "agents";

// ── structured-error `type` discriminator values ─────────────────────────
/// `error.type` of a user-initiated abort (`step.failed`); a clean stop, not a
/// failure, so it must not paint the error banner (v1 parity).
pub const ERR_ABORTED: &str = "aborted";
/// `error.type` of the benign "nothing to compact yet" compaction failure.
pub const ERR_COMPACTION_UNAVAILABLE: &str = "compaction.unavailable";

// ── request-body fields ──────────────────────────────────────────────────
pub const TITLE: &str = "title";
pub const AGENT: &str = "agent";
pub const DECISION: &str = "decision";
pub const BEFORE: &str = "before";
pub const REASON: &str = "reason";
/// `{methodID}` of an integration connect request.
pub const METHOD_ID: &str = "methodID";
/// `{code}` of an integration OAuth `complete` request (`null` = none needed).
pub const CODE: &str = "code";

// ── form field names (`Form.*` / the `{answer:{…}}` reply body) ───────────
pub const KEY: &str = "key";
pub const FIELDS: &str = "fields";
pub const OPTIONS: &str = "options";
pub const VALUE: &str = "value";
pub const LABEL: &str = "label";
pub const DESCRIPTION: &str = "description";
pub const REQUIRED: &str = "required";
pub const DEFAULT: &str = "default";
pub const ANSWER: &str = "answer";
pub const WHEN: &str = "when";
pub const OP: &str = "op";
pub const URL: &str = "url";

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

// ── Event types: live agent-loop stream. opencode 2.1.x emits these plain
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

// ── Catalog / config invalidation events (2.1.x) ─────────────────────────
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

// ── Permissions / forms / server ─────────────────────────────────────────
pub const PERMISSION_ASKED: &str = "permission.asked";
pub const PERMISSION_REPLIED: &str = "permission.replied";
pub const PERMISSION_REJECTED: &str = "permission.rejected";
pub const FORM_CREATED: &str = "form.created";
pub const FORM_REPLIED: &str = "form.replied";
pub const FORM_CANCELLED: &str = "form.cancelled";
pub const SERVER_CONNECTED: &str = "server.connected";

// ── Form field `type` discriminators (`Form.Field.type`) ─────────────────
pub const FIELD_STRING: &str = "string";
pub const FIELD_NUMBER: &str = "number";
pub const FIELD_INTEGER: &str = "integer";
pub const FIELD_BOOLEAN: &str = "boolean";
pub const FIELD_MULTISELECT: &str = "multiselect";
pub const FIELD_EXTERNAL: &str = "external";

// ── Form field visibility condition ops (`Form.When.op`) ─────────────────
pub const COND_EQ: &str = "eq";
pub const COND_NEQ: &str = "neq";

// ── Full 2.1.4 registry: events we consciously recognise (handled by an arm
// or ignored on purpose). The completeness test
// (`types::v2::tests::every_2_1_4_event_is_recognised`) asserts that every
// name in `tests/fixtures/v2-events-2.1.4.txt` is listed in
// `event::dispatcher::RECOGNISED_V2`, so a NEW upstream event fails the build
// instead of silently logging. See `scripts/gen-v2-fixtures.sh`.
//
// Lifecycle / session metadata
pub const SESSION_CREATED: &str = "session.created";
pub const SESSION_UPDATED: &str = "session.updated";
pub const SESSION_FORKED: &str = "session.forked";
pub const SESSION_VIEWED: &str = "session.viewed";
/// 2.1.4 replaced the old `session.compacted` with an archive flag on the
/// session (the compaction *job* events below are unchanged).
pub const SESSION_ARCHIVED: &str = "session.archived";
pub const SESSION_UNARCHIVED: &str = "session.unarchived";
pub const SESSION_DIFF: &str = "session.diff";
pub const SESSION_ERROR: &str = "session.error";
pub const SESSION_PERMISSIONS: &str = "session.permissions";
pub const SESSION_RETRY_SCHEDULED: &str = "session.retry.scheduled";
pub const SESSION_SKILL_ACTIVATED: &str = "session.skill.activated";
pub const SESSION_INSTRUCTIONS_UPDATED: &str = "session.instructions.updated";
pub const SESSION_METADATA_UPDATED: &str = "session.metadata.updated";
// Session inbox (durable prompt queue)
pub const SESSION_INBOX_ENQUEUED: &str = "session.inbox.enqueued";
pub const SESSION_INBOX_DELIVERED: &str = "session.inbox.delivered";
pub const SESSION_INBOX_CANCELLED: &str = "session.inbox.cancelled";
pub const SESSION_INBOX_DELIVERY_CHANGED: &str = "session.inbox.delivery.changed";
// Top-level compaction *job* events (distinct from the `session.compaction.*`
// family above: those are per-session stream updates, these are queue/job
// lifecycle notifications for the compaction worker).
pub const COMPACTION_JOB_FAILED: &str = "compaction.failed";
pub const COMPACTION_JOB_INTERRUPTED: &str = "compaction.interrupted";
pub const COMPACTION_JOB_UNAVAILABLE: &str = "compaction.unavailable";
// Tool-internal diagnostics
pub const TOOL_EXECUTION: &str = "tool.execution";
pub const TOOL_INPUT_INCOMPLETE: &str = "tool.input-incomplete";
pub const TOOL_INPUT_JSON: &str = "tool.input-json";
pub const TOOL_INTERRUPTED: &str = "tool.interrupted";
pub const TOOL_RESULT_MISSING: &str = "tool.result-missing";
pub const TOOL_UNKNOWN: &str = "tool.unknown";
// Message lifecycle (v1 names; still on the bus)
pub const MESSAGE_UPDATED: &str = "message.updated";
pub const MESSAGE_REMOVED: &str = "message.removed";
pub const MESSAGE_PART_UPDATED: &str = "message.part.updated";
pub const MESSAGE_PART_DELTA: &str = "message.part.delta";
pub const MESSAGE_PART_REMOVED: &str = "message.part.removed";
// Provider diagnostics
pub const PROVIDER_AUTH: &str = "provider.auth";
pub const PROVIDER_CONTENT_FILTER: &str = "provider.content-filter";
pub const PROVIDER_ERROR: &str = "provider.error";
pub const PROVIDER_NO_ROUTE: &str = "provider.no-route";
pub const PROVIDER_UNKNOWN: &str = "provider.unknown";
pub const PROVIDER_UNSUPPORTED_OPERATION: &str = "provider.unsupported-operation";
// PTY (terminal) lifecycle
pub const PTY_CREATED: &str = "pty.created";
pub const PTY_UPDATED: &str = "pty.updated";
pub const PTY_DELETED: &str = "pty.deleted";
pub const PTY_EXITED: &str = "pty.exited";
pub const PERSISTENT_PTY_ADDED: &str = "persistent-pty.added";
pub const PERSISTENT_PTY_REMOVED: &str = "persistent-pty.removed";
// Shell lifecycle
pub const SHELL_CREATED: &str = "shell.created";
pub const SHELL_DELETED: &str = "shell.deleted";
pub const SHELL_EXITED: &str = "shell.exited";
// Workspace / worktree
pub const WORKSPACE_READY: &str = "workspace.ready";
pub const WORKSPACE_FAILED: &str = "workspace.failed";
pub const WORKSPACE_STATUS: &str = "workspace.status";
pub const WORKTREE_READY: &str = "worktree.ready";
pub const WORKTREE_FAILED: &str = "worktree.failed";
pub const WORKTREE_RESOLVED: &str = "worktree.resolved";
pub const WORKTREE_UPDATED: &str = "worktree.updated";
// mcp / lsp / vcs / fs / file / config / credential
pub const MCP_TOOLS_CHANGED: &str = "mcp.tools.changed";
pub const MCP_PROMPTS_CHANGED: &str = "mcp.prompts.changed";
pub const MCP_RESOURCES_CHANGED: &str = "mcp.resources.changed";
pub const MCP_STATUS_CHANGED: &str = "mcp.status.changed";
pub const LSP_UPDATED: &str = "lsp.updated";
pub const VCS_BRANCH_UPDATED: &str = "vcs.branch.updated";
pub const FILESYSTEM_CHANGED: &str = "filesystem.changed";
pub const FILE_EDITED: &str = "file.edited";
pub const CONFIG_UPDATED: &str = "config.updated";
pub const CREDENTIAL_UPDATED: &str = "credential.updated";
pub const CREDENTIAL_SWITCHED: &str = "credential.switched";
pub const INSTRUCTION_DISCOVERY_UPDATED: &str = "instruction-discovery.updated";
pub const MODELS_DEV_REFRESHED: &str = "models-dev.refreshed";
pub const WELLKNOWN_UPDATED: &str = "wellknown.updated";
// Command / installation / update
pub const COMMAND_EXECUTED: &str = "command.executed";
pub const INSTALLATION_UPDATED: &str = "installation.updated";
pub const INSTALLATION_UPDATE_AVAILABLE: &str = "installation.update-available";
// Turn / queue
pub const TURN_IDLE: &str = "turn.idle";
pub const TURN_SEND: &str = "turn.send";
pub const TURN_DURATION: &str = "turn.duration";
pub const QUEUED_PROMPTS: &str = "queued.prompts";
// SDK plugin (emitted by the JS SDK layer, not the core bus)
pub const SDK_PLUGIN_UPDATED: &str = "sdk.plugin.updated";
// TUI-driven events (no UI effect in this client; recognised to stay quiet)
pub const TUI_COMMAND_EXECUTE: &str = "tui.command.execute";
pub const TUI_PROMPT_APPEND: &str = "tui.prompt.append";
pub const TUI_SESSION_SELECT: &str = "tui.session.select";
pub const TUI_TOAST_SHOW: &str = "tui.toast.show";
// UI / transport / rpc internals (never relevant to this client, but present
// in the registry; recognised so the strict completeness test passes and a
// rename upstream still gets caught by the fixture diff).
pub const CLIENT_CONNECTION: &str = "client.connection";
pub const GLOBAL_DISPOSED: &str = "global.disposed";
pub const LOCATION_SHUTDOWN: &str = "location.shutdown";
pub const LOG_SYNCED: &str = "log.synced";
pub const RPC_INTERNAL: &str = "rpc.internal";
pub const RESPONSE_CREATE: &str = "response.create";
pub const RESPONSE_OUTPUT_ITEM_DONE: &str = "response.output_item.done";
pub const STREAM_PATCH: &str = "stream.patch";
pub const STREAM_SUBAGENT: &str = "stream.subagent";
pub const STREAM_VIEW: &str = "stream.view";
pub const UI_ARROW: &str = "ui.arrow";
pub const UI_CLICK: &str = "ui.click";
pub const UI_ENTER: &str = "ui.enter";
pub const UI_FOCUS: &str = "ui.focus";
pub const UI_MOUSE: &str = "ui.mouse";
pub const UI_PRESS: &str = "ui.press";
pub const UI_RESIZE: &str = "ui.resize";
pub const UI_TYPE: &str = "ui.type";
