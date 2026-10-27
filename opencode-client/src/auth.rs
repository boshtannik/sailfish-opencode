//! Provider credentials that the user can enter in-app.
//!
//! The OpenCode provider ("big-pickle" models) is authenticated either with a
//! plain API key (see [`has_api_key`]) or with the OpenCode Console account
//! (device login, see [`ConsoleLogin`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::api::OpenCodeApi;
use crate::i18n::now_ms;
use crate::types::v2::integration::{Attempt, AttemptStatus};

/// Provider id of the OpenCode (Zen) provider.
pub const OPENCODE_PROVIDER: &str = "opencode";

/// Browser page where the user creates / copies their OpenCode API key.
pub const OPENCODE_AUTH_URL: &str = "https://opencode.ai/auth";

/// Integration id of the Console account (`GET /api/integration` → "OpenCode
/// Console"), and the id of its OAuth method (the device flow).
const CONSOLE_INTEGRATION: &str = "opencode";
const CONSOLE_METHOD: &str = "device";

/// How often the attempt is polled. The server polls the provider itself (the
/// method is `mode: "auto"`), so this only reads the local status — 1.5 s keeps
/// the "signed in" feedback prompt without hammering the daemon.
const POLL_MS: u64 = 1_500;

/// Path to opencode's credential store (`$XDG_DATA_HOME/opencode/auth.json`,
/// defaulting to `~/.local/share/opencode/auth.json`).
pub fn auth_json_path() -> Option<std::path::PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))?;
    Some(data_home.join("opencode").join("auth.json"))
}

/// Whether an API-key credential is stored for `provider`.
pub fn has_api_key(provider: &str) -> bool {
    let Some(path) = auth_json_path() else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    json.get(provider)
        .and_then(|v| v.get("key"))
        .and_then(|k| k.as_str())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

// ── OpenCode Console account login ────────────────────────────────────────────
//
// opencode 2.0.32 exposes the Console device login as HTTP, not as a CLI
// subcommand (`opencode console login` is gone — `auth`/`api` replaced it, see
// `vault/Разведка-v2.md`). The client therefore drives the integration API:
//
//   POST   /api/integration/opencode/connect/oauth {methodID:"device"}
//     → {attemptID, url, instructions, mode:"auto", time:{created, expires}}
//   GET    /api/integration/opencode/connect/oauth/{attemptID}
//     → {status:"pending"|"complete"|"failed"|"expired", …}
//   POST   /api/integration/opencode/connect/oauth/{attemptID}/complete
//     → 204, the credential is persisted by the server
//   DELETE …/{attemptID} → 204 when the user cancels
//
// The state machine is advanced by the worker's tick (`begin` once, then
// `poll`), so no thread and no blocking call sit in the UI path, and every
// transition is driven by the server's own answer.

/// Visual state of a device login, serialized to QML as JSON.
#[derive(Clone, Default, serde::Serialize)]
pub struct LoginStatus {
    /// "idle" | "waiting" | "success" | "error"
    pub state: String,
    /// Verification URL the user must open.
    pub url: String,
    /// The user code to enter there.
    pub code: String,
    /// Human-readable detail (failure reason), empty otherwise.
    pub message: String,
}

impl LoginStatus {
    pub fn is_waiting(&self) -> bool {
        self.state == "waiting"
    }
}

/// Owns the device-login state shared between the QML bridge and the worker.
///
/// Cheap to clone (the state is behind `Arc`); `begin`/`poll` are called by the
/// worker (which holds the API client), `status_json` by QML via the bridge.
#[derive(Clone, Default)]
pub struct ConsoleLogin {
    status: Arc<Mutex<LoginStatus>>,
    /// The live attempt, while one is pending.
    attempt: Arc<Mutex<Option<Attempt>>>,
    /// Epoch ms of the last status poll (throttles `poll`).
    last_poll: Arc<Mutex<u64>>,
    /// Set by `cancel`; the next `poll` drops the attempt server-side.
    cancelled: Arc<AtomicBool>,
    /// Set to `Some(success)` when the login ends; taken once by the worker.
    finished: Arc<Mutex<Option<bool>>>,
}

impl ConsoleLogin {
    /// Current state as JSON (`LoginStatus`), read by QML on every poll.
    pub fn status_json(&self) -> String {
        let st = self.status.lock().map(|g| g.clone()).unwrap_or_default();
        serde_json::to_string(&st).unwrap_or_else(|_| "{\"state\":\"error\"}".to_string())
    }

    /// Last detail message (for the failure notice).
    pub fn message(&self) -> String {
        self.status.lock().map(|g| g.message.clone()).unwrap_or_default()
    }

    /// Take the finish flag once (`Some(true)` = success, `Some(false)` = error).
    pub fn take_finished(&self) -> Option<bool> {
        self.finished
            .lock()
            .map(|mut f| f.take())
            .unwrap_or(None)
    }

    fn set(&self, state: &str, message: &str) {
        if let Ok(mut g) = self.status.lock() {
            g.state = state.to_string();
            g.message = message.to_string();
        }
    }

    fn finish(&self, ok: bool, state: &str, message: &str) {
        self.set(state, message);
        if let Ok(mut a) = self.attempt.lock() {
            *a = None;
        }
        if let Ok(mut f) = self.finished.lock() {
            *f = Some(ok);
        }
    }

    /// Start the device login: ask the server for an attempt and publish the
    /// verification URL/code. `Err` if one is already running or the server
    /// refuses (`GET /api/integration` missing → too old a server).
    pub async fn begin<A: OpenCodeApi>(&self, client: &A) -> Result<(), String> {
        if self.status.lock().map(|g| g.is_waiting()).unwrap_or(false) {
            return Err("login already in progress".to_string());
        }
        self.cancelled.store(false, Ordering::Relaxed);
        if let Ok(mut f) = self.finished.lock() {
            *f = None;
        }
        if let Ok(mut a) = self.attempt.lock() {
            *a = None;
        }
        // Show the spinner immediately: the connect call takes a round trip.
        self.set("waiting", "");

        match client
            .integration_oauth_connect(CONSOLE_INTEGRATION, CONSOLE_METHOD)
            .await
        {
            Ok(attempt) => {
                if let Ok(mut g) = self.status.lock() {
                    g.state = "waiting".to_string();
                    g.message.clear();
                    g.url = attempt.url.clone();
                    g.code = attempt.code();
                }
                if let Ok(mut g) = self.last_poll.lock() {
                    *g = now_ms();
                }
                if let Ok(mut g) = self.attempt.lock() {
                    *g = Some(attempt);
                }
                Ok(())
            }
            Err(e) => {
                self.finish(false, "error", &e);
                Err(e)
            }
        }
    }

    /// Advance a pending login: one throttled status poll.
    ///
    /// Reads the server's own answer — `pending` keeps waiting, `complete`
    /// persists the credential, `failed`/`expired` end with an error. Called on
    /// every worker tick; a no-op when nothing is pending.
    pub async fn poll<A: OpenCodeApi>(&self, client: &A) {
        let Some(attempt) = self.attempt.lock().ok().and_then(|g| g.clone()) else {
            return;
        };
        if self.cancelled.load(Ordering::Relaxed) {
            if let Err(e) = client
                .integration_oauth_cancel(CONSOLE_INTEGRATION, &attempt.attempt_id)
                .await
            {
                log::warn!("console login: cancel failed: {e}");
            }
            self.set("idle", "");
            if let Ok(mut a) = self.attempt.lock() {
                *a = None;
            }
            return;
        }

        let now = now_ms();
        {
            let Ok(mut last) = self.last_poll.lock() else { return };
            if now < last.saturating_add(POLL_MS) {
                return;
            }
            *last = now;
        }

        // Local deadline first: an expired code needs no round trip, and the
        // server may keep answering `pending` for a while after it died.
        if let Some(expires) = attempt.time.expires {
            if expires > 0.0 && now as f64 > expires {
                self.finish(false, "error", "sign-in code expired — start again");
                return;
            }
        }

        match client
            .integration_oauth_status(CONSOLE_INTEGRATION, &attempt.attempt_id)
            .await
        {
            Ok(AttemptStatus::Pending { .. }) => {}
            Ok(AttemptStatus::Complete { .. }) => {
                // The attempt is authorized; this is what persists the
                // credential on the server. A failure here is only logged: for
                // `mode: "auto"` the server has usually stored it already, and
                // the model refresh after the login shows the truth.
                if let Err(e) = client
                    .integration_oauth_complete(CONSOLE_INTEGRATION, &attempt.attempt_id)
                    .await
                {
                    log::warn!("console login: complete failed: {e}");
                }
                self.finish(true, "success", "");
            }
            Ok(AttemptStatus::Failed { message, .. }) => {
                self.finish(false, "error", &message);
            }
            Ok(AttemptStatus::Expired { .. }) => {
                self.finish(false, "error", "sign-in code expired — start again");
            }
            Err(e) => {
                // A transient poll failure (daemon restart, timeout) must not
                // kill the login: keep the attempt and retry on the next tick.
                log::warn!("console login: status: {e}");
            }
        }
    }

    /// Cancel a pending login (user pressed Cancel). The attempt is dropped
    /// server-side by the next `poll`.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if self.status.lock().map(|g| g.is_waiting()).unwrap_or(false) {
            self.set("idle", "");
        }
    }
}

/// The `mode` of the Console attempt must stay `auto`: the client never asks
/// the user to paste a code back (`code` mode integrations would need it).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_resets_a_waiting_login() {
        let l = ConsoleLogin::default();
        {
            let mut g = l.status.lock().unwrap();
            g.state = "waiting".to_string();
        }
        l.cancel();
        assert_eq!(l.status.lock().unwrap().state, "idle");
        assert!(l.cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn status_json_carries_the_ui_fields() {
        let l = ConsoleLogin::default();
        {
            let mut g = l.status.lock().unwrap();
            g.state = "waiting".to_string();
            g.url = "https://example.org/device?user_code=AB".to_string();
            g.code = "AB".to_string();
        }
        let v: serde_json::Value = serde_json::from_str(&l.status_json()).unwrap();
        assert_eq!(v["state"], "waiting");
        assert_eq!(v["code"], "AB");
        assert_eq!(v["url"], "https://example.org/device?user_code=AB");
    }
}
