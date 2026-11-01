//! i18n bridge for worker-side service messages.
//!
//! The Rust worker cannot call QML `qsTr`, so instead of building display text
//! it emits a structured token with a translation key and placeholder args
//! (see `markers.rs`, `SVC_OPEN` / `ERR_OPEN`). QML resolves the active
//! translation via `qsTr(key)` (`app.trSvc()`) and substitutes `%1`..`%9`.
//!
//! The keys below are the <source> strings of the .ts translation files
//! (kept language-neutral English). When a new key is added here it must also
//! be added to the .ts files, otherwise localized builds keep the English text.

use crate::markers::*;

pub const K_SERVER_ERROR: &str = "Server error: %1";
pub const K_SESSION_ERROR: &str = "Session error: %1";
pub const K_PROMPT_ERROR: &str = "Prompt error: %1";

pub const K_MODEL_DOWNLOADED: &str = "Voice model \"%1\" downloaded";
pub const K_MODEL_DOWNLOAD_FAILED: &str = "Failed to download voice model \"%1\"";
pub const K_STT_NO_MODEL_SELECTED: &str = "Select an STT model in settings first";
pub const K_MODEL_NOT_DOWNLOADED: &str =
    "Model \"%1\" is not downloaded — download it in Settings";
pub const K_STT_RECOGNIZING: &str = "Recognizing…";
pub const K_STT_RUN_FAILED: &str = "Failed to launch STT: %1";
pub const K_STT_RESULT_FAILED: &str = "Speech recognition failed (see log)";
pub const K_STT_EMPTY: &str = "Nothing recognized";
pub const K_TTS_NO_MODEL_SELECTED: &str = "Select a TTS model in settings first";
pub const K_TTS_PIPER_MISSING: &str = "piper is not installed (%1)";
pub const K_TTS_RUN_FAILED: &str = "Failed to launch TTS: %1";
pub const K_TTS_RESULT_FAILED: &str = "Speech synthesis failed (see log)";
pub const K_MODEL_DOWNLOADING: &str = "Downloading \"%1\"…";
pub const K_STT_MODEL_SELECTED: &str = "STT model \"%1\" selected";
pub const K_STT_MODELS_NONE: &str = "No STT models — download a new one";
pub const K_TTS_MODEL_SELECTED: &str = "TTS model \"%1\" selected";
pub const K_TTS_MODELS_NONE: &str = "No TTS models — download a new one";
pub const K_STT_RECORD_FINISHED: &str = "Recording finished";
pub const K_CATALOG_REFRESHED: &str = "Voice model catalog updated: %1 models available";
pub const K_CATALOG_REFRESH_FAILED: &str = "Failed to update the model catalog: %1";
pub const K_AUTH_SAVED: &str = "OpenCode API key saved";
pub const K_AUTH_EMPTY: &str = "Enter the API key first";
pub const K_AUTH_SAVE_FAILED: &str = "Failed to save the OpenCode API key: %1";
pub const K_AUTH_SIGNED_IN: &str = "Signed in to OpenCode";
pub const K_AUTH_SIGNIN_FAILED: &str = "OpenCode sign-in failed: %1";
pub const K_COMPACTING: &str = "Compacting history…";
pub const K_COMPACTED: &str = "Context compacted";
pub const K_COMPACTION_UNAVAILABLE: &str = "Nothing to compact: %1";

/// Current time in unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn i18n_line(ts: u64, open: &str, key: &str, args: &[String]) -> String {
    let mut body = format!("{open}{key}");
    for a in args {
        body.push(ARG_SEP);
        body.push_str(a);
    }
    proto_line(ts, &format!("{body}{TAG_CLOSE}"))
}

/// Service notice line: `[[t:<ms>]][[s:KEY\u001fARG…]]`. QML translates it.
pub fn svc_line(ts: u64, key: &str, args: &[String]) -> String {
    i18n_line(ts, SVC_OPEN, key, args)
}

/// Error line: `[[t:<ms>]][[e:KEY\u001fARG…]]`. QML translates, notifies and
/// renders a red bubble.
pub fn err_line(ts: u64, key: &str, args: &[String]) -> String {
    i18n_line(ts, ERR_OPEN, key, args)
}