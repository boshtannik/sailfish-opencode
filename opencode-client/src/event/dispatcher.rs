use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::i18n::{err_line, now_ms, svc_line};
use crate::markers::{
    proto_live_line, ANSWER_LIVE_TAG, THINK_LIVE_TAG,
};
use crate::state::permission::PermissionQueue;
use crate::state::form::FormQueue;
use crate::state::session::SessionFsm;
use crate::state::tool::ToolStore;
use crate::types::tool::ToolState;
use crate::types::v2::EventEnvelope;
use crate::types::v2::keys as k;

/// Minimum interval between two live pushes (reasoning or answer) — throttles a
/// long model turn so the QML queue is not flooded with tiny chunks. The final
/// full text is always sent once at `message.part.updated`/idle.
const REASONING_LIVE_MS: u64 = 150;

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
    /// Set by `dispatch_v2` when the server reports a session was compacted
    /// (`session.compaction.ended`): the caller must re-load that session's
    /// history.
    /// The reload itself is HTTP IO and lives in the SSE loop — here we only
    /// record the intent, keeping "what the event means" in one place.
    pub compacted: &'a mut Option<String>,
    /// Session already announced with a "Compacting…" notice, so a repeated
    /// `session.compaction.started` does not stack duplicate bubbles. Reset by
    /// the history reload on `session.compaction.ended`.
    pub compaction_seen: &'a mut Option<String>,
    /// Shared "compaction in progress" flag (polled by QML for the activity
    /// banner). Set on `session.compaction.started` for the shown session
    /// (covers AUTO-compaction), cleared when the session is compacted or goes
    /// idle.
    pub compacting: &'a std::sync::Mutex<bool>,
}

// ── v2 event dispatch (`GET /api/event`) ─────────────────────────────────
//
// v2 events arrive as flat envelopes (`{id, type, created, data}`) on
// `/api/event`; their fields and identifiers differ from v1 (see
// `vault/Разведка-v2.md` §2–§4). The handlers below normalise them onto the
// `DispatcherState`/`StreamState` accumulators the main loop renders, keeping
// "what a server event does" (live text/reasoning, idle flush, tools strip)
// here.
//
// Key mapping (v1 → v2):
//   * text/reasoning part id → `(assistantMessageID, ordinal)`;
//   * tool `callID` → `id`;
//   * `message.updated` (register assistant) → `session.step.started` /
//     `session.text.started` (`assistantMessageID`);
//   * `session.compacted` → `session.compaction.ended`.

/// Stable accumulator key for a v2 text block: `(assistantMessageID, ordinal)`.
/// opencode numbers ordinals SEPARATELY for reasoning and text parts (both
/// start at 0), so the kind prefix keeps a reasoning block and a text block of
/// the same message from colliding on one `[[p:key]]` (the official TUI
/// namespaces parts the same way: `text:<ord>` / `reasoning:<ord>`).
fn v2_text_key(assistant_message_id: &str, ordinal: i64) -> String {
    format!("t:{assistant_message_id}:{ordinal}")
}

/// Accumulator key for a v2 reasoning block — see [`v2_text_key`].
fn v2_reasoning_key(assistant_message_id: &str, ordinal: i64) -> String {
    format!("r:{assistant_message_id}:{ordinal}")
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
                    stream.assistant_messages.insert(d.assistant_message_id.clone());
                    // Auto-compaction runs its summary as a regular step with
                    // no `mode:"compaction"` marker: while the compacting
                    // banner is up, any message that opens IS the summary —
                    // mark it so auto-TTS never speaks it (v1 keyed this off
                    // `mode:"compaction"` at message.updated).
                    if stream.current_session == Some(d.session_id.as_str())
                        && stream.compacting.lock().map(|g| *g).unwrap_or(false)
                    {
                        stream.compaction_messages.insert(d.assistant_message_id);
                    }
                    // The step carries the model in use — a source of the
                    // session's current model on v2 (whose session GET/list have
                    // no `model`), alongside `session.model.selected`.
                    if let Some(m) = record_step_model(d.model.as_ref()) {
                        state.session_models.insert(d.session_id, m);
                    }
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
                // A user-initiated abort also arrives as `step.failed` (payload
                // `{error:{type:"aborted",…}}`) right before
                // `session.execution.interrupted`. In v1 an abort was a clean
                // stop, so we must NOT paint the error banner for it — the
                // execution.* arm below drives the turn end. Real failures
                // (non-aborted) still surface as errors.
                if !is_aborted_error(d.error.as_ref()) {
                    on_session_error_v2(stream, d.error.as_ref());
                }
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── answer text ───────────────────────────────────────────────
        k::TEXT_STARTED => match from_data::<v2::TextStartedData>(env) {
            Ok(d) => {
                stream.assistant_messages.insert(d.assistant_message_id.clone());
                // Same defensive compaction guard as STEP_STARTED: a text
                // block that opens while the banner is up belongs to the
                // summary — keep it out of auto-TTS.
                if stream.current_session == Some(d.session_id.as_str())
                    && stream.compacting.lock().map(|g| *g).unwrap_or(false)
                {
                    stream.compaction_messages.insert(d.assistant_message_id);
                }
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
                on_tool_input_started_v2(state, stream, &d.id, &d.name);
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
                on_tool_input_ended_v2(state, stream, &d.id, &d.text);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_CALLED => match from_data::<v2::ToolCalledData>(env) {
            Ok(d) => {
                on_tool_called_v2(state, stream, &d.id, &env.data);
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
                on_tool_success_v2(state, stream, &d.id, &env.data);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::TOOL_FAILED => match from_data::<v2::ToolFailedData>(env) {
            Ok(d) => {
                on_tool_failed_v2(state, stream, &d.id, &env.data);
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
        k::COMPACTION_DELTA => match from_data::<v2::CompactionDeltaData>(env) {
            Ok(d) => {
                on_compaction_delta_v2(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::COMPACTION_ENDED => match from_data::<v2::CompactionEndedData>(env) {
            Ok(d) => {
                on_compaction_ended_v2(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::COMPACTION_FAILED => match from_data::<v2::CompactionFailedData>(env) {
            Ok(d) => {
                on_compaction_failed_v2(stream, &d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── session lifecycle ─────────────────────────────────────────
        // The v2 session GET/list does not return the model, so record it from
        // the dedicated event; the worker merges it into the sessions JSON.
        k::MODEL_SELECTED => match from_data::<v2::ModelSelectedData>(env) {
            Ok(d) => {
                state.session_models.insert(
                    d.session_id.clone(),
                    (d.model.providerID.clone(), d.model.id.clone()),
                );
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::RENAMED | k::MOVED | k::AGENT_SELECTED => {
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
        | k::USAGE_RECORDED | k::USAGE_UPDATED | k::EXECUTION_STARTED => true,
        // ── turn lifecycle (2.0.32) ───────────────────────────────────
        // The real turn-end signal: `session.execution.succeeded` on success,
        // `session.execution.interrupted` on a user stop. Both are a clean end
        // (the main loop's `on_v2_idle` flushes the answer and sets idle).
        // `session.execution.failed` is a genuine failure → error banner.
        k::EXECUTION_SUCCEEDED | k::EXECUTION_INTERRUPTED => true,
        k::EXECUTION_FAILED => {
            if let Ok(mut s) = stream.status.lock() {
                *s = "error".to_string();
            }
            true
        },
        // ── catalog/config invalidation (2.0.32): re-read lazily ──────
        k::PROJECT_UPDATED | k::INTEGRATION_UPDATED | k::PROVIDER_UPDATED | k::MODEL_UPDATED
        | k::AGENT_UPDATED | k::COMMAND_UPDATED | k::SKILL_UPDATED | k::WEBSEARCH_UPDATED
        | k::REFERENCE_UPDATED | k::PLUGIN_UPDATED => true,
        k::DELETED => {
            if let Some(sid) = env.data.get(k::SESSION_ID).and_then(|v| v.as_str()) {
                state.sessions.remove(sid);
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
        //
        // `permission.asked` carries the whole request (`Permission.Request`),
        // `permission.replied` only its id.
        k::PERMISSION_ASKED => match from_data::<crate::types::v2::permission::PermissionRequest>(env) {
            Ok(d) => {
                state.permissions.push(d);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::PERMISSION_REPLIED => match from_data::<v2::PermissionRepliedData>(env) {
            Ok(d) => {
                state.permissions.pop(&d.request_id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── forms (opencode v2 replaces the v1 question model) ───────
        //
        // `form.created` carries the whole form (`{form: Form.Info}`), the
        // settled events carry only the form id. The queue is the UI's source
        // of truth — the worker snapshots it into the shared buffer for QML.
        k::FORM_CREATED => match from_data::<v2::FormCreatedData>(env) {
            Ok(d) => {
                state.forms.push(d.form);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        k::FORM_REPLIED | k::FORM_CANCELLED => match from_data::<v2::FormResolvedData>(env) {
            Ok(d) => {
                state.forms.pop(&d.id);
                true
            }
            Err(e) => from_data_error(env, &e),
        },
        // ── session errors ────────────────────────────────────────────
        //
        // `session.error` carries a structured error (`{type, message}`). The
        // permission-related one matters for the queue: `permission.rejected`
        // means the model gave up on the request, so the pending card must
        // not stay on screen (the server's list is the authority — the poll
        // would drop it too, but the card would linger until then).
        k::SESSION_ERROR => {
            let err = env.data.get(k::ERROR).cloned().unwrap_or_default();
            let err_type = err.get(k::TYPE).and_then(|v| v.as_str()).unwrap_or("");
            if err_type == k::PERMISSION_REJECTED {
                let session = env
                    .data
                    .get(k::SESSION_ID)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                state.permissions.clear_session(&session);
                log::info!("permission rejected: очередь очищена для {session:?}");
            }
            false
        }
        // ── notifications we do not render ────────────────────────────
        k::SERVER_CONNECTED => false,
        // Everything else is a v2 event with no UI handling. Recognised types
        // (see `RECOGNISED_V2`) are ignored silently; a genuinely unknown type
        // is reported once (never dropped). The completeness test keeps
        // `RECOGNISED_V2` in lockstep with the server registry.
        _ => {
            if !is_recognised_v2(kind) {
                note_unimplemented_v2(kind);
            }
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
    let key = v2_text_key(&d.assistant_message_id, d.ordinal);
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
    let key = v2_text_key(&d.assistant_message_id, d.ordinal);
    if stream.text_live.contains_key(&key) {
        stream.text_live.insert(key, d.text.clone());
    }
}

/// v2 reasoning-delta: same as the answer path but into the think accumulator.
fn on_v2_reasoning_delta(
    stream: &mut StreamState<'_>,
    d: &crate::types::v2::event::TextDeltaData,
) {
    let key = v2_reasoning_key(&d.assistant_message_id, d.ordinal);
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
    let key = v2_reasoning_key(&d.assistant_message_id, d.ordinal);
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

/// Stable bubble key for the growing v2 compaction summary. Reuses the live
/// answer mechanism (`[[answerlive]]` + `[[p:<key>]]`) so the summary bubble
/// grows in place exactly like a normal answer and, like v1, can be replaced by
/// the final text in the same bubble.
fn v2_compaction_key(session_id: &str) -> String {
    format!("compact:{session_id}")
}

/// v2 `session.compaction.delta` — the growing summary. Mirror it to QML as a
/// live answer bubble (throttled), keyed so the text replace-in-place works.
fn on_compaction_delta_v2(stream: &mut StreamState<'_>, d: &crate::types::v2::event::CompactionDeltaData) {
    if stream.current_session != Some(d.session_id.as_str()) {
        return;
    }
    let key = v2_compaction_key(&d.session_id);
    let entry = stream.text_live.entry(key.clone()).or_default();
    entry.push_str(&d.text);
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

/// v2 `session.compaction.ended` — push the exact final summary text into the
/// live bubble (so it matches even if trailing deltas were throttled) and record
/// the session for the caller to reload (mirrors v1 `session.compacted`).
fn on_compaction_ended_v2(stream: &mut StreamState<'_>, d: &crate::types::v2::event::CompactionEndedData) {
    if stream.current_session == Some(d.session_id.as_str()) && !d.text.trim().is_empty() {
        let key = v2_compaction_key(&d.session_id);
        stream.text_live.insert(key.clone(), d.text.clone());
        if let Ok(mut q) = stream.pending.lock() {
            q.push(proto_live_line(now_ms(), ANSWER_LIVE_TAG, d.text.trim_end(), &key));
        }
    }
    *stream.compacted = Some(d.session_id.clone());
    if stream.current_session == Some(d.session_id.as_str()) {
        if let Ok(mut g) = stream.compacting.lock() {
            *g = false;
        }
    }
}

/// v2 `session.compaction.failed`. A `compaction.unavailable` ("Nothing to
/// compact yet") is a benign no-op, not a failure: show it as a plain
/// informational feed line and leave the status alone (v1 parity — the message
/// is informative, the red error banner is not). Any other error is surfaced.
fn on_compaction_failed_v2(stream: &mut StreamState<'_>, d: &crate::types::v2::event::CompactionFailedData) {
    if stream.current_session == Some(d.session_id.as_str()) {
        if let Ok(mut g) = stream.compacting.lock() {
            *g = false;
        }
    }
    if is_compaction_unavailable(&d.error) {
        let msg = human_session_error_value(Some(&d.error));
        if !msg.is_empty() {
            if let Ok(mut q) = stream.pending.lock() {
                q.push(svc_line(now_ms(), crate::i18n::K_COMPACTION_UNAVAILABLE, &[msg]));
            }
        }
        return;
    }
    on_session_error_v2(stream, Some(&d.error));
}

/// True when a v2 structured error is the benign "nothing to compact yet"
/// (`{type:"compaction.unavailable", …}`).
fn is_compaction_unavailable(error: &serde_json::Value) -> bool {
    use crate::types::v2::keys as k;
    error.get(k::TYPE).and_then(|v| v.as_str()) == Some(k::ERR_COMPACTION_UNAVAILABLE)
}

/// Extract `(providerID, id)` from a `session.step.started` `model` value
/// (`Model.Ref`), if present and complete. Returns `None` when absent/blank.
fn record_step_model(model: Option<&serde_json::Value>) -> Option<(String, String)> {
    use crate::types::v2::keys as k;
    let m = model?;
    let provider = m.get(k::PROVIDER_ID)?.as_str()?.to_string();
    let id = m.get(k::ID)?.as_str()?.to_string();
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    Some((provider, id))
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

/// True when a v2 structured error marks a user-initiated abort
/// (`{type:"aborted", …}`). Such a stop is a clean interrupt, not a failure, so
/// it must not raise the error banner (v1 parity).
fn is_aborted_error(error: Option<&serde_json::Value>) -> bool {
    use crate::types::v2::keys as k;
    error
        .and_then(|e| e.get(k::TYPE))
        .and_then(|v| v.as_str())
        == Some(k::ERR_ABORTED)
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
// `input`/`content`/`error` shapes. Each transition (except progress — it
// fires per output chunk and would spam the feed) also mirrors the new state
// into QML as a `[[tool]]` line, replace-in-place by callID on the QML side.
// v1 fed these rows via `MessagePartUpdated`; the push was lost in the v2 port.

/// Mirror the current ToolStore state of `id` into the QML feed.
fn push_tool_line(state: &DispatcherState, stream: &StreamState<'_>, id: &str) {
    let Some(st) = state.tools.state_of(id) else {
        return;
    };
    let name = state.tools.name_of(id).unwrap_or("");
    if let Ok(mut q) = stream.pending.lock() {
        q.push(crate::markers::proto_tool_line(
            now_ms(),
            id,
            name,
            st.ui_status(),
            &st.description(),
        ));
    }
}

fn on_tool_input_started_v2(
    state: &mut DispatcherState,
    stream: &StreamState<'_>,
    id: &str,
    name: &str,
) {
    state.tools.upsert(
        id.to_string(),
        name.to_string(),
        ToolState::Pending {
            input: serde_json::Value::Null,
        },
    );
    push_tool_line(state, stream, id);
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

fn on_tool_input_ended_v2(
    state: &mut DispatcherState,
    stream: &StreamState<'_>,
    id: &str,
    text: &str,
) {
    if state.tools.name_of(id).is_none() {
        state.tools.ensure(id, "");
    }
    let input = serde_json::from_str(text)
        .unwrap_or_else(|_| serde_json::Value::String(text.to_string()));
    state.tools.set_state(id, ToolState::Pending { input });
    push_tool_line(state, stream, id);
}

fn on_tool_called_v2(
    state: &mut DispatcherState,
    stream: &StreamState<'_>,
    id: &str,
    data: &serde_json::Value,
) {
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
    push_tool_line(state, stream, id);
}

fn on_tool_progress_v2(state: &mut DispatcherState, id: &str) {
    if state.tools.name_of(id).is_none() {
        return;
    }
    let input = state.tools.input_of(id);
    state.tools.set_state(id, ToolState::Running { input });
}

fn on_tool_success_v2(
    state: &mut DispatcherState,
    stream: &StreamState<'_>,
    id: &str,
    data: &serde_json::Value,
) {
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
    push_tool_line(state, stream, id);
}

fn on_tool_failed_v2(
    state: &mut DispatcherState,
    stream: &StreamState<'_>,
    id: &str,
    data: &serde_json::Value,
) {
    let input = state.tools.input_of(id);
    let error = data
        .get(k::ERROR)
        .map(content_to_string)
        .unwrap_or_default();
    state.tools.set_state(id, ToolState::Error { input, error });
    push_tool_line(state, stream, id);
}

/// Every v2 event type this client consciously knows about — either handled by
/// a dedicated arm in `dispatch_v2`, or recognised and deliberately ignored.
///
/// This is the strict-gate source of truth for
/// `types::v2::tests::every_2_0_32_event_is_recognised`: that test asserts the
/// set below is *exactly* the event registry of the vendored opencode fixture
/// (`tests/fixtures/v2-events-2.0.32.txt`). A new upstream event (or a renamed
/// one) makes the test fail, so drift is caught at build time rather than by a
/// silent `note_unimplemented_v2` warning at runtime.
///
/// Keep in sync with `types::v2::keys` (each entry is one of those constants)
/// and refresh the fixture via `scripts/gen-v2-fixtures.sh` when bumping
/// opencode. See `vault/Разведка-v2.md` §10.
pub const RECOGNISED_V2: &[&str] = &[
    // Turn / step / text / reasoning
    k::STEP_STARTED,
    k::STEP_ENDED,
    k::STEP_STREAMED,
    k::STEP_FAILED,
    k::TEXT_STARTED,
    k::TEXT_DELTA,
    k::TEXT_ENDED,
    k::REASONING_STARTED,
    k::REASONING_DELTA,
    k::REASONING_ENDED,
    // Tool calls
    k::TOOL_INPUT_STARTED,
    k::TOOL_INPUT_DELTA,
    k::TOOL_INPUT_ENDED,
    k::TOOL_CALLED,
    k::TOOL_PROGRESS,
    k::TOOL_SUCCESS,
    k::TOOL_FAILED,
    // Tool diagnostics (no UI effect)
    k::TOOL_EXECUTION,
    k::TOOL_INPUT_INCOMPLETE,
    k::TOOL_INPUT_JSON,
    k::TOOL_INTERRUPTED,
    k::TOOL_RESULT_MISSING,
    k::TOOL_UNKNOWN,
    // Compaction (session-scoped + top-level)
    k::COMPACTION_STARTED,
    k::COMPACTION_DELTA,
    k::COMPACTION_ENDED,
    k::COMPACTION_FAILED,
    k::COMPACTION_JOB_FAILED,
    k::COMPACTION_JOB_INTERRUPTED,
    k::COMPACTION_JOB_UNAVAILABLE,
    // Session lifecycle / metadata
    k::SESSION_CREATED,
    k::SESSION_UPDATED,
    k::SESSION_FORKED,
    k::SESSION_VIEWED,
    k::SESSION_COMPACTED,
    k::SESSION_DIFF,
    k::SESSION_ERROR,
    k::SESSION_PERMISSIONS,
    k::SESSION_RETRY_SCHEDULED,
    k::SESSION_SKILL_ACTIVATED,
    k::SESSION_INSTRUCTIONS_UPDATED,
    k::SESSION_METADATA_UPDATED,
    k::RENAMED,
    k::MOVED,
    k::DELETED,
    k::IDLE,
    k::STATUS,
    k::AGENT_SELECTED,
    k::MODEL_SELECTED,
    k::MESSAGE_CONTENT_UPDATED,
    k::USAGE_RECORDED,
    k::USAGE_UPDATED,
    k::EXECUTION_STARTED,
    k::EXECUTION_FAILED,
    k::EXECUTION_INTERRUPTED,
    k::EXECUTION_SUCCEEDED,
    // Session inbox (durable prompt queue)
    k::SESSION_INBOX_ENQUEUED,
    k::SESSION_INBOX_DELIVERED,
    k::SESSION_INBOX_CANCELLED,
    k::SESSION_INBOX_DELIVERY_CHANGED,
    // Revert / shell / synthetic
    k::REVERT_STAGED,
    k::REVERT_CLEARED,
    k::REVERT_COMMITTED,
    k::SHELL_STARTED,
    k::SHELL_ENDED,
    k::SHELL_CREATED,
    k::SHELL_DELETED,
    k::SHELL_EXITED,
    k::SYNTHETIC,
    // Catalog / config invalidation
    k::PROJECT_UPDATED,
    k::INTEGRATION_UPDATED,
    k::PROVIDER_UPDATED,
    k::MODEL_UPDATED,
    k::AGENT_UPDATED,
    k::COMMAND_UPDATED,
    k::SKILL_UPDATED,
    k::WEBSEARCH_UPDATED,
    k::REFERENCE_UPDATED,
    k::PLUGIN_UPDATED,
    k::SDK_PLUGIN_UPDATED,
    k::CONFIG_UPDATED,
    k::CREDENTIAL_UPDATED,
    k::CREDENTIAL_SWITCHED,
    k::INSTRUCTION_DISCOVERY_UPDATED,
    k::MODELS_DEV_REFRESHED,
    k::WELLKNOWN_UPDATED,
    k::LSP_UPDATED,
    k::VCS_BRANCH_UPDATED,
    k::MCP_TOOLS_CHANGED,
    k::MCP_PROMPTS_CHANGED,
    k::MCP_RESOURCES_CHANGED,
    k::MCP_STATUS_CHANGED,
    k::FILE_EDITED,
    k::FILESYSTEM_CHANGED,
    k::COMMAND_EXECUTED,
    k::INSTALLATION_UPDATED,
    k::INSTALLATION_UPDATE_AVAILABLE,
    // Message lifecycle (v1 names, still on the bus)
    k::MESSAGE_UPDATED,
    k::MESSAGE_REMOVED,
    k::MESSAGE_PART_UPDATED,
    k::MESSAGE_PART_DELTA,
    k::MESSAGE_PART_REMOVED,
    // Provider diagnostics
    k::PROVIDER_AUTH,
    k::PROVIDER_CONTENT_FILTER,
    k::PROVIDER_ERROR,
    k::PROVIDER_NO_ROUTE,
    k::PROVIDER_UNKNOWN,
    k::PROVIDER_UNSUPPORTED_OPERATION,
    // PTY / persistent PTY
    k::PTY_CREATED,
    k::PTY_UPDATED,
    k::PTY_DELETED,
    k::PTY_EXITED,
    k::PERSISTENT_PTY_ADDED,
    k::PERSISTENT_PTY_REMOVED,
    // Permissions / forms / server
    // NOTE: the v1 question model (`question.asked` & co) is NOT in the 2.0.32
    // event registry — opencode v2 asks with forms — so it is gone entirely.
    k::PERMISSION_ASKED,
    k::PERMISSION_REPLIED,
    k::PERMISSION_REJECTED,
    k::FORM_CREATED,
    k::FORM_REPLIED,
    k::FORM_CANCELLED,
    k::SERVER_CONNECTED,
    // Turn / queue
    k::TURN_IDLE,
    k::TURN_SEND,
    k::TURN_DURATION,
    k::QUEUED_PROMPTS,
    // Workspace / worktree
    k::WORKSPACE_READY,
    k::WORKSPACE_FAILED,
    k::WORKSPACE_STATUS,
    k::WORKTREE_READY,
    k::WORKTREE_FAILED,
    k::WORKTREE_RESOLVED,
    k::WORKTREE_UPDATED,
    // TUI-driven (no effect in this client)
    k::TUI_COMMAND_EXECUTE,
    k::TUI_PROMPT_APPEND,
    k::TUI_SESSION_SELECT,
    k::TUI_TOAST_SHOW,
    // UI / transport / rpc internals
    k::CLIENT_CONNECTION,
    k::GLOBAL_DISPOSED,
    k::LOCATION_SHUTDOWN,
    k::LOG_SYNCED,
    k::RPC_INTERNAL,
    k::RESPONSE_CREATE,
    k::RESPONSE_OUTPUT_ITEM_DONE,
    k::STREAM_PATCH,
    k::STREAM_SUBAGENT,
    k::STREAM_VIEW,
    k::UI_ARROW,
    k::UI_CLICK,
    k::UI_ENTER,
    k::UI_FOCUS,
    k::UI_MOUSE,
    k::UI_PRESS,
    k::UI_RESIZE,
    k::UI_TYPE,
];

/// Is `kind` an event type this client knows about (handled or intentionally
/// ignored)? Backs the `dispatch_v2` fallback so only genuinely unknown types
/// are reported.
pub fn is_recognised_v2(kind: &str) -> bool {
    RECOGNISED_V2.contains(&kind)
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

#[derive(Default)]
pub struct DispatcherState {
    pub sessions: std::collections::HashMap<String, SessionFsm>,
    /// Current model per session — `(providerID, modelID)`.
    ///
    /// `GET /api/session[/{id}]` includes `model` only once a model is bound to
    /// the session; a fresh/unset session has none. We record the model from the
    /// events that carry it (`session.model.selected`, `session.step.started`)
    /// and the worker also seeds it from history, so the sessions list / picker
    /// can show a current model even before the server list would.
    pub session_models: std::collections::HashMap<String, (String, String)>,
    pub tools: ToolStore,
    pub permissions: PermissionQueue,
    pub forms: FormQueue,
}

pub type SharedState = Arc<Mutex<DispatcherState>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Owns the accumulators a `StreamState` borrows, so tests do not repeat
    /// the full field list. `stream()` borrows the fixture mutably; the borrow
    /// ends with the returned value, so the test can read the accumulators
    /// afterwards.
    struct Fixture {
        pending: Mutex<Vec<String>>,
        status: Mutex<String>,
        assistant_messages: HashSet<String>,
        compaction_messages: HashSet<String>,
        reasonings: HashMap<String, String>,
        reasoning_part_ids: HashSet<String>,
        live_reasoned_ids: HashSet<String>,
        last_reasoning_push: u64,
        text_live: HashMap<String, String>,
        last_answer_push: u64,
        compacted: Option<String>,
        compaction_seen: Option<String>,
        compacting: Mutex<bool>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                pending: Mutex::new(Vec::new()),
                status: Mutex::new(String::new()),
                assistant_messages: HashSet::new(),
                compaction_messages: HashSet::new(),
                reasonings: HashMap::new(),
                reasoning_part_ids: HashSet::new(),
                live_reasoned_ids: HashSet::new(),
                last_reasoning_push: 0,
                text_live: HashMap::new(),
                last_answer_push: 0,
                compacted: None,
                compaction_seen: None,
                compacting: Mutex::new(false),
            }
        }

        /// A `StreamState` over this fixture's accumulators. The shown session
        /// (if any) must outlive the borrow, hence the shared lifetime.
        fn stream<'a>(&'a mut self, current_session: Option<&'a str>) -> StreamState<'a> {
            StreamState {
                pending: &self.pending,
                status: &self.status,
                assistant_messages: &mut self.assistant_messages,
                compaction_messages: &mut self.compaction_messages,
                reasonings: &mut self.reasonings,
                reasoning_part_ids: &self.reasoning_part_ids,
                live_reasoned_ids: &mut self.live_reasoned_ids,
                last_reasoning_push: &mut self.last_reasoning_push,
                text_live: &mut self.text_live,
                last_answer_push: &mut self.last_answer_push,
                current_session,
                compacted: &mut self.compacted,
                compaction_seen: &mut self.compaction_seen,
                compacting: &self.compacting,
            }
        }
    }

    /// A v2 envelope carrying `data` as the body of a `kind` event.
    fn envelope(kind: &str, data: serde_json::Value) -> EventEnvelope {
        EventEnvelope {
            id: "evt".to_string(),
            kind: kind.to_string(),
            created: 0,
            data,
            location: None,
            metadata: None,
            durable: None,
        }
    }

    /// A v2 text/reasoning delta body. v2 has no part id: the QML bubble is
    /// keyed by `(assistantMessageID, ordinal)`.
    fn text_delta(
        message: &str,
        ordinal: i64,
        text: &str,
    ) -> crate::types::v2::event::TextDeltaData {
        crate::types::v2::event::TextDeltaData {
            session_id: "ses".to_string(),
            assistant_message_id: message.to_string(),
            ordinal,
            delta: text.to_string(),
        }
    }

    /// The live answer line MUST carry the block key so QML can edit the exact
    /// bubble in place, position-independently (the duplicate-bubble fix). The
    /// key is namespaced `t:` so it never collides with a reasoning block of
    /// the same message (opencode numbers ordinals separately per part kind).
    #[test]
    fn answer_delta_line_carries_block_key() {
        let mut fx = Fixture::new();
        fx.assistant_messages.insert("msg1".to_string());
        {
            let mut stream = fx.stream(None);
            on_v2_text_delta(&mut stream, &text_delta("msg1", 0, "hello"));
        }
        let q = fx.pending.lock().unwrap();
        assert_eq!(q.len(), 1, "{q:?}");
        assert!(q[0].ends_with("[[answerlive]]hello[[p:t:msg1:0]]"), "{}", q[0]);
    }

    /// Reasoning deltas route to the think-live tag, same block-key shape,
    /// namespaced `r:`.
    #[test]
    fn reasoning_delta_line_carries_block_key() {
        let mut fx = Fixture::new();
        {
            let mut stream = fx.stream(None);
            on_v2_reasoning_delta(&mut stream, &text_delta("msg1", 3, "why"));
        }
        let q = fx.pending.lock().unwrap();
        assert_eq!(q.len(), 1, "{q:?}");
        assert!(q[0].ends_with("[[thinklive]]why[[p:r:msg1:3]]"), "{}", q[0]);
    }

    /// A reasoning block and a text block of the same message BOTH start at
    /// ordinal 0 (opencode numbers them per part kind). Their live keys must
    /// differ, or the answer bubble replaces the think bubble in place
    /// (the disappearing-think-bubble bug).
    #[test]
    fn reasoning_and_text_blocks_of_one_message_have_distinct_keys() {
        let mut fx = Fixture::new();
        fx.assistant_messages.insert("msg1".to_string());
        {
            let mut stream = fx.stream(None);
            on_v2_reasoning_delta(&mut stream, &text_delta("msg1", 0, "think"));
            on_v2_text_delta(&mut stream, &text_delta("msg1", 0, "answer"));
        }
        assert!(fx.reasonings.contains_key("r:msg1:0"), "{:?}", fx.reasonings);
        assert!(fx.text_live.contains_key("t:msg1:0"), "{:?}", fx.text_live);
        let q = fx.pending.lock().unwrap();
        assert_eq!(q.len(), 2, "{q:?}");
        assert!(q[0].contains("[[thinklive]]"), "{}", q[0]);
        assert!(q[1].contains("[[answerlive]]"), "{}", q[1]);
    }

    /// A text delta of a message that never registered (not in
    /// `assistant_messages`) is ignored — no line pushed.
    #[test]
    fn text_delta_of_unregistered_message_is_ignored() {
        let mut fx = Fixture::new();
        {
            let mut stream = fx.stream(None);
            on_v2_text_delta(&mut stream, &text_delta("msgX", 0, "hello"));
        }
        let q = fx.pending.lock().unwrap();
        assert!(q.is_empty(), "{q:?}");
    }

    /// `session.compaction.ended` records the session to reload, so the SSE
    /// loop can rebuild the transcript (summary replaces the old turns)
    /// without the user leaving and re-entering the chat.
    #[test]
    fn compaction_ended_records_the_session_to_reload() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let env = envelope(
            k::COMPACTION_ENDED,
            serde_json::json!({ "sessionID": "ses_compact", "reason": "auto", "text": "" }),
        );
        let mut state = DispatcherState::default();
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&env, &mut state, &mut stream));
        }
        assert_eq!(fx.compacted.as_deref(), Some("ses_compact"));
    }

    /// A v2 permission request as the server sends it (`Permission.Request`).
    fn permission_request(id: &str, session: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "sessionID": session,
            "action": "bash",
            "resources": ["rm -rf /tmp/x"],
            "save": ["bash"],
            "message": "Run it?",
            "source": { "type": "tool", "messageID": "msg_1", "id": "call_1" }
        })
    }

    /// `permission.asked` puts the whole request into the queue — the card
    /// needs the action, the resources and the server's explanation.
    #[test]
    fn permission_asked_queues_the_request() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        let env = envelope(k::PERMISSION_ASKED, permission_request("per_1", "ses1"));
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&env, &mut state, &mut stream));
        }
        let p = state.permissions.first().expect("the request is queued");
        assert_eq!(p.id, "per_1");
        assert_eq!(p.session_id, "ses1");
        assert_eq!(p.action, "bash");
        assert_eq!(p.resources, vec!["rm -rf /tmp/x".to_string()]);
        assert_eq!(p.message.as_deref(), Some("Run it?"));
        assert_eq!(p.source.as_ref().map(|s| s.id.as_str()), Some("call_1"));
    }

    /// The same request twice (the event and the poll) must not double the
    /// card.
    #[test]
    fn permission_asked_is_not_duplicated() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        let env = envelope(k::PERMISSION_ASKED, permission_request("per_1", "ses1"));
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&env, &mut state, &mut stream));
            assert!(dispatch_v2(&env, &mut state, &mut stream));
        }
        assert_eq!(state.permissions.pending.len(), 1);
    }

    /// `permission.replied` settles the request — the card must go away.
    #[test]
    fn permission_replied_drops_the_request() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        let asked = envelope(k::PERMISSION_ASKED, permission_request("per_1", "ses1"));
        let replied = envelope(
            k::PERMISSION_REPLIED,
            serde_json::json!({ "sessionID": "ses1", "requestID": "per_1", "reply": "once" }),
        );
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&asked, &mut state, &mut stream));
            assert!(state.permissions.contains("per_1"));
            assert!(dispatch_v2(&replied, &mut state, &mut stream));
        }
        assert!(!state.permissions.contains("per_1"));
    }

    /// `session.error` with `permission.rejected` means the model gave up on
    /// the request — the ghost card must disappear (only for that session).
    #[test]
    fn permission_rejected_error_clears_the_session() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        let a = envelope(k::PERMISSION_ASKED, permission_request("per_1", "ses1"));
        let b = envelope(k::PERMISSION_ASKED, permission_request("per_2", "ses2"));
        let rejected = envelope(
            k::SESSION_ERROR,
            serde_json::json!({
                "sessionID": "ses1",
                "error": { "type": "permission.rejected", "message": "the model gave up" }
            }),
        );
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&a, &mut state, &mut stream));
            assert!(dispatch_v2(&b, &mut state, &mut stream));
            dispatch_v2(&rejected, &mut state, &mut stream);
        }
        assert!(!state.permissions.contains("per_1"));
        assert!(state.permissions.contains("per_2"));
    }

    /// `session.compaction.started` pushes ONE "Compacting…" notice for the
    /// shown session (a repeat does not stack a second bubble) and raises the
    /// banner; another session's compaction is ignored; `session.compaction.ended`
    /// lowers the banner again (the reload replaces the feed). This is what
    /// makes AUTO-compaction (context overflow) visible, not just the button.
    #[test]
    fn compaction_started_announces_once_for_the_shown_session() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        let start = |session: &str| {
            envelope(
                k::COMPACTION_STARTED,
                serde_json::json!({ "sessionID": session, "reason": "auto" }),
            )
        };
        {
            let mut stream = fx.stream(Some("ses1"));
            // Another session's compaction: no notice, flag stays down.
            assert!(dispatch_v2(&start("other"), &mut state, &mut stream));
            assert!(!*stream.compacting.lock().unwrap());
            // Shown session, twice: ONE notice.
            assert!(dispatch_v2(&start("ses1"), &mut state, &mut stream));
            assert!(dispatch_v2(&start("ses1"), &mut state, &mut stream));
            // The banner flag is up while the compaction runs.
            assert!(*stream.compacting.lock().unwrap());
            // Empty summary text: the notice itself is the only line pushed.
            let done = envelope(
                k::COMPACTION_ENDED,
                serde_json::json!({ "sessionID": "ses1", "reason": "auto", "text": "" }),
            );
            assert!(dispatch_v2(&done, &mut state, &mut stream));
            assert!(!*stream.compacting.lock().unwrap());
        }
        let q = fx.pending.lock().unwrap();
        assert_eq!(q.len(), 1, "expected exactly one notice, got {q:?}");
        assert!(q[0].contains("[[s:Compacting history"), "{}", q[0]);
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

    /// `session.model.selected` records the session's current model. The v2
    /// session GET/list do not carry `model`, so this map is the only source the
    /// sessions/picker list can use on v2.
    #[test]
    fn v2_model_selected_records_the_session_model() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let env = envelope(
            k::MODEL_SELECTED,
            serde_json::json!({
                "sessionID": "ses_x",
                "model": { "id": "claude-sonnet-4", "providerID": "anthropic" },
            }),
        );
        let mut state = DispatcherState::default();
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&env, &mut state, &mut stream));
        }
        assert_eq!(
            state.session_models.get("ses_x"),
            Some(&("anthropic".to_string(), "claude-sonnet-4".to_string()))
        );
    }

    /// `session.step.started` also records the model in use — a second source of
    /// the session's current model on v2 (alongside `session.model.selected`).
    #[test]
    fn v2_step_started_records_the_model() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let env = envelope(
            k::STEP_STARTED,
            serde_json::json!({
                "sessionID": "ses_y",
                "assistantMessageID": "msg_a",
                "model": { "id": "gpt-5", "providerID": "openai" },
            }),
        );
        let mut state = DispatcherState::default();
        {
            let mut stream = fx.stream(None);
            assert!(dispatch_v2(&env, &mut state, &mut stream));
        }
        assert_eq!(
            state.session_models.get("ses_y"),
            Some(&("openai".to_string(), "gpt-5".to_string()))
        );
    }

    /// Live tool calls: `tool.input.started` opens a `[[tool]]` row and
    /// `tool.success` replaces it in place with the completed status. Without
    /// these pushes the feed row only appeared after a session switch (the
    /// v1 `MessagePartUpdated` push was lost in the v2 port).
    #[test]
    fn tool_lifecycle_pushes_tool_lines() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        {
            let mut stream = fx.stream(None);
            let started = envelope(
                k::TOOL_INPUT_STARTED,
                serde_json::json!({
                    "sessionID": "ses",
                    "assistantMessageID": "msg1",
                    "id": "call_1",
                    "name": "bash",
                }),
            );
            assert!(dispatch_v2(&started, &mut state, &mut stream));
            let ok = envelope(
                k::TOOL_SUCCESS,
                serde_json::json!({
                    "sessionID": "ses",
                    "assistantMessageID": "msg1",
                    "id": "call_1",
                    "content": [{ "type": "text", "text": "done" }],
                }),
            );
            assert!(dispatch_v2(&ok, &mut state, &mut stream));
        }
        let q = fx.pending.lock().unwrap();
        let tools: Vec<&String> = q.iter().filter(|l| l.contains("[[tool]]")).collect();
        assert_eq!(tools.len(), 2, "{q:?}");
        assert!(
            tools[0].contains("call_1") && tools[0].contains("bash") && tools[0].contains("running"),
            "{}",
            tools[0]
        );
        assert!(
            tools[1].contains("call_1") && tools[1].contains("completed"),
            "{}",
            tools[1]
        );
    }

    /// `session.tool.progress` must NOT push a feed line: it fires per output
    /// chunk and would flood the transcript (status stays "running").
    #[test]
    fn tool_progress_does_not_push_lines() {
        use crate::types::v2::keys as k;
        let mut fx = Fixture::new();
        let mut state = DispatcherState::default();
        {
            let mut stream = fx.stream(None);
            let started = envelope(
                k::TOOL_INPUT_STARTED,
                serde_json::json!({
                    "sessionID": "ses",
                    "assistantMessageID": "msg1",
                    "id": "call_1",
                    "name": "bash",
                }),
            );
            assert!(dispatch_v2(&started, &mut state, &mut stream));
            let progress = envelope(
                k::TOOL_PROGRESS,
                serde_json::json!({
                    "sessionID": "ses",
                    "assistantMessageID": "msg1",
                    "id": "call_1",
                }),
            );
            assert!(dispatch_v2(&progress, &mut state, &mut stream));
        }
        let q = fx.pending.lock().unwrap();
        let tools: Vec<&String> = q.iter().filter(|l| l.contains("[[tool]]")).collect();
        assert_eq!(tools.len(), 1, "progress must not push: {q:?}");
    }
}
