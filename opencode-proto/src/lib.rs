//! Worker <-> QML protocol: the single place describing the service
//! prefixes/labels used in chat feed lines, plus helpers that build lines.
//!
//! This crate is intentionally dependency-free so the protocol can be unit
//! tested on a normal host (`cargo test -p opencode-proto` / `make test`),
//! unlike `opencode-client`, which needs the SailfishOS cross toolchain.
//!
//! Mirrored on the QML side in `opencode-client/qml/harbour-opencode.qml`
//! (readonly constants `kTsOpen`, `kTsClose`, `kThinkTag`, `kUserPrefix`,
//! `kSvcOpen`, `kErrOpen`, `kTagClose`, `kErrTag`, `kSttTag`, `kTtsTag`,
//! `kTtsStopTag`, `kSttDoneTag`, `kArgSep`, `kMsgIdOpen`, `kSetMsgIdTag`,
//! `kRevertedTag`, `kToolTag`, `kPartIdOpen`, `kClearFeedTag`, plus helpers
//! `stripTs`, `lineIs`, `partIdOf`, `stripPartId`, `liveIndexOf`). Keep both
//! files in sync: only change a marker here and in the QML mirror.

/// Line timestamp opener: `[[t:<ms>]]`.
pub const TS_OPEN: &str = "[[t:";
/// Line timestamp closer: `]]`.
pub const TS_CLOSE: &str = "]]";
/// Model-reasoning label. Placed AFTER the timestamp, BEFORE the answer text.
pub const THINK_TAG: &str = "[[think]]";
/// Live model-reasoning update. The worker streams `message.part.delta` chunks
/// as `[[thinklive]]<text>[[p:<partID>]]`; QML replaces the bubble carrying that
/// `partID` in place (or starts a new one), so thinking is visible WHILE the
/// model works. At `message.part.updated`/idle the worker sends the final
/// reasoning once more with the SAME `partID` — never duplicated.
pub const THINK_LIVE_TAG: &str = "[[thinklive]]";
/// Live answer update. The worker streams `message.part.delta` chunks of every
/// answer text part as `[[answerlive]]<text>[[p:<partID>]]` (reasoning, if any,
/// travels in separate `type:"reasoning"` parts as `[[thinklive]]`). QML
/// replaces the bubble carrying that `partID` in place, so the answer is visible
/// WHILE the model types — the same way opencode's TUI renders a text part on
/// each delta. The `partID` is what makes the replacement position-independent:
/// the text-end re-push can land after interleaved tool rows and still edits the
/// right bubble (a position-based heuristic used to append a duplicate).
pub const ANSWER_LIVE_TAG: &str = "[[answerlive]]";
/// Part-id marker appended to the TAIL of a live line:
/// `[[answerlive]]<text>[[p:<partID>]]`. QML reads it to find the exact bubble
/// to replace regardless of how many other lines landed in between. Mirrored as
/// `kPartIdOpen` in the QML, parsed by `partIdOf`/`liveIndexOf` there.
pub const PART_ID_OPEN: &str = "[[p:";
/// User-line prefix (goes after the timestamp).
pub const USER_PREFIX: &str = ">>> ";
/// Message-id marker, appended to the TAIL of a user-history line:
/// `[[t:<ms>]]>>> <text>[[m:<messageID>]]`. The id is the server `messageID`,
/// needed to offer revert/fork on that bubble. Only the worker attaches it
/// (from history — the live path learns the id via [`SET_MSG_ID_TAG`]).
pub const MSG_ID_OPEN: &str = "[[m:";
/// Control line carrying the id of the just-sent user message
/// (`[[msgid]]<messageID>`): the worker learns it from `message.updated`
/// (role=user) and QML attaches it to the last user bubble that lacks an id.
pub const SET_MSG_ID_TAG: &str = "[[msgid]]";
/// Control line carrying the session revert flag (`[[reverted]]<0|1>`): QML
/// shows a "Restore" banner while the tail is reverted.
pub const REVERTED_TAG: &str = "[[reverted]]";
/// Tool-call line, rendered INLINE in the transcript (like opencode's TUI):
/// `[[t:<ms>]][[tool]]<callID>\u001f<name>\u001f<status>\u001f<text>`.
/// The worker re-emits it on EVERY state change; QML keeps one row per
/// `callID` and replaces it in place. `status` ∈ `running` | `completed` |
/// `error`; `text` is the command/path/pattern (newlines flattened).
pub const TOOL_OPEN: &str = "[[tool]]";
/// Recognized STT-text label (worker sends `[[stt]] <text>`).
pub const STT_TAG: &str = "[[stt]]";
/// Finished TTS-WAV label (worker sends `[[tts]]<path>`).
pub const TTS_TAG: &str = "[[tts]]";
/// Control: stop playback and drop the whole speech queue (worker emits when the
/// user starts dictating or presses a stop button; QML silences its player and
/// does not report the stop back as a finished playback).
pub const TTS_STOP_TAG: &str = "[[ttsstop]]";
/// Control: STT cycle finished (worker emits; QML stops the mic loader).
pub const STT_DONE_TAG: &str = "[[sttdone]]";
/// Control: drop the whole visible transcript (`[[clearfeed]]`). The worker
/// emits it right before re-loading a session's history when the server
/// reports the session was compacted, so QML replaces the feed instead of
/// appending the reloaded history to the old one.
pub const CLEAR_FEED_TAG: &str = "[[clearfeed]]";

// i18n tokens (see `i18n.rs`): the worker cannot call QML `qsTr`, so instead
// of building display text it emits a translation KEY plus placeholder args:
//   `[[t:<ms>]][[s:KEY]]`        service notice
//   `[[t:<ms>]][[s:KEY\u001fARG1]]`  notice with args (unit-separator)
//   `[[t:<ms>]][[e:KEY\u001fARG1]]`  error (red bubble + in-app notification)
// QML resolves the active translation via `qsTr(key)` in `app.trSvc()`.
/// Service-notice token opener.
pub const SVC_OPEN: &str = "[[s:";
/// Error token opener.
pub const ERR_OPEN: &str = "[[e:";
/// Generic token closer (`]]`, same as the timestamp closer).
pub const TAG_CLOSE: &str = "]]";
/// Argument separator inside i18n tokens (unit separator; safe in chat lines).
pub const ARG_SEP: char = '\u{1f}';

/// Bubble marker for error lines. Built by QML only (red bubble styling);
/// the worker never sends it — it sends `[["e":…]]` tokens instead.
pub const ERR_TAG: &str = "[[e]]";

// ── Connection / Server daemon status ──────────────────────────────────────
/// Daemon connecting / negotiating SSE stream.
pub const STATUS_CONNECTING: &str = "connecting";
/// Daemon connected, SSE stream active, session idle.
pub const STATUS_IDLE: &str = "idle";
pub const STATUS_BUSY: &str = "busy";
/// Daemon / SSE connection broken.
pub const STATUS_ERROR: &str = "error";
/// Prefix for retry state: `retry|<next_ms>|<attempt>|<message>`.
pub const STATUS_RETRY_PREFIX: &str = "retry|";

/// Strongly-typed connection lifecycle state of the background daemon / SSE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerConnectionStatus {
    Connecting,
    Idle,
    Busy,
    Error,
    Retry {
        next: u64,
        attempt: u32,
        message: String,
    },
}

impl ServerConnectionStatus {
    pub fn as_base_str(&self) -> &str {
        match self {
            ServerConnectionStatus::Connecting => STATUS_CONNECTING,
            ServerConnectionStatus::Idle => STATUS_IDLE,
            ServerConnectionStatus::Busy => STATUS_BUSY,
            ServerConnectionStatus::Error => STATUS_ERROR,
            ServerConnectionStatus::Retry { .. } => STATUS_BUSY,
        }
    }

    pub fn to_status_string(&self) -> String {
        match self {
            ServerConnectionStatus::Connecting => STATUS_CONNECTING.to_string(),
            ServerConnectionStatus::Idle => STATUS_IDLE.to_string(),
            ServerConnectionStatus::Busy => STATUS_BUSY.to_string(),
            ServerConnectionStatus::Error => STATUS_ERROR.to_string(),
            ServerConnectionStatus::Retry { next, attempt, message } => {
                format!("{STATUS_RETRY_PREFIX}{next}|{attempt}|{message}")
            }
        }
    }
}

/// Build a feed line with a timestamp: `[[t:<ts>]]<body>`.
pub fn proto_line(ts: u64, body: &str) -> String {
    format!("{TS_OPEN}{ts}{TS_CLOSE}{body}")
}

/// Build a user feed line, appending the message-id marker when `id` is known:
/// `[[t:<ts>]]>>> <text>[[m:<id>]]`. An empty `id` yields a plain user line.
pub fn proto_user_line(ts: u64, id: &str, text: &str) -> String {
    let base = format!("{TS_OPEN}{ts}{TS_CLOSE}{USER_PREFIX}{text}");
    if id.is_empty() {
        base
    } else {
        format!("{base}{MSG_ID_OPEN}{id}{TAG_CLOSE}")
    }
}

/// Build an inline tool-call line. Newlines and the arg separator are flattened
/// to spaces so the line survives the `\u001e`-joined / `\u001f`-split protocol.
pub fn proto_tool_line(ts: u64, call_id: &str, name: &str, status: &str, text: &str) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' || c == ARG_SEP { ' ' } else { c })
        .collect();
    let body = format!("{TOOL_OPEN}{call_id}{ARG_SEP}{name}{ARG_SEP}{status}{ARG_SEP}{clean}");
    proto_line(ts, &body)
}

/// Build a live answer/reasoning line:
/// `[[t:<ts>]]<tag><text>[[p:<partID>]]` (`tag` is [`ANSWER_LIVE_TAG`] or
/// [`THINK_LIVE_TAG`]). The trailing `partID` lets QML replace the exact bubble
/// for that part whatever else has been appended to the feed meanwhile — the
/// position-based replacement it replaces appended a duplicate final answer
/// whenever a tool row slipped in between the last delta and the text-end push.
pub fn proto_live_line(ts: u64, tag: &str, text: &str, part_id: &str) -> String {
    proto_line(ts, &format!("{tag}{text}{PART_ID_OPEN}{part_id}{TAG_CLOSE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live line must end with the part-id marker so QML can key the bubble
    /// by part, independent of its position in the feed.
    #[test]
    fn live_line_carries_part_id_at_the_tail() {
        let line = proto_live_line(42, ANSWER_LIVE_TAG, "hello", "prt_abc");
        assert_eq!(line, "[[t:42]][[answerlive]]hello[[p:prt_abc]]");
    }

    /// Reasoning uses the same shape with the think-live tag.
    #[test]
    fn think_live_line_carries_part_id() {
        let line = proto_live_line(7, THINK_LIVE_TAG, "think", "prt_x");
        assert_eq!(line, "[[t:7]][[thinklive]]think[[p:prt_x]]");
    }

    /// The text sits between the live tag and the part-id marker.
    #[test]
    fn live_line_keeps_text_before_the_marker() {
        let line = proto_live_line(1, ANSWER_LIVE_TAG, "a[[b", "id");
        assert!(line.contains("a[[b"));
        assert!(line.ends_with("[[p:id]]"));
    }

    /// A user line carries the message id only when it is known.
    #[test]
    fn user_line_appends_msg_id_only_when_known() {
        assert_eq!(
            proto_user_line(5, "", "hi"),
            "[[t:5]]>>> hi"
        );
        assert_eq!(
            proto_user_line(5, "msg1", "hi"),
            "[[t:5]]>>> hi[[m:msg1]]"
        );
    }

    /// Tool rows flatten newlines and the arg separator so the line survives
    /// the unit-separator framing.
    #[test]
    fn tool_line_flattens_separators() {
        let line = proto_tool_line(3, "call_1", "bash", "running", "ls\n-a\u{1f}x");
        assert!(line.starts_with("[[t:3]][[tool]]call_1\u{1f}bash\u{1f}running\u{1f}"));
        assert!(line.ends_with("ls -a x"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn server_connection_status_serialization() {
        assert_eq!(ServerConnectionStatus::Connecting.to_status_string(), STATUS_CONNECTING);
        assert_eq!(ServerConnectionStatus::Idle.to_status_string(), STATUS_IDLE);
        assert_eq!(ServerConnectionStatus::Busy.to_status_string(), STATUS_BUSY);
        assert_eq!(ServerConnectionStatus::Error.to_status_string(), STATUS_ERROR);
        assert_eq!(
            ServerConnectionStatus::Retry {
                next: 1000,
                attempt: 2,
                message: "quota exceeded".to_string(),
            }
            .to_status_string(),
            "retry|1000|2|quota exceeded"
        );
    }
}
