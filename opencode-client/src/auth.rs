//! Provider credentials that the user can enter in-app.
//!
//! The OpenCode provider ("big-pickle" models) is authenticated either with a
//! plain API key (see [`has_api_key`]) or with the OpenCode Console account
//! (device login, see [`ConsoleLogin`]). Other providers, such as Anthropic
//! (Claude), take a plain API key.

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

/// Provider id of Anthropic (Claude models), stored as a plain API key.
pub const ANTHROPIC_PROVIDER: &str = "anthropic";

/// Browser page where the user creates an Anthropic API key.
pub const ANTHROPIC_KEYS_URL: &str = "https://platform.claude.com/settings/keys";

/// Providers with an active credential on the server, as last reported by
/// `GET /api/credential`. opencode v2 keeps credentials in its own database
/// (the v1 `auth.json` is no longer written), so the worker refreshes this
/// cache from the server and QML reads it synchronously.
static CONNECTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Replace the cached set of connected providers.
pub fn set_connected(providers: Vec<String>) {
    if let Ok(mut g) = CONNECTED.lock() {
        *g = providers;
    }
}

/// Whether a credential (API key or account sign-in) is stored for `provider`.
pub fn has_api_key(provider: &str) -> bool {
    CONNECTED
        .lock()
        .map(|g| g.iter().any(|p| p == provider))
        .unwrap_or(false)
}

// ── Rejected keys ─────────────────────────────────────────────────────────────
//
// The server stores a key without checking it. When a provider later answers
// a turn with `provider.auth` (HTTP 401, e.g. "invalid x-api-key"), the key is
// marked rejected here: its models are listed as locked again and Settings
// asks for a new key. Saving a key clears the mark.

static REJECTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Set when the locked/unlocked state changed and the model catalog must be
/// rebuilt; the worker polls it with [`take_models_dirty`].
static MODELS_DIRTY: AtomicBool = AtomicBool::new(false);

/// Mark `provider`'s stored key as rejected by the provider.
pub fn mark_rejected(provider: &str) {
    if provider.is_empty() {
        return;
    }
    if let Ok(mut g) = REJECTED.lock() {
        if !g.iter().any(|p| p == provider) {
            log::warn!("auth: the {provider} API key was rejected; locking its models");
            g.push(provider.to_string());
            MODELS_DIRTY.store(true, Ordering::SeqCst);
        }
    }
}

/// Forget a rejection (a new key was saved for `provider`).
pub fn clear_rejected(provider: &str) {
    if let Ok(mut g) = REJECTED.lock() {
        let before = g.len();
        g.retain(|p| p != provider);
        if g.len() != before {
            MODELS_DIRTY.store(true, Ordering::SeqCst);
        }
    }
}

/// Whether the stored key of `provider` was rejected by the provider.
pub fn is_rejected(provider: &str) -> bool {
    REJECTED
        .lock()
        .map(|g| g.iter().any(|p| p == provider))
        .unwrap_or(false)
}

/// True once after the locked state changed (see [`MODELS_DIRTY`]).
pub fn take_models_dirty() -> bool {
    MODELS_DIRTY.swap(false, Ordering::SeqCst)
}

// ── Key validation (Anthropic) ────────────────────────────────────────────────

/// Anthropic endpoint used to check a key before it is stored: listing models
/// is free and needs only a valid key.
pub const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const ANTHROPIC_KEY_HEADER: &str = "x-api-key";
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const KEY_CHECK_TIMEOUT_SECS: u64 = 12;

/// Outcome of checking a key with its provider before saving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCheck {
    /// The provider accepted the key.
    Valid,
    /// The provider refused the key (HTTP 401/403); it must not be stored.
    Rejected(String),
    /// The provider could not be asked (offline, timeout, 5xx…). The key is
    /// stored anyway: refusing it would lock out a user who is just offline.
    Unverified(String),
}

/// Classify an HTTP status of the key check.
pub fn key_check_from_status(status: u16) -> KeyCheck {
    match status {
        200..=299 => KeyCheck::Valid,
        401 | 403 => KeyCheck::Rejected(format!("HTTP {status}")),
        other => KeyCheck::Unverified(format!("HTTP {other}")),
    }
}

/// Ask Anthropic whether `key` is valid.
pub async fn check_anthropic_key(key: &str) -> KeyCheck {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(KEY_CHECK_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => return KeyCheck::Unverified(e.to_string()),
    };
    match client
        .get(ANTHROPIC_MODELS_URL)
        .header(ANTHROPIC_KEY_HEADER, key)
        .header(ANTHROPIC_VERSION_HEADER, ANTHROPIC_VERSION)
        .send()
        .await
    {
        Ok(resp) => key_check_from_status(resp.status().as_u16()),
        Err(e) => KeyCheck::Unverified(e.to_string()),
    }
}

// ── Locked catalog ────────────────────────────────────────────────────────────
//
// Providers the app can unlock in-app (Settings → API key). Without a key the
// server lists none of their models, so the picker reads them from opencode's
// own copy of the models.dev catalog (`$XDG_CACHE_HOME/opencode/models.json`,
// `{provider: {models: {id: {name}}}}`) to show them greyed out.

/// Providers whose models are shown locked until a key is saved.
pub const LOCKABLE_PROVIDERS: &[&str] = &[ANTHROPIC_PROVIDER];

const CATALOG_MODELS: &str = "models";
const CATALOG_NAME: &str = "name";

/// Path to opencode's cached models.dev catalog.
pub fn models_catalog_path() -> Option<std::path::PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))?;
    Some(cache.join("opencode").join("models.json"))
}

/// `(model id, display name)` of `provider` from a models.dev catalog value,
/// sorted by id. Empty when the provider or its models are missing.
pub fn catalog_models(catalog: &serde_json::Value, provider: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = catalog
        .get(provider)
        .and_then(|p| p.get(CATALOG_MODELS))
        .and_then(|m| m.as_object())
        .map(|models| {
            models
                .iter()
                .map(|(id, m)| {
                    let name = m.get(CATALOG_NAME).and_then(|n| n.as_str()).unwrap_or(id);
                    (id.clone(), name.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Read the cached catalog; `None` when it is missing or unreadable (the
/// picker then shows the provider with an "add a key" hint and no models).
pub fn read_models_catalog() -> Option<serde_json::Value> {
    let path = models_catalog_path()?;
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("auth: no models catalog at {path:?}: {e}");
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("auth: models catalog {path:?} is not valid JSON: {e}");
            None
        }
    }
}

/// Ask the server which providers are connected and update the cache.
pub async fn refresh_connected(client: &impl OpenCodeApi) {
    match client.connected_providers().await {
        Ok(list) => {
            log::info!("auth: connected providers: {list:?}");
            set_connected(list);
        }
        Err(e) => log::warn!("auth: failed to list credentials: {e}"),
    }
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
        self.status
            .lock()
            .map(|g| g.message.clone())
            .unwrap_or_default()
    }

    /// Take the finish flag once (`Some(true)` = success, `Some(false)` = error).
    pub fn take_finished(&self) -> Option<bool> {
        self.finished.lock().map(|mut f| f.take()).unwrap_or(None)
    }

    fn set(&self, state: &str, message: &str) {
        if let Ok(mut g) = self.status.lock() {
            g.state = state.to_string();
            g.message = message.to_string();
        }
    }

    // FIXME: State shall be enum with str values. Compiler shall help us to restict behaviour of
    // variety of data.
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
            let Ok(mut last) = self.last_poll.lock() else {
                return;
            };
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

    /// Only 401/403 refuse a key; anything else (rate limit, outage) must not
    /// lock a user out.
    #[test]
    fn key_check_classifies_status() {
        assert_eq!(key_check_from_status(200), KeyCheck::Valid);
        assert_eq!(key_check_from_status(401), KeyCheck::Rejected("HTTP 401".into()));
        assert_eq!(key_check_from_status(403), KeyCheck::Rejected("HTTP 403".into()));
        assert_eq!(key_check_from_status(429), KeyCheck::Unverified("HTTP 429".into()));
        assert_eq!(key_check_from_status(529), KeyCheck::Unverified("HTTP 529".into()));
    }

    /// models.dev shape (`{provider:{models:{id:{name}}}}`), sorted by id;
    /// a missing name falls back to the id, a missing provider gives nothing.
    #[test]
    fn catalog_models_reads_provider() {
        let v = serde_json::json!({
            "anthropic": { "id": "anthropic", "models": {
                "claude-opus-5-5": { "name": "Claude Opus 5.5" },
                "claude-haiku-5-5": { "name": "Claude Haiku 5.5" },
                "claude-x": {}
            } }
        });
        assert_eq!(
            catalog_models(&v, ANTHROPIC_PROVIDER),
            vec![
                ("claude-haiku-5-5".to_string(), "Claude Haiku 5.5".to_string()),
                ("claude-opus-5-5".to_string(), "Claude Opus 5.5".to_string()),
                ("claude-x".to_string(), "claude-x".to_string()),
            ]
        );
        assert!(catalog_models(&v, "openai").is_empty());
    }

    #[test]
    fn rejection_marks_and_clears() {
        let p = "test-provider-rejection";
        assert!(!is_rejected(p));
        mark_rejected(p);
        assert!(is_rejected(p));
        assert!(take_models_dirty());
        clear_rejected(p);
        assert!(!is_rejected(p));
    }
}
