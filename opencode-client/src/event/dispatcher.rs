use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::i18n::{err_line, now_ms, svc_line};
use crate::markers::{
    proto_live_line, proto_tool_line, ANSWER_LIVE_TAG, SET_MSG_ID_TAG, THINK_LIVE_TAG,
};
use crate::state::permission::PermissionQueue;
use crate::state::question::QuestionQueue;
use crate::state::session::SessionFsm;
use crate::state::tool::ToolStore;
use crate::types::event::{
    human_session_error, Event, MessagePartDeltaProperties, MessageUpdatedProperties,
    SessionErrorProperties, SessionNextShellEndedProperties, SessionNextShellStartedProperties,
    SessionNextToolCalledProperties, SessionNextToolFailedProperties,
    SessionNextToolInputDeltaProperties, SessionNextToolInputEndedProperties,
    SessionNextToolInputStartedProperties, SessionNextToolProgressProperties,
    SessionNextToolSuccessProperties,
};
use crate::types::message::Message;
use crate::types::permission::Permission;
use crate::types::tool::ToolState;
use crate::types::v2::EventEnvelope;
use crate::types::QuestionRequest;

/// Minimum interval between two live pushes (reasoning or answer) — throttles a
/// long model turn so the QML queue is not flooded with tiny chunks. The final
/// full text is always sent once at `message.part.updated`/idle.
const REASONING_LIVE_MS: u64 = 150;

/// Display name for the agent's persistent shell (`session.next.shell.*`),
/// which carries a command but no tool name of its own.
const SHELL_TOOL_NAME: &str = "bash";

/// Handles the streaming accumulators need: the QML out-queue and the status
/// string, plus the per-connection accumulators for the live chat rendering.
///
/// Built by the SSE loop for every event. It is deliberately *this* module,
/// and not the caller, that decides how a message event touches these
/// accumulators — the caller only lends them. This keeps the "which server
/// event does what" responsibility in one place (`dispatch`, below).
pub struct StreamState<'a> {
    /// Lines queued for QML (`worker_pending`).
    pub pending: &'a std::sync::Mutex<Vec<String>>,
    /// Status token shown by QML (`idle`/`busy`/`error`/…).
    pub status: &'a std::sync::Mutex<String>,
    /// IDs of assistant messages — only their text parts feed the answer.
    pub assistant_messages: &'a mut HashSet<String>,
    /// IDs of `mode:"compaction"` assistant messages — the summary of the
    /// compressed history. Their text streams into the transcript exactly like
    /// a normal answer (mirrors the TUI's growing compaction bubble); the
    /// caller reads this set to keep the summary out of auto-TTS.
    pub compaction_messages: &'a mut HashSet<String>,
    /// Reasoning parts accumulated by part.id.
    pub reasonings: &'a mut HashMap<String, String>,
    /// part.ids confirmed (via `message.part.updated`) to be `type:"reasoning"`,
    /// so `message.part.delta` (which carries only partID + field) routes right.
    pub reasoning_part_ids: &'a HashSet<String>,
    /// Reasoning part.ids already streamed live (`[[thinklive]]` pushed once).
    pub live_reasoned_ids: &'a mut HashSet<String>,
    /// Timestamp of the last live reasoning push (throttle).
    pub last_reasoning_push: &'a mut u64,
    /// Answer text parts accumulated live by part.id (`[[answerlive]]`).
    pub text_live: &'a mut HashMap<String, String>,
    /// Timestamp of the last live answer push (throttle).
    pub last_answer_push: &'a mut u64,
    /// Session currently shown in the UI (only its user messages are tracked
    /// for revert/fork — a user message of another session is ignored).
    pub current_session: Option<&'a str>,
    /// Set by `dispatch` when the server reports a session was compacted
    /// (`session.compacted`): the caller must re-load that session's history.
    /// The reload itself is HTTP IO and lives in the SSE loop — here we only
    /// record the intent, keeping "what the event means" in one place.
    pub compacted: &'a mut Option<String>,
    /// Compaction part.id already announced with a "Compacting…" notice, so a
    /// repeated `message.part.updated` for the same compaction does not stack
    /// duplicate bubbles. Reset by the history reload on `session.compacted`.
    pub compaction_seen: &'a mut Option<String>,
    /// Shared "compaction in progress" flag (polled by QML for the activity
    /// banner). Set when a compaction part shows up for the shown session
    /// (covers AUTO-compaction), cleared when the session is compacted or goes
    /// idle.
    pub compacting: &'a std::sync::Mutex<bool>,
    /// Set when the worker has just sent a prompt and is waiting for the
    /// user-message id from `message.updated`; cleared once captured (see
    /// `on_message_updated`). Prevents attaching a replayed id to the wrong
    /// bubble.
    pub expect_user_id: &'a mut bool,
}

/// Applies an event to the state. Returns bool — "state changed" (redraw the UI).
///
/// This is the single hub for "what a server event does": every branch either
/// mutates state or delegates to a handler that does. Branches listed after the
/// explicit ones are not implemented in the UI yet — they warn (see
/// `note_unimplemented`). Coverage of the server's event union is still
/// incomplete; new handling is added *here* first.
pub async fn dispatch(event: &Event, state: &mut DispatcherState, stream: &mut StreamState<'_>) -> bool {
    match event {
        Event::SessionStatus { properties } => {
            let id = properties.sessionID.clone();
            let fsm = state.sessions.entry(id.clone()).or_insert_with(SessionFsm::default);
            fsm.apply(properties.status.clone());
            let _ = id;
            true
        }
        Event::SessionIdle { properties } => {
            if let Some(fsm) = state.sessions.get_mut(&properties.sessionID) {
                fsm.apply(crate::types::session::SessionStatus::Idle);
            }
            // Safety: a session that went idle is no longer compacting (covers
            // an aborted/failed compaction that never emitted `session.compacted`).
            if stream.current_session == Some(properties.sessionID.as_str()) {
                if let Ok(mut g) = stream.compacting.lock() {
                    *g = false;
                }
            }
            true
        }
        Event::PermissionV2Asked { properties } => {
            let perm = Permission {
                id: properties.id.clone(),
                sessionID: properties.sessionID.clone(),
                action: properties.action.clone(),
                resources: properties.resources.clone(),
                save: properties.save.clone(),
                metadata: properties.metadata.clone(),
                source: properties.source.clone(),
            };
            state.permissions.push(perm);
            true
        }
        Event::PermissionAsked { properties } => {
            // Legacy permission.asked → the same Permission model.
            let perm = Permission {
                id: properties.id.clone(),
                sessionID: properties.sessionID.clone(),
                action: properties.permission.clone(),
                resources: properties.patterns.clone(),
                save: Some(properties.always.clone()),
                metadata: Some(properties.metadata.clone()),
                source: properties.tool.as_ref().map(|t| crate::types::event::PermissionV2Source {
                    kind: "tool".to_string(),
                    messageID: t.messageID.clone(),
                    callID: t.callID.clone(),
                }),
            };
            state.permissions.push(perm);
            true
        }
        Event::PermissionV2Replied { properties: p } => {
            state.permissions.pop(&p.requestID);
            true
        }
        Event::PermissionReplied { properties: p } => {
            state.permissions.pop(&p.requestID);
            true
        }
        Event::MessagePartUpdated { properties } => {
            if let crate::types::message::Part::Tool { callID, tool, state: ts, .. } = &properties.part
            {
                state.tools.upsert(callID.clone(), tool.clone(), ts.clone());
                // Emit an inline tool row: one feed line per callID, re-sent on
                // every state change so QML can replace it in place (TUI-style
                // inline tool calls in the transcript).
                if let Ok(mut q) = stream.pending.lock() {
                    q.push(proto_tool_line(
                        now_ms(),
                        callID,
                        tool,
                        ts.ui_status(),
                        &ts.description(),
                    ));
                }
            }
            // Compaction started: the server attaches a `type:"compaction"` part
            // to a user message, then streams the summary as an assistant
            // `summary:true` message, and finally reports `session.compacted`.
            // Announce it ONCE per part so the chat shows that work is happening
            // (the TUI renders a dedicated compaction row here). Driving this off
            // the event — not the Compact button — means AUTO-compaction on
            // context overflow is visible too. The `session.compacted` reload
            // replaces the notice with the summary + marker.
            if let crate::types::message::Part::Compaction { id, .. } = &properties.part {
                if stream.current_session == Some(properties.sessionID.as_str()) {
                    if let Ok(mut g) = stream.compacting.lock() {
                        *g = true;
                    }
                    if stream.compaction_seen.as_deref() != Some(id.as_str()) {
                        *stream.compaction_seen = Some(id.clone());
                        if let Ok(mut q) = stream.pending.lock() {
                            q.push(svc_line(now_ms(), crate::i18n::K_COMPACTING, &[]));
                        }
                    }
                }
            }
            true
        }
        Event::QuestionAsked { properties } => {
            state.questions.push(QuestionRequest {
                id: properties.id.clone(),
                sessionID: properties.sessionID.clone(),
                questions: properties.questions.clone(),
                tool: properties.tool.clone(),
            });
            true
        }
        Event::QuestionV2Asked { properties } => {
            state.questions.push(QuestionRequest {
                id: properties.id.clone(),
                sessionID: properties.sessionID.clone(),
                questions: properties.questions.clone(),
                tool: properties.tool.clone(),
            });
            true
        }
        Event::QuestionReplied { properties: p } => {
            state.questions.pop(&p.requestID);
            true
        }
        Event::QuestionV2Replied { properties: p } => {
            state.questions.pop(&p.requestID);
            true
        }
        Event::QuestionRejected { properties: p } => {
            state.questions.pop(&p.requestID);
            true
        }
        Event::QuestionV2Rejected { properties: p } => {
            state.questions.pop(&p.requestID);
            true
        }
        Event::SessionCreated { properties } | Event::SessionUpdated { properties } => {
            state.session_meta.insert(properties.info.id.clone(), properties.info.clone());
            true
        }
        Event::SessionDeleted { properties } => {
            state.session_meta.remove(&properties.info.id);
            state.sessions.remove(&properties.info.id);
            true
        }
        // Compaction finished: the transcript changed (old turns replaced by a
        // summary). The caller re-loads the history — needs HTTP IO, so we only
        // record which session here (see `StreamState::compacted`).
        Event::SessionCompacted { properties } => {
            *stream.compacted = Some(properties.sessionID.clone());
            if stream.current_session == Some(properties.sessionID.as_str()) {
                if let Ok(mut g) = stream.compacting.lock() {
                    *g = false;
                }
            }
            true
        }
        // Streaming / error events. Their handling used to live inline in the
        // SSE loop (main.rs), which smeared the "event → what happens"
        // responsibility across files. The roots grow here now; the handlers
        // (`on_message_updated`/`on_message_part_delta`/`on_session_error`)
        // mutate the accumulators carried in `StreamState`.
        Event::MessageUpdated { properties } => {
            on_message_updated(stream, properties);
            true
        }
        Event::MessagePartDelta { properties } => {
            on_message_part_delta(stream, properties);
            true
        }
        Event::SessionError { properties } => {
            on_session_error(stream, properties);
            true
        }
        // Tool / shell activity of the agent (`session.next.tool.*` and
        // `session.next.shell.*`). These feed `state.tools`, which the UI polls
        // as a compact activity strip so bash / read / edit / webfetch are
        // visible WHILE the agent works instead of the chat going silent.
        Event::SessionNextToolInputStarted { properties } => {
            on_tool_input_started(state, properties);
            true
        }
        Event::SessionNextToolInputDelta { properties } => {
            on_tool_input_delta(state, properties);
            true
        }
        Event::SessionNextToolInputEnded { properties } => {
            on_tool_input_ended(state, properties);
            true
        }
        Event::SessionNextToolCalled { properties } => {
            on_tool_called(state, properties);
            true
        }
        Event::SessionNextToolProgress { properties } => {
            on_tool_progress(state, properties);
            true
        }
        Event::SessionNextToolSuccess { properties } => {
            on_tool_success(state, properties);
            true
        }
        Event::SessionNextToolFailed { properties } => {
            on_tool_failed(state, properties);
            true
        }
        Event::SessionNextShellStarted { properties } => {
            on_shell_started(state, properties);
            true
        }
        Event::SessionNextShellEnded { properties } => {
            on_shell_ended(state, properties);
            true
        }
        // Everything else is not yet implemented in the UI. These branches are
        // listed exhaustively (no catch-all) so the compiler forces us to
        // revisit them when the event union grows. Each one is logged so the
        // logs make the degree of agreement between our UI and the native
        // server visible. See AGENTS.md ("Принципы").
        Event::ModelsDevRefreshed { .. }
        | Event::IntegrationUpdated { .. }
        | Event::IntegrationConnectionUpdated { .. }
        | Event::CatalogUpdated { .. }
        | Event::SessionDiff { .. }
        | Event::MessageRemoved { .. }
        | Event::MessagePartRemoved { .. }
        | Event::SessionNextAgentSwitched { .. }
        | Event::SessionNextModelSwitched { .. }
        | Event::SessionNextMoved { .. }
        | Event::SessionNextPrompted { .. }
        | Event::SessionNextPromptAdmitted { .. }
        | Event::SessionNextContextUpdated { .. }
        | Event::SessionNextSynthetic { .. }
        | Event::SessionNextStepStarted { .. }
        | Event::SessionNextStepEnded { .. }
        | Event::SessionNextStepFailed { .. }
        | Event::SessionNextTextStarted { .. }
        | Event::SessionNextTextDelta { .. }
        | Event::SessionNextTextEnded { .. }
        | Event::SessionNextReasoningStarted { .. }
        | Event::SessionNextReasoningDelta { .. }
        | Event::SessionNextReasoningEnded { .. }
        | Event::SessionNextRetried { .. }
        | Event::SessionNextCompactionStarted { .. }
        | Event::SessionNextCompactionDelta { .. }
        | Event::SessionNextCompactionEnded { .. }
        | Event::SessionNextRevertStaged { .. }
        | Event::SessionNextRevertCleared { .. }
        | Event::SessionNextRevertCommitted { .. }
        | Event::InstallationUpdated { .. }
        | Event::InstallationUpdateAvailable { .. }
        | Event::FileEdited { .. }
        | Event::ReferenceUpdated { .. }
        | Event::PluginAdded { .. }
        | Event::ProjectDirectoriesUpdated { .. }
        | Event::FileWatcherUpdated { .. }
        | Event::PtyCreated { .. }
        | Event::PtyUpdated { .. }
        | Event::PtyExited { .. }
        | Event::PtyDeleted { .. }
        | Event::TodoUpdated { .. }
        | Event::LspUpdated { .. }
        | Event::TuiPromptAppend { .. }
        | Event::TuiCommandExecute { .. }
        | Event::TuiToastShow { .. }
        | Event::TuiSessionSelect { .. }
        | Event::McpToolsChanged { .. }
        | Event::McpBrowserOpenFailed { .. }
        | Event::CommandExecuted { .. }
        | Event::ProjectUpdated { .. }
        | Event::VcsBranchUpdated { .. }
        | Event::WorkspaceReady { .. }
        | Event::WorkspaceFailed { .. }
        | Event::WorkspaceStatus { .. }
        | Event::WorktreeReady { .. }
        | Event::WorktreeFailed { .. }
        | Event::ServerConnected { .. } | Event::ServerHeartbeat { .. }
        | Event::GlobalDisposed { .. }
        | Event::ServerInstanceDisposed { .. }
        | Event::Sync { .. } => {
            note_unimplemented(event);
            false
        }
    }
}

// ── v2 event dispatch (`GET /api/event`) ─────────────────────────────────
//
// v2 events arrive as flat envelopes (`{id, type, created, data}`) on
// `/api/event`; their fields and identifiers differ from v1 (see
// `vault/Разведка-v2.md` §2–§4). The handlers below normalise them onto the
// SAME `DispatcherState`/`StreamState` accumulators the v1 path uses, so the
// main loop's rendering (live text/reasoning, idle flush, tools strip) is
// version-agnostic.
//
// Key mapping (v1 → v2):
//   * text/reasoning part id → `(assistantMessageID, ordinal)`;
//   * tool `callID` → `id`;
//   * `message.updated` (register assistant) → `session.step.started` /
//     `session.text.started` (`assistantMessageID`);
//   * `session.compacted` → `session.compaction.ended`.

/// Stable accumulator key for a v2 text/reasoning block: v1 keyed parts by a
/// server `partID`; v2 identifies a block by `(assistantMessageID, ordinal)`.
fn v2_part_key(assistant_message_id: &str, ordinal: i64) -> String {
    format!("{assistant_message_id}:{ordinal}")
}

/// Dispatch a single v2 envelope onto the shared state. Mirrors `dispatch`:
/// every known type is handled, the rest warn once via `note_unimplemented_v2`.
pub fn dispatch_v2(
    env: &EventEnvelope,
    state: &mut DispatcherState,
    stream: &mut StreamState<'_>,
) -> bool {
    use crate::types::v2::event as v2;
    use crate::types::v2::keys as k;
    // opencode 2.0.32 emits the plain `session.*` v2 event names, so the raw
    // envelope type is matched directly (no bridge prefix to strip).
    let kind = env.kind.as_str();
    match kind {
        // ── turn lifecycle ────────────────────────────────────────────
        k::STEP_STARTED => {
            match from_data::<v2::StepStartedData>(env) {
                Ok(d) => {
                    // A step opens the assistant message: register it so its
                    // text parts feed the answer (v1 did this at
                    // `message.updated`).
                    stream.assistant_messages.insert(d.assistant_message_id);
                    true
                }
                Err(e) => from_data_error(env, &e),
            }
        }
        k::STEP_ENDED | k::STEP_STREAMED => {
            // Terminal step info (cost/tokens/finish) is not rendered yet; the
            // turn end is driven by `session.idle`/`session.status`. Nothing to
            // record here — but the envelopes must still be recognised.
            false
        }
        k::STEP_FAILED => match from_data::<v2::StepEndedData>(env) {
            Ok(d) => {
                on_session_error_v2(stream, d.error.as_ref());
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── answer text ───────────────────────────────────────────────
        k::TEXT_STARTED => match from_data::<v2::TextStartedData>(env) {
            Ok(d) => {
                stream.assistant_messages.insert(d.assistant_message_id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TEXT_DELTA => match from_data::<v2::TextDeltaData>(env) {
            Ok(d) => {
                on_v2_text_delta(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TEXT_ENDED => match from_data::<v2::TextEndedData>(env) {
            Ok(d) => {
                on_v2_text_ended(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── reasoning ─────────────────────────────────────────────────
        k::REASONING_STARTED => match from_data::<v2::TextStartedData>(env) {
            Ok(_) => true,
            Err(e) => from_data_error(env, &e),
        },
        k::REASONING_DELTA => match from_data::<v2::TextDeltaData>(env) {
            Ok(d) => {
                on_v2_reasoning_delta(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::REASONING_ENDED => match from_data::<v2::TextEndedData>(env) {
            Ok(d) => {
                on_v2_reasoning_ended(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── tool calls ────────────────────────────────────────────────
        k::TOOL_INPUT_STARTED => match from_data::<v2::ToolInputStartedData>(env) {
            Ok(d) => {
                on_tool_input_started_v2(state, &d.id, &d.name);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_INPUT_DELTA => match from_data::<v2::ToolInputDeltaData>(env) {
            Ok(d) => {
                on_tool_input_delta_v2(state, &d.id, &d.delta);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_INPUT_ENDED => match from_data::<v2::ToolInputEndedData>(env) {
            Ok(d) => {
                on_tool_input_ended_v2(state, &d.id, &d.text);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_CALLED => match from_data::<v2::ToolCalledData>(env) {
            Ok(d) => {
                on_tool_called_v2(state, &d.id, &env.data);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_PROGRESS => match from_data::<v2::ToolProgressData>(env) {
            Ok(d) => {
                on_tool_progress_v2(state, &d.id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_SUCCESS => match from_data::<v2::ToolSuccessData>(env) {
            Ok(d) => {
                on_tool_success_v2(state, &d.id, &env.data);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_FAILED => match from_data::<v2::ToolFailedData>(env) {
            Ok(d) => {
                on_tool_failed_v2(state, &d.id, &env.data);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── compaction ────────────────────────────────────────────────
        k::COMPACTION_STARTED => match from_data::<v2::CompactionStartedData>(env) {
            Ok(d) => {
                on_compaction_started_v2(stream, &d.session_id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::COMPACTION_DELTA => {
            // The growing compaction summary is not rendered live yet; the
            // final text arrives with `compaction.ended`, which reloads history.
            false
        }
        k::COMPACTION_ENDED => match from_data::<v2::CompactionEndedData>(env) {
            Ok(d) => {
                on_compaction_ended_v2(stream, &d.session_id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::COMPACTION_FAILED => match from_data::<v2::CompactionFailedData>(env) {
            Ok(d) => {
                if stream.current_session == Some(d.session_id.as_str()) {
                    if let Ok(mut g) = stream.compacting.lock() {
                        *g = false;
                    }
                }
                on_session_error_v2(stream, Some(&d.error));
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── session lifecycle ─────────────────────────────────────────
        k::RENAMED | k::MOVED | k::AGENT_SELECTED | k::MODEL_SELECTED => {
            // Session metadata changes are re-read from the sessions list on the
            // next poll; no live-accumulator effect.
            true
        }
        // ── notifications we recognise but do not render (yet) ────────
        // The UI gets nothing from them yet (turn end is
        // `session.idle`/`session.status`, prompt admission is driven by our
        // own send path). Recognised so they are not reported as unknown.
        k::SYNTHETIC | k::REVERT_STAGED | k::REVERT_CLEARED | k::REVERT_COMMITTED
        | k::SHELL_STARTED | k::SHELL_ENDED | k::MESSAGE_CONTENT_UPDATED
        | k::USAGE_RECORDED | k::USAGE_UPDATED | k::EXECUTION_STARTED | k::EXECUTION_FAILED
        | k::EXECUTION_INTERRUPTED | k::EXECUTION_SUCCEEDED => true,
        // ── catalog/config invalidation (2.0.32): re-read lazily ──────
        k::PROJECT_UPDATED | k::INTEGRATION_UPDATED | k::PROVIDER_UPDATED | k::MODEL_UPDATED
        | k::AGENT_UPDATED | k::COMMAND_UPDATED | k::SKILL_UPDATED | k::WEBSEARCH_UPDATED
        | k::REFERENCE_UPDATED | k::PLUGIN_UPDATED => true,
        k::DELETED => {
            if let Some(sid) = env.data.get(k::SESSION_ID).and_then(|v| v.as_str()) {
                state.sessions.remove(sid);
                state.session_meta.remove(sid);
            }
            true
        }
        // ── status / idle ─────────────────────────────────────────────
        k::IDLE => match from_data::<v2::SessionIdleData>(env) {
            Ok(d) => {
                if let Some(fsm) = state.sessions.get_mut(&d.sessionID) {
                    fsm.apply(crate::types::session::SessionStatus::Idle);
                }
                if stream.current_session == Some(d.sessionID.as_str()) {
                    if let Ok(mut g) = stream.compacting.lock() {
                        *g = false;
                    }
                }
                // The main loop flushes the answer on `session.idle` via
                // `idle_session_id_v2`.
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::STATUS => match from_data::<v2::SessionStatusData>(env) {
            Ok(d) => {
                let fsm = state
                    .sessions
                    .entry(d.sessionID.clone())
                    .or_insert_with(SessionFsm::default);
                match d.status {
                    v2::SessionStatusValue::Idle => {
                        fsm.apply(crate::types::session::SessionStatus::Idle)
                    }
                    v2::SessionStatusValue::Busy => {
                        fsm.apply(crate::types::session::SessionStatus::Busy)
                    }
                    v2::SessionStatusValue::Retry { attempt, message, next } => fsm.apply(
                        crate::types::session::SessionStatus::Retry { attempt, message, next },
                    ),
                    v2::SessionStatusValue::Unknown => {}
                }
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── permissions ───────────────────────────────────────────────
        k::PERMISSION_ASKED => match from_data::<v2::PermissionAskedData>(env) {
            Ok(d) => {
                state.permissions.push(Permission {
                    id: d.id,
                    sessionID: d.session_id,
                    action: d.action,
                    resources: d.resources,
                    save: d.save,
                    metadata: d.metadata,
                    source: d.source.map(|s| match s {
                        crate::types::v2::permission::PermissionSource::Tool {
                            message_id,
                            id,
                        } => crate::types::event::PermissionV2Source {
                            kind: k::SOURCE_TOOL.to_string(),
                            messageID: message_id,
                            callID: id,
                        },
                        crate::types::v2::permission::PermissionSource::Unknown => {
                            crate::types::event::PermissionV2Source {
                                kind: k::SOURCE_UNKNOWN.to_string(),
                                messageID: String::new(),
                                callID: String::new(),
                            }
                        }
                    }),
                });
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::PERMISSION_REPLIED => match from_data::<v2::PermissionRepliedData>(env) {
            Ok(d) => {
                state.permissions.pop(&d.requestID);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── questions (legacy, still emitted) ─────────────────────────
        k::QUESTION_ASKED => match from_data::<v2::QuestionAskedData>(env) {
            Ok(d) => {
                state.questions.push(QuestionRequest {
                    id: d.id,
                    sessionID: d.session_id,
                    questions: d.questions,
                    tool: d.tool,
                });
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::QUESTION_REPLIED | k::QUESTION_REJECTED => {
            match from_data::<v2::QuestionResolvedData>(env) {
                Ok(d) => {
                    state.questions.pop(&d.requestID);
                    true
                }
                Err(e) => from_data_error(env, &e),
            }
        }
        // ── forms (replace questions) ─────────────────────────────────
        k::FORM_CREATED => {
            // The form panel is step 6 work; recognised here so the envelope is
            // not reported as an unknown type.
            false
        }
        k::FORM_REPLIED | k::FORM_CANCELLED => false,
        // ── notifications we do not render ────────────────────────────
        k::SERVER_CONNECTED => false,
        // Everything else is a v2 event with no UI handling yet. Listed via the
        // fallback so a NEW upstream event is surfaced once (never dropped).
        _ => {
            note_unimplemented_v2(&env.kind);
            false
        }
    }
}

/// Deserialise an envelope's `data` into a typed v2 body. A malformed body is
/// reported like a parse error, not silently ignored.
fn from_data<T: serde::de::DeserializeOwned>(env: &EventEnvelope) -> Result<T, String> {
    serde_json::from_value(env.data.clone()).map_err(|e| format!("data: {e}"))
}

/// Log a malformed v2 body once per event type and keep going.
fn from_data_error(env: &EventEnvelope, err: &str) -> bool {
    log::warn!("v2 event '{}' has an unparseable body ({err})", env.kind);
    false
}

/// v2 answer-delta: append to the live answer accumulator keyed by
/// `(assistantMessageID, ordinal)` and (throttled) mirror it to QML.
fn on_v2_text_delta(stream: &mut StreamState<'_>, d: &crate::types::v2::event::TextDeltaData) {
    if !stream.assistant_messages.contains(&d.assistant_message_id) {
        return;
    }
    let key = v2_part_key(&d.assistant_message_id, d.ordinal);
    let entry = stream.text_live.entry(key.clone()).or_default();
    entry.push_str(&d.delta);
    if entry.trim().is_empty() {
        return;
    }
    let now = now_ms();
    if now.saturating_sub(*stream.last_answer_push) < REASONING_LIVE_MS {
        return;
    }
    *stream.last_answer_push = now;
    if let Ok(mut q) = stream.pending.lock() {
        q.push(proto_live_line(now, ANSWER_LIVE_TAG, entry.trim_end(), &key));
    }
}

/// v2 answer-end: the block's exact final text replaces the live-grown one in
/// the accumulator. The QML-facing final line is emitted by the worker loop
/// (`TurnState::on_v2_event`), which owns the per-turn text bookkeeping.
fn on_v2_text_ended(stream: &mut StreamState<'_>, d: &crate::types::v2::event::TextEndedData) {
    if !stream.assistant_messages.contains(&d.assistant_message_id) {
        return;
    }
    let key = v2_part_key(&d.assistant_message_id, d.ordinal);
    if stream.text_live.contains_key(&key) {
        stream.text_live.insert(key, d.text.clone());
    }
}

/// v2 reasoning-delta: same as the answer path but into the think accumulator.
fn on_v2_reasoning_delta(
    stream: &mut StreamState<'_>,
    d: &crate::types::v2::event::TextDeltaData,
) {
    let key = v2_part_key(&d.assistant_message_id, d.ordinal);
    let entry = stream.reasonings.entry(key.clone()).or_default();
    entry.push_str(&d.delta);
    if entry.trim().is_empty() {
        return;
    }
    let now = now_ms();
    if now.saturating_sub(*stream.last_reasoning_push) < REASONING_LIVE_MS {
        return;
    }
    *stream.last_reasoning_push = now;
    stream.live_reasoned_ids.insert(key.clone());
    if let Ok(mut q) = stream.pending.lock() {
        q.push(proto_live_line(now, THINK_LIVE_TAG, entry.trim_end(), &key));
    }
}

/// v2 reasoning-end: finalise the block in the accumulator (the worker loop
/// emits the QML-facing final line via `TurnState::on_v2_event`).
fn on_v2_reasoning_ended(
    stream: &mut StreamState<'_>,
    d: &crate::types::v2::event::TextEndedData,
) {
    let key = v2_part_key(&d.assistant_message_id, d.ordinal);
    stream.reasonings.insert(key, d.text.clone());
}

/// v2 `session.compaction.started` — announce once for the shown session and
/// raise the "compacting" banner (mirrors the v1 `message.part.updated`
/// compaction part handling).
fn on_compaction_started_v2(stream: &mut StreamState<'_>, session_id: &str) {
    if stream.current_session != Some(session_id) {
        return;
    }
    if let Ok(mut g) = stream.compacting.lock() {
        *g = true;
    }
    if stream.compaction_seen.as_deref() != Some(session_id) {
        *stream.compaction_seen = Some(session_id.to_string());
        if let Ok(mut q) = stream.pending.lock() {
            q.push(svc_line(now_ms(), crate::i18n::K_COMPACTING, &[]));
        }
    }
}

/// v2 `session.compaction.ended` — the transcript changed: record the session
/// for the caller to reload (mirrors v1 `session.compacted`).
fn on_compaction_ended_v2(stream: &mut StreamState<'_>, session_id: &str) {
    *stream.compacted = Some(session_id.to_string());
    if stream.current_session == Some(session_id) {
        if let Ok(mut g) = stream.compacting.lock() {
            *g = false;
        }
    }
}

/// v2 `session.error`-equivalent: surface a structured error as a feed line.
fn on_session_error_v2(stream: &mut StreamState<'_>, error: Option<&serde_json::Value>) {
    let msg = human_session_error_value(error);
    if msg.is_empty() {
        return;
    }
    log::error!("v2 session error: {msg}");
    if let Ok(mut q) = stream.pending.lock() {
        q.push(err_line(now_ms(), crate::i18n::K_SERVER_ERROR, &[msg]));
    }
    if let Ok(mut s) = stream.status.lock() {
        *s = "error".to_string();
    }
}

/// Human-readable text from a v2 structured error (`{type, message, status?}`).
fn human_session_error_value(error: Option<&serde_json::Value>) -> String {
    use crate::types::v2::keys as k;
    let Some(e) = error else {
        return String::new();
    };
    if let Some(m) = e.get(k::MESSAGE).and_then(|v| v.as_str()) {
        if !m.is_empty() {
            return m.to_string();
        }
    }
    String::new()
}

// v2 tool handlers: same ToolStore transitions as the v1 `on_tool_*`, but the
// v2 `data` uses `id` (not `callID`) and carries the v1 field-name-independent
// `input`/`content`/`error` shapes.

fn on_tool_input_started_v2(state: &mut DispatcherState, id: &str, name: &str) {
    state.tools.upsert(
        id.to_string(),
        name.to_string(),
        ToolState::Pending {
            input: serde_json::Value::Null,
        },
    );
}

fn on_tool_input_delta_v2(state: &mut DispatcherState, id: &str, delta: &str) {
    if state.tools.name_of(id).is_none() {
        state.tools.ensure(id, "");
    }
    let prev = state.tools.input_of(id);
    let mut s = prev.as_str().unwrap_or("").to_string();
    s.push_str(delta);
    state.tools.set_state(
        id,
        ToolState::Pending {
            input: serde_json::Value::String(s),
        },
    );
}

fn on_tool_input_ended_v2(state: &mut DispatcherState, id: &str, text: &str) {
    if state.tools.name_of(id).is_none() {
        state.tools.ensure(id, "");
    }
    let input = serde_json::from_str(text)
        .unwrap_or_else(|_| serde_json::Value::String(text.to_string()));
    state.tools.set_state(id, ToolState::Pending { input });
}

fn on_tool_called_v2(state: &mut DispatcherState, id: &str, data: &serde_json::Value) {
    use crate::types::v2::keys as k;
    let name = state.tools.name_of(id).unwrap_or("").to_string();
    let input = data
        .get(k::INPUT)
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    // `name` is not in `tool.called`; keep the one learned at input.started.
    if name.is_empty() {
        state.tools.ensure(id, "");
    }
    state.tools.set_state(id, ToolState::Running { input });
}

fn on_tool_progress_v2(state: &mut DispatcherState, id: &str) {
    if state.tools.name_of(id).is_none() {
        return;
    }
    let input = state.tools.input_of(id);
    state.tools.set_state(id, ToolState::Running { input });
}

fn on_tool_success_v2(state: &mut DispatcherState, id: &str, data: &serde_json::Value) {
    use crate::types::v2::keys as k;
    let input = state.tools.input_of(id);
    let title = short_input(&input)
        .or_else(|| state.tools.name_of(id).map(str::to_string))
        .unwrap_or_else(|| k::DEFAULT_TOOL_TITLE.to_string());
    let output = data
        .get(k::CONTENT)
        .map(content_to_string)
        .unwrap_or_default();
    state.tools.set_state(
        id,
        ToolState::Completed {
            input,
            output,
            title,
        },
    );
}

fn on_tool_failed_v2(state: &mut DispatcherState, id: &str, data: &serde_json::Value) {
    use crate::types::v2::keys as k;
    let input = state.tools.input_of(id);
    let error = data
        .get(k::ERROR)
        .map(content_to_string)
        .unwrap_or_default();
    state.tools.set_state(id, ToolState::Error { input, error });
}

/// Logs (once per event type) that a v2 event is not implemented in the UI.
fn note_unimplemented_v2(kind: &str) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let first_time = seen
        .lock()
        .map(|mut set| set.insert(kind.to_string()))
        .unwrap_or(true);
    if first_time {
        log::warn!("v2 event '{kind}' is not implemented in the UI yet — dispatch ignores it");
    } else {
        log::debug!("v2 event '{kind}' (not implemented) ignored again");
    }
}

/// `message.updated` — register assistant message ids, so that only the text
/// parts of assistant messages feed the answer (user/echo parts are ignored).
/// A user message of the *shown* session, arriving right after we sent a prompt,
/// yields its `messageID` to QML (control line `[[msgid]]<id>`), so the freshly
/// created user bubble can offer revert/fork like the ones loaded from history.
fn on_message_updated(stream: &mut StreamState<'_>, props: &MessageUpdatedProperties) {
    match &props.info {
        // Every assistant message feeds the answer accumulator — including the
        // `mode:"compaction"` summary of the compressed history, which thus
        // grows as an ordinary bubble while it is written (exactly what the TUI
        // does). `session.compacted` later reloads the history and replaces the
        // live bubble with the final marker + summary. The summary is recorded
        // in `compaction_messages` too, so the caller never reads it aloud.
        Message::Assistant { id, mode, .. } => {
            stream.assistant_messages.insert(id.clone());
            if mode == crate::types::keys::MODE_COMPACTION {
                stream.compaction_messages.insert(id.clone());
            }
        }
        Message::User { id, sessionID, .. } => {
            if *stream.expect_user_id && stream.current_session == Some(sessionID.as_str()) {
                *stream.expect_user_id = false;
                if let Ok(mut q) = stream.pending.lock() {
                    q.push(format!("{SET_MSG_ID_TAG}{id}"));
                }
            }
        }
    }
}

/// `message.part.delta` — append an on-the-fly text/reasoning chunk to its
/// accumulator and (throttled) stream it to QML so the bubble grows WHILE the
/// model works. `partID` is routed to the reasoning accumulator if the part was
/// confirmed `type:"reasoning"` via `message.part.updated`, otherwise to the
/// answer accumulator. Unknown-message and non-`text` deltas are ignored.
fn on_message_part_delta(stream: &mut StreamState<'_>, props: &MessagePartDeltaProperties) {
    if props.field != "text" || !stream.assistant_messages.contains(&props.messageID) {
        return;
    }
    let id = &props.partID;
    if stream.reasoning_part_ids.contains(id) {
        let entry = stream.reasonings.entry(id.clone()).or_default();
        entry.push_str(&props.delta);
        if entry.trim().is_empty() {
            return;
        }
        let now = now_ms();
        if now.saturating_sub(*stream.last_reasoning_push) < REASONING_LIVE_MS {
            return;
        }
        *stream.last_reasoning_push = now;
        stream.live_reasoned_ids.insert(id.clone());
        if let Ok(mut q) = stream.pending.lock() {
            q.push(proto_live_line(now, THINK_LIVE_TAG, entry.trim_end(), id));
        }
    } else {
        let entry = stream.text_live.entry(id.clone()).or_default();
        entry.push_str(&props.delta);
        if entry.trim().is_empty() {
            return;
        }
        let now = now_ms();
        if now.saturating_sub(*stream.last_answer_push) < REASONING_LIVE_MS {
            return;
        }
        *stream.last_answer_push = now;
        if let Ok(mut q) = stream.pending.lock() {
            q.push(proto_live_line(now, ANSWER_LIVE_TAG, entry.trim_end(), id));
        }
    }
}

/// `session.error` — classify the server error into a human-readable message
/// (`human_session_error`) and surface it as an error line in the chat feed,
/// switching the status to `error`. An event without an `error` field (e.g.
/// empty sessionID) is skipped.
fn on_session_error(stream: &mut StreamState<'_>, props: &SessionErrorProperties) {
    let msg = human_session_error(props.error.as_ref());
    if msg.is_empty() {
        log::debug!("session.error without error field, skipping");
        return;
    }
    log::error!("session.error: {msg}");
    if let Ok(mut q) = stream.pending.lock() {
        q.push(err_line(now_ms(), crate::i18n::K_SERVER_ERROR, &[msg]));
    }
    if let Ok(mut s) = stream.status.lock() {
        *s = "error".to_string();
    }
}

// ── Tool / shell activity ────────────────────────────────────────────────
//
// The agent's tool calls are tracked in `state.tools` (callID -> name+state).
// The UI polls a JSON snapshot of it every second and renders a compact strip,
// so bash / read / edit / webfetch stay visible WHILE the turn runs instead of
// the chat going silent.

/// `session.next.tool.input.started` — the model begins streaming a tool call;
/// we know the tool name, not yet its arguments.
fn on_tool_input_started(
    state: &mut DispatcherState,
    props: &SessionNextToolInputStartedProperties,
) {
    state.tools.upsert(
        props.callID.clone(),
        props.name.clone(),
        ToolState::Pending {
            input: serde_json::Value::Null,
        },
    );
}

/// `session.next.tool.input.delta` — a fragment of the tool-call arguments
/// (JSON being streamed). Accumulate it into the pending entry's description.
fn on_tool_input_delta(state: &mut DispatcherState, props: &SessionNextToolInputDeltaProperties) {
    let call = props.callID.as_str();
    if state.tools.name_of(call).is_none() {
        state.tools.ensure(call, "");
    }
    let prev = state.tools.input_of(call);
    let mut s = prev.as_str().unwrap_or("").to_string();
    s.push_str(&props.delta);
    state.tools.set_state(
        call,
        ToolState::Pending {
            input: serde_json::Value::String(s),
        },
    );
}

/// `session.next.tool.input.ended` — the arguments are complete as a raw JSON
/// string; parse them so the description can pick `command`/`filePath`/…
fn on_tool_input_ended(state: &mut DispatcherState, props: &SessionNextToolInputEndedProperties) {
    let call = props.callID.as_str();
    if state.tools.name_of(call).is_none() {
        state.tools.ensure(call, "");
    }
    let input = serde_json::from_str(&props.text)
        .unwrap_or_else(|_| serde_json::Value::String(props.text.clone()));
    state.tools.set_state(call, ToolState::Pending { input });
}

/// `session.next.tool.called` — the tool is invoked with parsed `input`; it is
/// now running.
fn on_tool_called(state: &mut DispatcherState, props: &SessionNextToolCalledProperties) {
    state.tools.upsert(
        props.callID.clone(),
        props.tool.clone(),
        ToolState::Running {
            input: props.input.clone(),
        },
    );
}

/// `session.next.tool.progress` — intermediate progress; keep it running.
fn on_tool_progress(state: &mut DispatcherState, props: &SessionNextToolProgressProperties) {
    let call = props.callID.as_str();
    if state.tools.name_of(call).is_none() {
        return;
    }
    let input = state.tools.input_of(call);
    state.tools.set_state(call, ToolState::Running { input });
}

/// `session.next.tool.success` — the tool finished; store a short title (from
/// the input) plus its output.
fn on_tool_success(state: &mut DispatcherState, props: &SessionNextToolSuccessProperties) {
    let call = props.callID.as_str();
    let input = state.tools.input_of(call);
    let title = short_input(&input)
        .or_else(|| state.tools.name_of(call).map(str::to_string))
        .unwrap_or_else(|| "tool".to_string());
    state.tools.set_state(
        call,
        ToolState::Completed {
            input,
            output: content_to_string(&props.content),
            title,
        },
    );
}

/// `session.next.tool.failed` — the tool errored.
fn on_tool_failed(state: &mut DispatcherState, props: &SessionNextToolFailedProperties) {
    let call = props.callID.as_str();
    let input = state.tools.input_of(call);
    state.tools.set_state(
        call,
        ToolState::Error {
            input,
            error: content_to_string(&props.error),
        },
    );
}

/// `session.next.shell.started` — the agent's persistent shell starts a command.
fn on_shell_started(state: &mut DispatcherState, props: &SessionNextShellStartedProperties) {
    state.tools.upsert(
        props.callID.clone(),
        SHELL_TOOL_NAME.to_string(),
        ToolState::Running {
            input: serde_json::json!({ crate::types::keys::COMMAND: props.command }),
        },
    );
}

/// `session.next.shell.ended` — the shell command finished.
fn on_shell_ended(state: &mut DispatcherState, props: &SessionNextShellEndedProperties) {
    let call = props.callID.as_str();
    let input = state.tools.input_of(call);
    let title = short_input(&input)
        .or_else(|| state.tools.name_of(call).map(str::to_string))
        .unwrap_or_else(|| SHELL_TOOL_NAME.to_string());
    state.tools.set_state(
        call,
        ToolState::Completed {
            input,
            output: props.output.clone(),
            title,
        },
    );
}

/// A short human label pulled from a tool-call input (command / path / …).
fn short_input(input: &serde_json::Value) -> Option<String> {
    use crate::types::keys as k;
    for key in [
        k::COMMAND,
        k::FILE_PATH,
        k::PATH,
        k::PATTERN,
        k::QUERY,
        k::URL,
        k::PROMPT,
        k::DESCRIPTION,
    ] {
        if let Some(s) = input.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Render a tool `content`/`error` value: plain string as-is, an array of
/// `{"text": …}` parts joined, anything else as compact JSON.
fn content_to_string(v: &serde_json::Value) -> String {
    use crate::types::keys as k;
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|it| {
                it.get(k::TEXT)
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| it.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

/// Logs (once per event type) that a dispatched event is not yet implemented in
/// the UI. Warning on the first occurrence keeps the "not implemented" surface
/// visible in the logs without flooding them on high-frequency events.
fn note_unimplemented(event: &Event) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static SEEN: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let name = event.type_name();
    let first_time = seen.lock().map(|mut set| set.insert(name)).unwrap_or(true);
    if first_time {
        log::warn!("event '{name}' is not implemented in the UI yet — dispatch branch ignores it");
    } else {
        log::debug!("event '{name}' (not implemented) ignored again");
    }
}

#[derive(Default)]
pub struct DispatcherState {
    pub sessions: std::collections::HashMap<String, SessionFsm>,
    pub session_meta: std::collections::HashMap<String, crate::types::session::Session>,
    pub tools: ToolStore,
    pub permissions: PermissionQueue,
    pub questions: QuestionQueue,
}

pub type SharedState = Arc<Mutex<DispatcherState>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Build a `StreamState` over fresh test accumulators, run `f`, and return
    /// the lines pushed to QML. `assistant` is the set of assistant message ids
    /// (a delta of an unknown message is ignored); `reasoning_ids` marks parts
    /// already confirmed as `type:"reasoning"`.
    fn pushed(
        assistant: &[&str],
        reasoning_ids: &[&str],
        props: &MessagePartDeltaProperties,
    ) -> Vec<String> {
        let pending = Mutex::new(Vec::new());
        let status = Mutex::new(String::new());
        let mut assistant_messages: HashSet<String> =
            assistant.iter().map(|s| s.to_string()).collect();
        let mut compaction_messages = HashSet::new();
        let mut reasonings = HashMap::new();
        let reasoning_part_ids: HashSet<String> =
            reasoning_ids.iter().map(|s| s.to_string()).collect();
        let mut live_reasoned_ids = HashSet::new();
        let mut last_reasoning_push = 0u64;
        let mut text_live = HashMap::new();
        let mut last_answer_push = 0u64;
        let mut expect_user_id = false;
        let mut compacted = None;
        let mut compaction_seen = None;
        let compacting = Mutex::new(false);
        {
            let mut stream = StreamState {
                pending: &pending,
                status: &status,
                assistant_messages: &mut assistant_messages,
                compaction_messages: &mut compaction_messages,
                reasonings: &mut reasonings,
                reasoning_part_ids: &reasoning_part_ids,
                live_reasoned_ids: &mut live_reasoned_ids,
                last_reasoning_push: &mut last_reasoning_push,
                text_live: &mut text_live,
                last_answer_push: &mut last_answer_push,
                current_session: None,
                compacted: &mut compacted,
                compaction_seen: &mut compaction_seen,
                compacting: &compacting,
                expect_user_id: &mut expect_user_id,
            };
            on_message_part_delta(&mut stream, props);
        }
        pending.lock().map(|q| q.clone()).unwrap_or_default()
    }

    fn delta(message_id: &str, part_id: &str, text: &str) -> MessagePartDeltaProperties {
        MessagePartDeltaProperties {
            sessionID: "ses".to_string(),
            messageID: message_id.to_string(),
            partID: part_id.to_string(),
            field: "text".to_string(),
            delta: text.to_string(),
        }
    }

    /// The live answer line MUST carry the part-id so QML can edit the exact
    /// bubble in place, position-independently (the duplicate-bubble fix).
    #[test]
    fn answer_delta_line_carries_part_id() {
        let out = pushed(&["msg1"], &[], &delta("msg1", "prt_1", "hello"));
        assert_eq!(out.len(), 1);
        assert!(out[0].ends_with("[[answerlive]]hello[[p:prt_1]]"), "{}", out[0]);
    }

    /// Reasoning deltas route to the think-live tag, same part-id shape.
    #[test]
    fn reasoning_delta_line_carries_part_id() {
        let out = pushed(&["msg1"], &["prt_r"], &delta("msg1", "prt_r", "why"));
        assert_eq!(out.len(), 1);
        assert!(out[0].ends_with("[[thinklive]]why[[p:prt_r]]"), "{}", out[0]);
    }

    /// A delta of a non-assistant message is ignored (no line pushed).
    #[test]
    fn unknown_message_delta_is_ignored() {
        let out = pushed(&[], &[], &delta("msgX", "prt_1", "hello"));
        assert!(out.is_empty());
    }

    /// `session.compacted` records the session to reload, so the SSE loop can
    /// rebuild the transcript (summary replaces the old turns) without the user
    /// leaving and re-entering the chat.
    #[tokio::test]
    async fn session_compacted_records_the_session_to_reload() {
        let pending = Mutex::new(Vec::new());
        let status = Mutex::new(String::new());
        let mut assistant_messages = HashSet::new();
        let mut compaction_messages = HashSet::new();
        let mut reasonings = HashMap::new();
        let reasoning_part_ids = HashSet::new();
        let mut live_reasoned_ids = HashSet::new();
        let mut last_reasoning_push = 0u64;
        let mut text_live = HashMap::new();
        let mut last_answer_push = 0u64;
        let mut expect_user_id = false;
        let mut compacted = None;
        let mut compaction_seen = None;
        let compacting = Mutex::new(false);
        {
            let mut stream = StreamState {
                pending: &pending,
                status: &status,
                assistant_messages: &mut assistant_messages,
                compaction_messages: &mut compaction_messages,
                reasonings: &mut reasonings,
                reasoning_part_ids: &reasoning_part_ids,
                live_reasoned_ids: &mut live_reasoned_ids,
                last_reasoning_push: &mut last_reasoning_push,
                text_live: &mut text_live,
                last_answer_push: &mut last_answer_push,
                current_session: None,
                compacted: &mut compacted,
                compaction_seen: &mut compaction_seen,
                compacting: &compacting,
                expect_user_id: &mut expect_user_id,
            };
            let event = Event::SessionCompacted {
                properties: crate::types::event::SessionIdProperties {
                    sessionID: "ses_compact".to_string(),
                },
            };
            let mut state = DispatcherState::default();
            assert!(dispatch(&event, &mut state, &mut stream).await);
        }
        assert_eq!(compacted.as_deref(), Some("ses_compact"));
    }

    /// A `type:"compaction"` part of the SHOWN session pushes a single
    /// "Compacting…" notice; a repeat of the same part.id does not stack a
    /// second bubble, and another session's compaction is ignored. This is what
    /// makes AUTO-compaction (context overflow) visible, not just the button.
    #[tokio::test]
    async fn compaction_part_announces_once_for_the_shown_session() {
        use crate::types::event::MessagePartUpdatedProperties;
        use crate::types::message::Part;

        let pending = Mutex::new(Vec::new());
        let status = Mutex::new(String::new());
        let mut assistant_messages = HashSet::new();
        let mut compaction_messages = HashSet::new();
        let mut reasonings = HashMap::new();
        let reasoning_part_ids = HashSet::new();
        let mut live_reasoned_ids = HashSet::new();
        let mut last_reasoning_push = 0u64;
        let mut text_live = HashMap::new();
        let mut last_answer_push = 0u64;
        let mut compacted = None;
        let mut compaction_seen = None;
        let mut expect_user_id = false;
        let compacting = Mutex::new(false);
        {
            let mut stream = StreamState {
                pending: &pending,
                status: &status,
                assistant_messages: &mut assistant_messages,
                compaction_messages: &mut compaction_messages,
                reasonings: &mut reasonings,
                reasoning_part_ids: &reasoning_part_ids,
                live_reasoned_ids: &mut live_reasoned_ids,
                last_reasoning_push: &mut last_reasoning_push,
                text_live: &mut text_live,
                last_answer_push: &mut last_answer_push,
                current_session: Some("ses1"),
                compacted: &mut compacted,
                compaction_seen: &mut compaction_seen,
                compacting: &compacting,
                expect_user_id: &mut expect_user_id,
            };
            let mut state = DispatcherState::default();
            let part = |session: &str| Event::MessagePartUpdated {
                properties: MessagePartUpdatedProperties {
                    sessionID: session.to_string(),
                    part: Part::Compaction {
                        id: "prt_c1".to_string(),
                        auto: true,
                    },
                    time: 0.0,
                },
            };
            // Another session's compaction: no notice, flag stays down.
            assert!(dispatch(&part("other"), &mut state, &mut stream).await);
            assert!(!*compacting.lock().unwrap());
            // Shown session, twice with the same part id: ONE notice.
            assert!(dispatch(&part("ses1"), &mut state, &mut stream).await);
            assert!(dispatch(&part("ses1"), &mut state, &mut stream).await);
            // The banner flag is up while the compaction runs.
            assert!(*compacting.lock().unwrap());
            // `session.compacted` clears it (the reload replaces the feed).
            let done = Event::SessionCompacted {
                properties: crate::types::event::SessionIdProperties {
                    sessionID: "ses1".to_string(),
                },
            };
            assert!(dispatch(&done, &mut state, &mut stream).await);
            assert!(!*compacting.lock().unwrap());
        }
        let q = pending.lock().unwrap();
        assert_eq!(q.len(), 1, "expected exactly one notice, got {q:?}");
        assert!(q[0].contains("[[s:Compacting history"), "{}", q[0]);
    }

    /// A `mode:"compaction"` assistant message is registered in BOTH sets:
    /// `assistant_messages` (its text streams into the transcript like any
    /// answer — the summary grows live, mirroring the TUI) and
    /// `compaction_messages` (so the caller never reads it aloud). A normal
    /// assistant message lands only in the former.
    #[test]
    fn compaction_message_streams_and_is_flagged() {
        use crate::types::message::{AssistantPath, CacheUsage, MessageTime, MessageTokens};
        let props = |id: &str, mode: &str| -> MessageUpdatedProperties {
            MessageUpdatedProperties {
                sessionID: "ses".to_string(),
                info: Message::Assistant {
                    id: id.to_string(),
                    sessionID: "ses".to_string(),
                    time: MessageTime {
                        created: 1,
                        completed: None,
                    },
                    error: None,
                    parentID: "msg_u".to_string(),
                    modelID: "m".to_string(),
                    providerID: "p".to_string(),
                    mode: mode.to_string(),
                    path: AssistantPath {
                        cwd: "/x".to_string(),
                        root: "/x".to_string(),
                    },
                    cost: 0.0,
                    tokens: MessageTokens {
                        input: 0,
                        output: 0,
                        reasoning: 0,
                        cache: CacheUsage { read: 0, write: 0 },
                    },
                    finish: None,
                },
            }
        };
        let pending = Mutex::new(Vec::new());
        let status = Mutex::new(String::new());
        let mut assistant_messages = HashSet::new();
        let mut compaction_messages = HashSet::new();
        let mut reasonings = HashMap::new();
        let reasoning_part_ids = HashSet::new();
        let mut live_reasoned_ids = HashSet::new();
        let mut last_reasoning_push = 0u64;
        let mut text_live = HashMap::new();
        let mut last_answer_push = 0u64;
        let mut expect_user_id = false;
        let mut compacted = None;
        let mut compaction_seen = None;
        let compacting = Mutex::new(false);
        {
            let mut stream = StreamState {
                pending: &pending,
                status: &status,
                assistant_messages: &mut assistant_messages,
                compaction_messages: &mut compaction_messages,
                reasonings: &mut reasonings,
                reasoning_part_ids: &reasoning_part_ids,
                live_reasoned_ids: &mut live_reasoned_ids,
                last_reasoning_push: &mut last_reasoning_push,
                text_live: &mut text_live,
                last_answer_push: &mut last_answer_push,
                current_session: None,
                compacted: &mut compacted,
                compaction_seen: &mut compaction_seen,
                compacting: &compacting,
                expect_user_id: &mut expect_user_id,
            };
            on_message_updated(&mut stream, &props("msg_n", "build"));
            on_message_updated(&mut stream, &props("msg_c", crate::types::keys::MODE_COMPACTION));
        }
        assert!(assistant_messages.contains("msg_n"));
        assert!(assistant_messages.contains("msg_c"));
        assert!(!compaction_messages.contains("msg_n"));
        assert!(compaction_messages.contains("msg_c"));
    }

    #[test]
    fn v2_event_constants_are_plain_session_names() {
        // opencode 2.0.32 emits plain `session.*` event names (no `session.next.`
        // bridge prefix). Guard the dispatch constants against a stray prefix or
        // typo: each must be exactly the unprefixed wire spelling.
        use crate::types::v2::keys as k;
        for (const_name, value) in [
            ("STEP_STARTED", k::STEP_STARTED),
            ("STEP_ENDED", k::STEP_ENDED),
            ("STEP_STREAMED", k::STEP_STREAMED),
            ("STEP_FAILED", k::STEP_FAILED),
            ("TEXT_STARTED", k::TEXT_STARTED),
            ("TEXT_DELTA", k::TEXT_DELTA),
            ("TEXT_ENDED", k::TEXT_ENDED),
            ("REASONING_STARTED", k::REASONING_STARTED),
            ("REASONING_DELTA", k::REASONING_DELTA),
            ("REASONING_ENDED", k::REASONING_ENDED),
            ("TOOL_INPUT_STARTED", k::TOOL_INPUT_STARTED),
            ("TOOL_INPUT_DELTA", k::TOOL_INPUT_DELTA),
            ("TOOL_INPUT_ENDED", k::TOOL_INPUT_ENDED),
            ("TOOL_CALLED", k::TOOL_CALLED),
            ("TOOL_PROGRESS", k::TOOL_PROGRESS),
            ("TOOL_SUCCESS", k::TOOL_SUCCESS),
            ("TOOL_FAILED", k::TOOL_FAILED),
            ("COMPACTION_STARTED", k::COMPACTION_STARTED),
            ("COMPACTION_DELTA", k::COMPACTION_DELTA),
            ("COMPACTION_ENDED", k::COMPACTION_ENDED),
            ("COMPACTION_FAILED", k::COMPACTION_FAILED),
        ] {
            assert!(
                value.starts_with("session."),
                "{const_name} = {value:?} must be a session.* event"
            );
            assert!(
                !value.starts_with("session.next."),
                "{const_name} = {value:?} must not carry the 1.19.0 bridge prefix"
            );
        }
    }
}