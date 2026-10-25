use std::cell::RefCell;
use std::ffi::CStr;
use std::io::Write as _;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use futures_util::StreamExt;
use qmetaobject::{qml_register_type, QObjectPinned};
use sailors::sailfishapp::QmlApp;
use tokio::sync::Mutex;

use opencode_client::api;
use opencode_client::bridge::app::AppBridge;
use opencode_client::cmd::Cmd;
use opencode_client::event;
use opencode_client::i18n::{self, err_line, now_ms, svc_line};
use opencode_client::markers::*;
use opencode_client::server;
use opencode_client::settings::Settings;
use opencode_client::types::v2::permission::PermissionReplyKind;
use opencode_client::types::keys;
use opencode_client::types::tool::ToolState;
use opencode_client::types::v2::keys as v2keys;
use opencode_client::voice;

/// The only QML file of the application (located in /usr/share/harbour-opencode/qml).
const MAIN_QML: &str = "qml/harbour-opencode.qml";
/// Application name: determines both the data directory `SailfishApp::pathTo` and argv[0].
const APP_NAME: &str = "harbour-opencode";

/// Logger writes both to stderr and to `~/.cache/harbour-opencode/app.log`
/// (when running via booster, stdout is inherited and lost).
struct Tee(std::fs::File);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        self.0.flush()
    }
}

/// Initialize logging: `RUST_LOG` (default `warn`) + file in the application cache.
fn init_logging() {
    // Default `warn`: `info` logged every SSE event (~1 s) and grew app.log
    // tens of MB. Set `RUST_LOG=info` / `debug` for verbose troubleshooting.
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn"),
    );
    if let Some(home) = std::env::var_os("HOME") {
        let dir = std::path::PathBuf::from(home).join(".cache/harbour-opencode");
        let _ = std::fs::create_dir_all(&dir);
        install_panic_log(&dir);
        let path = dir.join("app.log");
        // Rotate: keep the previous run, start a fresh file (bounds growth).
        if path.exists() {
            let _ = std::fs::rename(&path, dir.join("app.log.1"));
        }
        if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            builder.target(env_logger::Target::Pipe(Box::new(Tee(file))));
        }
    }
    let _ = builder.try_init();
    log::info!("--- harbour-opencode starting (pid={}) ---", std::process::id());
}

/// Routed to the file — the launcher does not capture the app's stderr, so a
/// panic from the icon launch would otherwise be lost. Writes the payload and
/// a symbolicated backtrace to `panic.log` in the app cache.
fn install_panic_log(dir: &std::path::Path) {
    let path = dir.join("panic.log");
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("\n[{} PANIC] thread={:?}\n{}\n{}",
                          humantime_since_epoch(), std::thread::current().id(), info, bt);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(msg.as_bytes());
            let _ = f.flush();
        }
        // If the app was started from a terminal, keep the visible output too.
        let _ = std::io::stderr().write_all(msg.as_bytes());
    }));
}

fn humantime_since_epoch() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}s", d.as_secs()),
        Err(e) => format!("{e:?}"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    // Ensure opencode asks for permissions by default
    if let Some(home) = std::env::var_os("HOME") {
        let cfg_dir = std::path::Path::new(&home).join(".config").join("opencode");
        let cfg_path = cfg_dir.join("opencode.jsonc");
        if !cfg_path.exists() {
            let _ = std::fs::create_dir_all(&cfg_dir);
            let _ = std::fs::write(&cfg_path, "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"permission\": {\n    \"*\": \"ask\"\n  }\n}\n");
        }
    }
    // Drop leftover temp files from a previous run (old /tmp WAVs, half-written
    // TTS cache entries) so they don't accumulate.
    voice::cleanup_tmp();

    // QML ←→ Rust bridge (created before Qt to pass queues to the worker).
    qml_register_type::<AppBridge>(
        CStr::from_bytes_with_nul(b"OpenCodeClient\0")?,
        1,
        0,
        CStr::from_bytes_with_nul(b"AppBridge\0")?,
    );
    let bridge = RefCell::new(AppBridge::default());
    let bridge_pinned = unsafe { QObjectPinned::new(&bridge) };
    let (pending, prompts) = bridge_pinned.borrow().queues();
    let status = bridge_pinned.borrow().status_handle();
    let sessions = bridge_pinned.borrow().sessions_handle();
    let commands = bridge_pinned.borrow().commands_handle();
    let current_session = bridge_pinned.borrow().current_session_handle();
    let compacting = bridge_pinned.borrow().compacting_handle();
    let nav = bridge_pinned.borrow().nav_handle();
    if let Ok(mut n) = nav.lock() {
        *n = -1;
    }
    let todos = bridge_pinned.borrow().todo_handle();
    let models = bridge_pinned.borrow().models_handle();
    let sounds = bridge_pinned.borrow().sounds_handle();
    let voice = voice::VoiceState::with_status(bridge_pinned.borrow().voice_status_handle());
    let tools = bridge_pinned.borrow().tools_handle();
    let permissions = bridge_pinned.borrow().permissions_handle();
    let permission_error = bridge_pinned.borrow().permission_error_handle();
    let saved_permissions = bridge_pinned.borrow().saved_permissions_handle();
    let forms = bridge_pinned.borrow().forms_handle();
    let settings = bridge_pinned.borrow().settings_handle();
    let console_login = bridge_pinned.borrow().console_login_handle();
    bridge_pinned.borrow().set_status_shared(markers::STATUS_CONNECTING);

    let dispatcher: event::dispatcher::SharedState = Arc::new(Mutex::new(Default::default()));

    let (url_tx, _url_rx) = mpsc::channel::<Option<String>>();
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let env_base = std::env::var("OPENCODE_SERVER_URL").ok();
    let env_auth = std::env::var("OPENCODE_SERVER_PASSWORD").ok().map(|pw| {
        let user =
            std::env::var("OPENCODE_SERVER_USERNAME").unwrap_or_else(|_| "opencode".to_string());
        (user, pw)
    });

    // Async stack (SSE, HTTP, model downloads) runs in a separate std thread:
    // QML must stay on the main thread; the tokio runtime runs off the main thread.
    let worker_status = status.clone();
    let worker_sessions = sessions.clone();
    let worker_commands = commands.clone();
    let worker_current = current_session.clone();
    let worker_compacting = compacting.clone();
    let worker_nav = nav.clone();
    let worker_todos = todos.clone();
    let worker_models = models.clone();
    let worker_sounds = sounds.clone();
    let worker_voice = voice.clone();
    let worker_tools = tools.clone();
    let worker_permissions = permissions.clone();
    let worker_permission_error = permission_error.clone();
    let worker_saved_permissions = saved_permissions.clone();
    let worker_forms = forms.clone();
    let worker_url_tx = url_tx.clone();
    let worker_dispatcher = dispatcher.clone();
    let worker_base = env_base.clone();
    let worker_auth = env_auth.clone();
    let worker_stop_rx = stop_rx;
    let worker_settings = settings.clone();
    let worker = std::thread::Builder::new()
        .name("opencode-worker".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                let st = Settings::load();
                let cwd = std::path::PathBuf::from(&st.workdir);
                if let Err(e) = std::fs::create_dir_all(&cwd) {
                    log::warn!("failed to create workdir {cwd:?}: {e}");
                }
                let guard = server::ServerGuard::start(server::ServerConfig { cwd: Some(cwd), ..Default::default() }).await;
                // Auth for the connection the worker will use. Our own server
                // (2.0.32) prints a Basic-auth password on startup → take it
                // from the guard; the env auth only matters for a pre-existing
                // external server (`base` from the environment).
                let mut worker_auth = worker_auth;
                let base = match &guard {
                    Ok(g) => {
                        log::info!("opencode serve started: {}", g.base_url());
                        if worker_base.is_none() {
                            worker_auth = g.auth.clone();
                        }
                        Some(worker_base.unwrap_or_else(|| g.base_url().to_string()))
                    }
                    Err(e) => {
                        log::error!("failed to start opencode serve: {e}");
                        worker_base
                    }
                };
                let _ = worker_url_tx.send(base.clone());
                match base {
                    Some(base) => {
                        run_worker(
                            base,
                            worker_auth,
                            worker_dispatcher,
                            pending,
                            prompts,
                            worker_status,
                            worker_sessions,
                            worker_commands,
                            worker_current,
                            worker_compacting,
                            worker_nav,
                            worker_todos,
                            worker_models,
                            worker_sounds,
                            worker_voice,
                            worker_tools,
                        worker_permissions,
                        worker_permission_error,
                        worker_saved_permissions,
                        worker_forms,
                        worker_settings,
                        console_login,
                        worker_stop_rx,
                    )
                    .await
                    }
                    None => set_server_status(&worker_status, &markers::ServerConnectionStatus::Error),
                }
                // Guard drop happens here: after the stop signal.
                drop(guard);
            });
        })
        .expect("spawn worker");

    // The window is shown immediately: the worker sets the live status
    // ("connecting" → "idle"/"error") once `opencode serve` is ready, so a slow
    // cold start no longer blocks the UI or flashes a false "error" badge.

    // QML runs on the main thread; exec() blocks until the window is closed.
    let mut app = QmlApp::application(APP_NAME.to_string());
    // Native Qt i18n: load translation (harbour-opencode-ru.qm, etc.) according to system locale.
    match app.install_default_translator() {
        Ok(()) => log::info!("translation loaded"),
        Err(e) => log::warn!("translation not loaded: {e}"),
    }
    app.set_object_property("bridge".into(), bridge_pinned);
    app.set_source(QmlApp::path_to(MAIN_QML.into()));
    app.show_full_screen();
    app.exec();

    log::info!("QML closed, exiting");

    // Give the worker time to exit the SSE loop and drop ServerGuard
    // (to kill serve), otherwise the process will exit without Drop and serve may be orphaned.
    let _ = stop_tx.send(true);
    let _ = worker.join();
    log::info!("worker stopped, opencode serve released");
    Ok(())
}

/// Background worker: (1) SSE events → QML bridge, (2) prompts from QML → server.
/// Why the SSE loop stopped. Drives the worker's decision: reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamEnd {
    /// The server closed the stream (or the session was dropped) — reconnect.
    Reconnect,
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    base: String,
    auth: Option<(String, String)>,
    dispatcher: event::dispatcher::SharedState,
    worker_pending: Arc<std::sync::Mutex<Vec<String>>>,
    worker_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    status: Arc<std::sync::Mutex<String>>,
    sessions: Arc<std::sync::Mutex<String>>,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
    current_session: Arc<std::sync::Mutex<String>>,
    compacting: Arc<std::sync::Mutex<bool>>,
    nav: Arc<std::sync::Mutex<i32>>,
    todos: Arc<std::sync::Mutex<String>>,
    models: Arc<std::sync::Mutex<String>>,
    sounds: Arc<std::sync::Mutex<String>>,
    voice: Arc<voice::VoiceState>,
    tools: Arc<std::sync::Mutex<String>>,
    permissions: Arc<std::sync::Mutex<String>>,
    permission_error: Arc<std::sync::Mutex<String>>,
    saved_permissions: Arc<std::sync::Mutex<String>>,
    forms: Arc<std::sync::Mutex<String>>,
    settings: Arc<std::sync::Mutex<String>>,
    console_login: opencode_client::auth::ConsoleLogin,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        log::info!("stream adapter: v2");
        tokio::select! {
            _ = stop_rx.changed() => {
                log::info!("stop signal received, exiting");
                return;
            }
            r = run_stream_adapter(
                base.clone(),
                auth.clone(),
                dispatcher.clone(),
                worker_pending.clone(),
                worker_prompts.clone(),
                status.clone(),
                sessions.clone(),
                commands.clone(),
                current_session.clone(),
                compacting.clone(),
                nav.clone(),
                todos.clone(),
                models.clone(),
                sounds.clone(),
                voice.clone(),
                tools.clone(),
                permissions.clone(),
                permission_error.clone(),
                saved_permissions.clone(),
                forms.clone(),
                settings.clone(),
                console_login.clone(),
            ) => {
                match r {
                    Ok(StreamEnd::Reconnect) => log::warn!("SSE stream ended, reconnecting…"),
                    Err(e) => log::error!("SSE error: {e}, reconnecting…"),
                }
                set_server_status(&status, &markers::ServerConnectionStatus::Connecting);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

/// Run one SSE session with the v2 adapter.
#[allow(clippy::too_many_arguments)]
async fn run_stream_adapter(
    base: String,
    auth: Option<(String, String)>,
    dispatcher: event::dispatcher::SharedState,
    worker_pending: Arc<std::sync::Mutex<Vec<String>>>,
    worker_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    status: Arc<std::sync::Mutex<String>>,
    sessions: Arc<std::sync::Mutex<String>>,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
    current_session: Arc<std::sync::Mutex<String>>,
    compacting: Arc<std::sync::Mutex<bool>>,
    nav: Arc<std::sync::Mutex<i32>>,
    todos: Arc<std::sync::Mutex<String>>,
    models: Arc<std::sync::Mutex<String>>,
    sounds: Arc<std::sync::Mutex<String>>,
    voice: Arc<voice::VoiceState>,
    tools: Arc<std::sync::Mutex<String>>,
    permissions: Arc<std::sync::Mutex<String>>,
    permission_error: Arc<std::sync::Mutex<String>>,
    saved_permissions: Arc<std::sync::Mutex<String>>,
    forms: Arc<std::sync::Mutex<String>>,
    settings: Arc<std::sync::Mutex<String>>,
    console_login: opencode_client::auth::ConsoleLogin,
) -> Result<StreamEnd, String> {
    let client = api::V2Adapter::new(base.clone(), auth.clone());
    run_stream(
        client, dispatcher, worker_pending, worker_prompts, status, sessions,
        commands, current_session, compacting, nav, todos, models, sounds, voice, tools,
        permissions, permission_error, saved_permissions, forms, settings, console_login,
    )
    .await
}

/// Normalised retry/busy/idle state, from the v2 `session.status` event — so
/// the main loop formats the status banner once.
enum RetryStatus {
    Idle,
    Busy,
    Retry {
        attempt: u32,
        message: String,
        next: u64,
    },
}

/// Extract the retry/busy/idle state from a `session.status` event.
fn retry_status(env: &opencode_client::types::v2::EventEnvelope) -> Option<(String, RetryStatus)> {
    use opencode_client::types::v2::event::SessionStatusValue;
    if env.kind != opencode_client::types::v2::keys::STATUS {
        return None;
    }
    let d: opencode_client::types::v2::event::SessionStatusData =
        serde_json::from_value(env.data.clone()).ok()?;
    let st = match d.status {
        SessionStatusValue::Retry { attempt, message, next } => {
            RetryStatus::Retry { attempt, message, next }
        }
        SessionStatusValue::Busy => RetryStatus::Busy,
        SessionStatusValue::Idle => RetryStatus::Idle,
        SessionStatusValue::Unknown => return None,
    };
    Some((d.sessionID, st))
}

/// Per-turn streaming accumulators plus the shared IO handles a terminal
/// streaming event needs. Bundling them keeps the v2 terminal handlers
/// (`on_v2_event`/`on_v2_idle`) signature small.
struct TurnState<'a> {
    parts: &'a mut std::collections::HashMap<String, String>,
    text_order: &'a mut Vec<String>,
    text_live: &'a mut std::collections::HashMap<String, String>,
    spoken_parts: &'a mut std::collections::HashSet<String>,
    assistant_messages: &'a std::collections::HashSet<String>,
    compaction_messages: &'a std::collections::HashSet<String>,
    reasonings: &'a mut std::collections::HashMap<String, String>,
    reasoning_order: &'a mut Vec<String>,
    reasoning_part_ids: &'a mut std::collections::HashSet<String>,
    live_reasoned_ids: &'a mut std::collections::HashSet<String>,
    last_reasoning_push: &'a mut u64,
    last_answer_push: &'a mut u64,
    current_session: Option<&'a str>,
    pending: &'a Arc<std::sync::Mutex<Vec<String>>>,
    status: &'a Arc<std::sync::Mutex<String>>,
    voice: &'a Arc<voice::VoiceState>,
}

impl<'a> TurnState<'a> {
    /// v2 terminal streaming events: `session.text.ended` / `session.reasoning.ended`
    /// carry the final text of one block (v1 delivered the same at
    /// `message.part.updated`; hence the shared accumulator semantics).
    async fn on_v2_event(&mut self, env: &opencode_client::types::v2::EventEnvelope, _client: &impl api::OpenCodeApi) {
        use opencode_client::types::v2::event as v2;
        match env.kind.as_str() {
            "session.text.ended" => {
                let Ok(d): Result<v2::TextEndedData, _> = serde_json::from_value(env.data.clone())
                else {
                    return;
                };
                if !self.assistant_messages.contains(&d.assistant_message_id) {
                    return;
                }
                let key = v2_text_key(&d.assistant_message_id, d.ordinal);
                if self.compaction_messages.contains(&d.assistant_message_id) {
                    self.spoken_parts.insert(key.clone());
                }
                let was_live = self.text_live.contains_key(&key);
                if self.parts.insert(key.clone(), d.text.clone()).is_none() {
                    self.text_order.push(key.clone());
                }
                if was_live && !d.text.trim().is_empty() {
                    if let Ok(mut q) = self.pending.lock() {
                        q.push(proto_live_line(
                            now_ms(),
                            ANSWER_LIVE_TAG,
                            d.text.trim_end(),
                            &key,
                        ));
                    }
                    let tts_mode = self.voice.tts_mode.lock().map(|m| *m).unwrap_or_default();
                    if tts_mode == voice::TtsMode::Auto
                        && self.current_session == Some(d.session_id.as_str())
                        && !self.voice.dictating()
                        && self.spoken_parts.insert(key.clone())
                    {
                        voice::tts_from_call(self.voice, d.text.trim_end(), self.pending).await;
                    }
                }
            }
            "session.reasoning.ended" => {
                let Ok(d): Result<v2::TextEndedData, _> = serde_json::from_value(env.data.clone())
                else {
                    return;
                };
                if !self.assistant_messages.contains(&d.assistant_message_id) {
                    return;
                }
                let key = v2_reasoning_key(&d.assistant_message_id, d.ordinal);
                self.reasoning_part_ids.insert(key.clone());
                if self.reasonings.insert(key.clone(), d.text.clone()).is_none() {
                    self.reasoning_order.push(key.clone());
                }
                if self.live_reasoned_ids.contains(&key) && !d.text.trim().is_empty() {
                    if let Ok(mut q) = self.pending.lock() {
                        q.push(proto_live_line(
                            now_ms(),
                            THINK_LIVE_TAG,
                            d.text.trim_end(),
                            &key,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    /// v2 `session.idle` — flush the accumulated answer + reasoning to QML and
    /// reset the turn (mirrors the v1 `idle_session_id` block).
    async fn on_v2_idle(
        &mut self,
        env: &opencode_client::types::v2::EventEnvelope,
        client: &impl api::OpenCodeApi,
        sessions: &Arc<std::sync::Mutex<String>>,
        dispatcher: &event::dispatcher::SharedState,
    ) {
        use opencode_client::types::v2::event as v2;
        use opencode_client::types::v2::keys as k;
        // Turn end in 2.0.32 is `session.execution.succeeded` (success) or
        // `session.execution.interrupted` (user abort) — the legacy
        // `session.idle` is not emitted on this path. All three end the turn
        // cleanly; only `session.execution.failed` is an error (handled in the
        // dispatcher).
        if !matches!(
            env.kind.as_str(),
            k::IDLE | k::EXECUTION_SUCCEEDED | k::EXECUTION_INTERRUPTED
        ) {
            return;
        }
        let Ok(d): Result<v2::SessionIdleData, _> = serde_json::from_value(env.data.clone()) else {
            return;
        };
        if self.current_session != Some(d.sessionID.as_str()) {
            return;
        }
        let text: String = self
            .text_order
            .iter()
            .filter_map(|pid| self.parts.get(pid))
            .map(|s| s.as_str())
            .collect();
        if !text.trim().is_empty() {
            for rid in self.reasoning_order.iter() {
                if let Some(rea) = self.reasonings.get(rid) {
                    if !rea.trim().is_empty() && !self.live_reasoned_ids.contains(rid) {
                        if let Ok(mut q) = self.pending.lock() {
                            let body = format!("{THINK_TAG}{}", rea.trim_end());
                            q.push(proto_line(now_ms(), &body));
                        }
                    }
                }
            }
            let answer_was_live = self.parts.keys().any(|k| self.text_live.contains_key(k));
            if !answer_was_live {
                if let Ok(mut q) = self.pending.lock() {
                    q.push(proto_line(now_ms(), text.trim_end()));
                }
            }
            let tts_mode = self.voice.tts_mode.lock().map(|m| *m).unwrap_or_default();
            // The user aborted the turn: the partial answer in `parts` is not
            // a completed reply — never auto-speak it (Jack's smoke: abort
            // mid-generation spoke the fragment).
            let aborted = env.kind.as_str() == k::EXECUTION_INTERRUPTED;
            if tts_mode == voice::TtsMode::Auto && !aborted {
                for pid in self.text_order.iter() {
                    if self.spoken_parts.contains(pid) || self.voice.dictating() {
                        continue;
                    }
                    if let Some(p) = self.parts.get(pid) {
                        let p = p.trim_end();
                        if !p.is_empty() {
                            self.spoken_parts.insert(pid.clone());
                            voice::tts_from_call(self.voice, p, self.pending).await;
                        }
                    }
                }
            }
        }
        set_server_status(self.status, &markers::ServerConnectionStatus::Idle);
        self.parts.clear();
        self.text_live.clear();
        self.text_order.clear();
        self.spoken_parts.clear();
        self.reasonings.clear();
        self.reasoning_order.clear();
        self.reasoning_part_ids.clear();
        self.live_reasoned_ids.clear();
        *self.last_reasoning_push = 0;
        *self.last_answer_push = 0;
        refresh_sessions(client, sessions, &dispatcher).await;
    }
}

/// Accumulator key for a v2 text block (`t:<assistantMessageID>:<ordinal>`).
/// opencode numbers ordinals SEPARATELY for reasoning and text parts (both
/// start at 0), so the kind prefix keeps the two from colliding on one
/// `[[p:key]]` (the official TUI namespaces parts the same way).
fn v2_text_key(assistant_message_id: &str, ordinal: i64) -> String {
    format!("t:{assistant_message_id}:{ordinal}")
}

/// Accumulator key for a v2 reasoning block — see [`v2_text_key`].
fn v2_reasoning_key(assistant_message_id: &str, ordinal: i64) -> String {
    format!("r:{assistant_message_id}:{ordinal}")
}

/// A single SSE session: `Ok(())` means the stream ended, `Err` means connection failed.
#[allow(clippy::too_many_arguments)]
async fn run_stream<A: api::OpenCodeApi>(
    client: A,
    dispatcher: event::dispatcher::SharedState,
    worker_pending: Arc<std::sync::Mutex<Vec<String>>>,
    worker_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    status: Arc<std::sync::Mutex<String>>,
    sessions: Arc<std::sync::Mutex<String>>,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
    current_session_shared: Arc<std::sync::Mutex<String>>,
    compacting: Arc<std::sync::Mutex<bool>>,
    nav: Arc<std::sync::Mutex<i32>>,
    todos: Arc<std::sync::Mutex<String>>,
    models: Arc<std::sync::Mutex<String>>,
    sounds: Arc<std::sync::Mutex<String>>,
    voice: Arc<voice::VoiceState>,
    tools: Arc<std::sync::Mutex<String>>,
    permissions: Arc<std::sync::Mutex<String>>,
    permission_error: Arc<std::sync::Mutex<String>>,
    saved_permissions: Arc<std::sync::Mutex<String>>,
    forms: Arc<std::sync::Mutex<String>>,
    settings: Arc<std::sync::Mutex<String>>,
    console_login: opencode_client::auth::ConsoleLogin,
) -> Result<StreamEnd, String> {
    let mut stream = client.event_stream().await?;
    log::info!("SSE connected");
    set_server_status(&status, &markers::ServerConnectionStatus::Idle);

    // Accumulate assistant text parts by part.id (see example full_cycle).
    let mut parts: std::collections::HashMap<String, String> = Default::default();
    let mut current_session: Option<String> = None;
    // Per-session agent mode (build/plan) as chosen in the UI. Must be re-sent
    // on every prompt: the server resets the session agent to "build" when a
    // prompt arrives without an agent, which silently disables plan mode.
    let mut session_agents: std::collections::HashMap<String, String> = Default::default();
    // IDs of assistant messages — only their text parts go into the answer.
    let mut assistant_messages: std::collections::HashSet<String> = Default::default();
    // Model "reasoning": accumulate reasoning parts by part.id (same as text).
    // Two event sources feed this map:
    //   - `message.part.updated` with `Part::Reasoning` — a FULL part. The server
    //     emits it at reasoning-start (empty text) and at reasoning-end (complete).
    //   - `message.part.delta` — the ON-THE-FLY chunks (server's `reasoning-delta` →
    //     `session.updatePartDelta({ field: "text" })`). The TUI mirrors these to
    //     show thinking while the model works.
    // The worker streams live chunks to QML as `[[thinklive]]` lines that REPLACE
    // the trailing `[[think]]` bubble in place; at reasoning-end/idle the final
    // text is re-sent once (so the bubble is exact even if throttling skipped
    // trailing deltas) and the static `[[think]]` line is NOT duplicated.
    // TTS does not read reasoning: auto-TTS takes only the answer text.
    let mut reasonings: std::collections::HashMap<String, String> = Default::default();
    // Reasoning part.ids in ARRIVAL ORDER. A single model turn may contain several
    // reasoning blocks (think → tool → think): the map above can't keep the
    // sequence, and sorting values alphabetically would shuffle them. Each block
    // is rendered as its OWN [[think]] bubble — see the idle handler below.
    let mut reasoning_order: Vec<String> = Vec::new();
    // part.id of parts confirmed to be `type:"reasoning"` (via message.part.updated).
    // Needed to route `message.part.delta` (which carries only partID + field) to
    // the right accumulator instead of the text one.
    let mut reasoning_part_ids: std::collections::HashSet<String> = Default::default();
    // Reasoning part.ids already streamed live (`[[thinklive]]` pushed at least once):
    // the QML bubble exists, so on idle we must NOT push a static [[think]] duplicate.
    let mut live_reasoned_ids: std::collections::HashSet<String> = Default::default();
    // Timestamp of the last live reasoning push — throttle so a long thinking
    // phase does not flood the QML queue with hundreds of tiny lines.
    let mut last_reasoning_push: u64 = 0;
    // Live text-part streaming: deltas of every answer text part accumulate
    // per part.id and are pushed as `[[answerlive]]` while the model types,
    // so the answer bubble grows in place (mirrors opencode's TUI rendering
    // the text part on each delta). Without this the whole answer would pop
    // in at once on idle. Reasoning, if any, arrives in separate
    // `type:"reasoning"` parts and streams as `[[thinklive]]` (see above).
    let mut text_live: std::collections::HashMap<String, String> = Default::default();
    let mut last_answer_push: u64 = 0;
    // Answer text part.ids in ARRIVAL ORDER. A turn may contain several text
    // parts (text → tool → text); the map keyed by id cannot keep the sequence
    // and sorting by value would shuffle the answer. Each part is also spoken
    // as its OWN auto-TTS job, bound to its own bubble.
    let mut text_order: Vec<String> = Vec::new();
    // Text part.ids already queued for auto-TTS. A part is spoken as soon as
    // its bubble is final (text-end during the turn — no waiting for idle), so
    // the fallback idle pass must not queue it a second time.
    let mut spoken_parts: std::collections::HashSet<String> = Default::default();
    // Periodically refresh sessions list (tick every 200ms; query roughly every 3s).
    let mut ticks: u32 = 0;
    // Persistent ticker (NOT a fresh `sleep` per loop turn): a recreated timer
    // never completes when the SSE stream keeps returning immediately (e.g. a
    // reconnect storm against a missing server), which starved the command
    // queue — QML never received settings.
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // The first `tick()` resolves immediately; consume it so the loop body runs
    // on a steady 200ms cadence.
    tick.tick().await;
    // Session the server reported compacted (`session.compaction.ended`).
    // `dispatch_v2` records it here; below, after the dispatch call, we reload
    // that session's history so the summary replaces the old turns without
    // re-entering the chat.
    let mut compacted_session: Option<String> = None;
    // Compaction part.id already announced with a "Compacting…" notice, so a
    // repeat `message.part.updated` for the same compaction does not stack a
    // second bubble (the dispatcher fills/reads this; see `StreamState`).
    let mut compaction_seen: Option<String> = None;
    // IDs of `mode:"compaction"` assistant messages (the compressed-history
    // summary). They render as a normal growing bubble, but auto-TTS must skip
    // them — the summary is internal machinery, not an answer to the user.
    let mut compaction_messages: std::collections::HashSet<String> = Default::default();

    // Show previously started sessions and available models immediately.
    refresh_sessions(&client, &sessions, &dispatcher).await;
    refresh_models(&client, &models).await;
    refresh_sounds(&sounds);
    // Resume the last session on startup — the chat shows its history.
    resume_last_session(&client, &mut current_session, &current_session_shared, &worker_pending, &todos, &dispatcher, &sessions).await;
    // The server may already be waiting for an answer (the client started
    // late, or the `form.created` frame was missed): ask for the pending
    // forms once, up front.
    refresh_forms(&client, &dispatcher, &forms, current_session.as_deref()).await;
    // The same for the permission requests: the server may already be waiting
    // for an answer (the client started late, or the `permission.asked` frame
    // was missed).
    refresh_permissions(
        &client,
        &dispatcher,
        &permissions,
        current_session.as_deref(),
    )
    .await;
    refresh_saved_permissions_json(&client, &saved_permissions).await;

    // Main loop: listen to SSE and poll the bridge for new prompts in parallel.
    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(env)) => {
                        // The server reports its own retry state (retryable model
                        // errors such as a failed URL call or an exhausted quota):
                        // `session.status` carries the human message, the attempt
                        // number and `next` — the ms timestamp of the next try.
                        // Mirror the TUI: show "message — retrying in N s (#attempt)".
                        if let Some((sid, retry)) = retry_status(&env) {
                            if current_session.as_deref() == Some(sid.as_str()) {
                                match retry {
                                    RetryStatus::Retry { attempt, message, next } => {
                                        // Token consumed by QML StatusHeader (live countdown):
                                        // `retry|<next_ms>|<attempt>|<message>`.
                                        log::info!(
                                            "session retry #{} at {}: {message}",
                                            attempt, next
                                        );
                                        set_server_status(
                                            &status,
                                            &markers::ServerConnectionStatus::Retry {
                                                next,
                                                attempt,
                                                message,
                                            },
                                        );
                                    }
                                    RetryStatus::Busy => {
                                        set_server_status(&status, &markers::ServerConnectionStatus::Busy)
                                    }
                                    RetryStatus::Idle => {}
                                }
                            }
                        }
                        {
                            // The event hub (`dispatch`) owns what each server
                            // event does; we only lend it the streaming
                            // accumulators for this turn.
                            let mut stream = event::dispatcher::StreamState {
                                pending: &*worker_pending,
                                status: &*status,
                                assistant_messages: &mut assistant_messages,
                                compaction_messages: &mut compaction_messages,
                                reasonings: &mut reasonings,
                                reasoning_part_ids: &reasoning_part_ids,
                                live_reasoned_ids: &mut live_reasoned_ids,
                                last_reasoning_push: &mut last_reasoning_push,
                                text_live: &mut text_live,
                                last_answer_push: &mut last_answer_push,
                                current_session: current_session.as_deref(),
                                compacted: &mut compacted_session,
                                compaction_seen: &mut compaction_seen,
                                compacting: &*compacting,
                            };
                            let mut st = dispatcher.lock().await;
                            let _ = event::dispatcher::dispatch_v2(&env, &mut st, &mut stream);
                        }
                        // A form arrived (or was settled): hand the new queue
                        // to QML right away — waiting for the 1 s tick would
                        // delay the form dialog by a whole tick.
                        if matches!(
                            env.kind.as_str(),
                            v2keys::FORM_CREATED | v2keys::FORM_REPLIED | v2keys::FORM_CANCELLED
                        ) {
                            refresh_forms_json(&dispatcher, &forms).await;
                        }
                        // The same for a permission request: hand the new queue
                        // to QML right away — waiting for the 1 s tick would
                        // delay the dialog by a whole tick.
                        if matches!(
                            env.kind.as_str(),
                            v2keys::PERMISSION_ASKED
                                | v2keys::PERMISSION_REPLIED
                                | v2keys::SESSION_ERROR
                        ) {
                            refresh_permissions_json(&dispatcher, &permissions).await;
                        }
                        // The server compacted the shown session: reload its
                        // history so the summary and the "Context compacted"
                        // marker appear right away (previously the feed only
                        // refreshed after leaving and re-entering the chat).
                        if let Some(sid) = compacted_session.take() {
                            // The compaction that was announced is over: drop
                            // the dedup marker regardless of which session it
                            // belonged to, so a later compaction (or a return
                            // to this session) starts from a clean state.
                            compaction_seen = None;
                            if current_session.as_deref() == Some(sid.as_str()) {
                                log::info!("session compacted: reloading history of {sid}");
                                parts.clear();
                                assistant_messages.clear();
                                compaction_messages.clear();
                                reasonings.clear();
                                reasoning_order.clear();
                                text_live.clear();
                                text_order.clear();
                                spoken_parts.clear();
                                last_answer_push = 0;
                                if let Ok(mut g) = compacting.lock() {
                                    *g = false;
                                }
                                clear_tools(&dispatcher, &tools).await;
                                if let Ok(mut q) = worker_pending.lock() {
                                    q.push(CLEAR_FEED_TAG.to_string());
                                }
                                load_history(&client, &sid, &worker_pending, &dispatcher).await;
                                refresh_sessions(&client, &sessions, &dispatcher).await;
                                refresh_todo(&client, &sid, &todos).await;
                            } else {
                                // Compaction ended in a session the user is not
                                // looking at: nothing on screen to reload.
                                log::info!("session {sid} compacted in background");
                            }
                        }
                        let mut turn = TurnState {
                            parts: &mut parts,
                            text_order: &mut text_order,
                            text_live: &mut text_live,
                            spoken_parts: &mut spoken_parts,
                            assistant_messages: &assistant_messages,
                            compaction_messages: &compaction_messages,
                            reasonings: &mut reasonings,
                            reasoning_order: &mut reasoning_order,
                            reasoning_part_ids: &mut reasoning_part_ids,
                            live_reasoned_ids: &mut live_reasoned_ids,
                            last_reasoning_push: &mut last_reasoning_push,
                            last_answer_push: &mut last_answer_push,
                            current_session: current_session.as_deref(),
                            pending: &worker_pending,
                            status: &status,
                            voice: &voice,
                        };
                        turn.on_v2_event(&env, &client).await;
                        turn.on_v2_idle(&env, &client, &sessions, &dispatcher).await;
                    }
                    Some(Err(e)) => log::error!("sse error: {e}"),
                    None => {
                        log::error!("SSE closed");
                        set_server_status(&status, &markers::ServerConnectionStatus::Error);
                        return Ok(StreamEnd::Reconnect);
                    }
                }
            }
            _ = tick.tick() => {
                ticks = ticks.wrapping_add(1);

                // Commands from QML: new / open / rename / delete (JSON).
                for raw in commands.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default() {
                    let command: Cmd = match serde_json::from_str(&raw) {
                        Ok(c) => c,
                        Err(e) => {
                            log::error!("command not JSON: {e}: {raw}");
                            continue;
                        }
                    };
                    match &command {
                        Cmd::New { title } => match client.create_session(title.as_deref()).await {
                            Ok(sess) => match sess["id"].as_str() {
                                Some(id) => {
                                    log::info!("new session: {id}");
                                    current_session = Some(id.to_string());
                                    persist_last_session(Some(id));
                                    parts.clear();
                                    assistant_messages.clear();
                                    compaction_messages.clear();
                                    reasonings.clear();
                                    reasoning_order.clear();
                                    text_live.clear();
                                    text_order.clear();
                                    spoken_parts.clear();
                                    last_answer_push = 0;
                                    // Fresh session: no compaction state carries
                                    // over from the previously shown one.
                                    compaction_seen = None;
                                    if let Ok(mut g) = compacting.lock() {
                                        *g = false;
                                    }
                                    clear_tools(&dispatcher, &tools).await;
                                    ensure_session_model(&client, id, &dispatcher).await;
                                    refresh_sessions(&client, &sessions, &dispatcher).await;
                                    set_current_session(&current_session_shared, &current_session);
                                    if let Ok(mut g) = todos.lock() {
                                        *g = "[]".to_string();
                                    }
                                    // A fresh session is never reverted.
                                    if let Ok(mut q) = worker_pending.lock() {
                                        q.push(format!("{REVERTED_TAG}0"));
                                    }
                                }
                                None => log::error!("new: session.id missing in response: {sess}"),
                            },
                            Err(e) => log::error!("new_session: {e}"),
                        },
                        Cmd::Open { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            log::info!("open session: {id}");
                            current_session = Some(id.to_string());
                            persist_last_session(Some(id.as_str()));
                            parts.clear();
                            assistant_messages.clear();
                            compaction_messages.clear();
                            reasonings.clear();
                            reasoning_order.clear();
                            text_live.clear();
                            text_order.clear();
                            spoken_parts.clear();
                            last_answer_push = 0;
                            // The compaction state is per-shown-session: a
                            // switch must not leave the "Compacting…" banner
                            // (or the dedup marker) behind on the new session.
                            compaction_seen = None;
                            if let Ok(mut g) = compacting.lock() {
                                *g = false;
                            }
                            clear_tools(&dispatcher, &tools).await;
                            load_history(&client, id, &worker_pending, &dispatcher).await;
                            ensure_session_model(&client, id, &dispatcher).await;
                            refresh_sessions(&client, &sessions, &dispatcher).await;
                            set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                            set_current_session(&current_session_shared, &current_session);
                            refresh_todo(&client, id, &todos).await;
                        }
                        Cmd::Fork { id, message } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.fork_session(id, message.as_deref()).await {
                                Ok(sess) => match sess["id"].as_str() {
                                    Some(new_id) => {
                                        log::info!("fork {id} -> {new_id}");
                                        current_session = Some(new_id.to_string());
                                        persist_last_session(Some(new_id));
                                        parts.clear();
                                        assistant_messages.clear();
                                        compaction_messages.clear();
                                        reasonings.clear();
                                        reasoning_order.clear();
                                        text_live.clear();
                                        text_order.clear();
                                        spoken_parts.clear();
                                        last_answer_push = 0;
                                        // The fork is a different session: reset
                                        // the per-shown-session compaction state.
                                        compaction_seen = None;
                                        if let Ok(mut g) = compacting.lock() {
                                            *g = false;
                                        }
                                        clear_tools(&dispatcher, &tools).await;
                                        load_history(&client, new_id, &worker_pending, &dispatcher).await;
                                        ensure_session_model(&client, new_id, &dispatcher).await;
                                        refresh_sessions(&client, &sessions, &dispatcher).await;
                                        set_current_session(&current_session_shared, &current_session);
                                        refresh_todo(&client, new_id, &todos).await;
                                        request_nav(&nav, 2);
                                    }
                                    None => log::error!("fork: id missing in response: {sess}"),
                                },
                                Err(e) => log::error!("fork_session: {e}"),
                            }
                        }
                        Cmd::Revert { id, message } => {
                            if id.is_empty() || message.is_empty() {
                                continue;
                            }
                            if current_session.as_deref() != Some(id.as_str()) {
                                log::warn!("revert for non-current session {id}, ignored");
                                continue;
                            }
                            log::info!("revert {id} at {message}");
                            match client.revert_session(id, message).await {
                                Ok(()) => {
                                    parts.clear();
                                    assistant_messages.clear();
                                    compaction_messages.clear();
                                    reasonings.clear();
                                    reasoning_order.clear();
                                    text_live.clear();
                                    text_order.clear();
                                    spoken_parts.clear();
                                    last_answer_push = 0;
                                    clear_tools(&dispatcher, &tools).await;
                                    load_history(&client, id, &worker_pending, &dispatcher).await;
                                    set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                                    refresh_todo(&client, id, &todos).await;
                                }
                                Err(e) => log::error!("revert_session: {e}"),
                            }
                        }
                        Cmd::Unrevert { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            if current_session.as_deref() != Some(id.as_str()) {
                                log::warn!("unrevert for non-current session {id}, ignored");
                                continue;
                            }
                            log::info!("unrevert {id}");
                            match client.unrevert_session(id).await {
                                Ok(()) => {
                                    parts.clear();
                                    assistant_messages.clear();
                                    compaction_messages.clear();
                                    reasonings.clear();
                                    reasoning_order.clear();
                                    text_live.clear();
                                    text_order.clear();
                                    spoken_parts.clear();
                                    last_answer_push = 0;
                                    clear_tools(&dispatcher, &tools).await;
                                    load_history(&client, id, &worker_pending, &dispatcher).await;
                                    set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                                }
                                Err(e) => log::error!("unrevert_session: {e}"),
                            }
                        }
                        #[cfg(debug_assertions)]
                        Cmd::MockPermission => {
                            let mut st = dispatcher.lock().await;
                            st.permissions.push(opencode_client::types::PermissionRequest {
                                id: format!("mock-perm-{}", std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis()).unwrap_or(0)),
                                session_id: "mock-session".to_string(),
                                action: "bash".to_string(),
                                resources: vec!["npm install && npm run build".to_string()],
                                save: None,
                                metadata: None,
                                source: None,
                                message: Some("Разрешить запуск команды?".to_string()),
                            });
                            drop(st);
                            refresh_permissions_json(&dispatcher, &permissions).await;
                        }
                        Cmd::Permission { session, id, response, message } => {
                            if session.is_empty() || id.is_empty() {
                                continue;
                            }
                            // Mock permission doesn't exist on server — just remove from queue.
                            // (This branch is only in debug build; no point looking for mock-session in release.)
                            #[cfg(debug_assertions)]
                            if session == "mock-session" {
                                let mut st = dispatcher.lock().await;
                                st.permissions.pop(id);
                                drop(st);
                                refresh_permissions_json(&dispatcher, &permissions).await;
                                log::info!("permission mock {id} removed from queue");
                                continue;
                            }
                            match client
                                .answer_permission(session, id, response, message.as_deref())
                                .await
                            {
                                Ok(true) => {
                                    log::info!("permission {session}/{id} -> {response:?}");
                                    // Drop it from the queue right away; the
                                    // `permission.replied` event does the same.
                                    {
                                        let mut st = dispatcher.lock().await;
                                        st.permissions.pop(id);
                                    }
                                    set_permission_error(&permission_error, "");
                                    refresh_permissions_json(&dispatcher, &permissions).await;
                                }
                                Ok(false) => {
                                    log::warn!("permission {session}/{id}: сервер не принял ответ");
                                    // The request is still pending — keep the card
                                    // and tell the user why nothing happened.
                                    set_permission_error(
                                        &permission_error,
                                        "Сервер не принял ответ — попробуй ещё раз",
                                    );
                                    refresh_permissions_json(&dispatcher, &permissions).await;
                                }
                                Err(e) => {
                                    log::error!("answer_permission: {e}");
                                    // The card stays on screen: the user must be
                                    // able to answer again (the queue is only
                                    // cleared on success).
                                    set_permission_error(
                                        &permission_error,
                                        &format!("Не удалось ответить: {e}"),
                                    );
                                    refresh_permissions_json(&dispatcher, &permissions).await;
                                }
                            }
                        }
                        Cmd::RemoveSavedPermission { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.remove_saved_permission(&id).await {
                                Ok(true) => log::info!("saved permission {id} revoked"),
                                Ok(false) => {
                                    log::warn!("saved permission {id}: сервер не принял удаление")
                                }
                                Err(e) => log::error!("remove_saved_permission: {e}"),
                            }
                            refresh_saved_permissions_json(&client, &saved_permissions).await;
                        }
                        Cmd::Form { session, id, answer } => {
                            if session.is_empty() || id.is_empty() {
                                continue;
                            }
                            // No answer → the user dismissed the form: cancel it.
                            let result = match answer {
                                Some(a) => client.reply_form(session, id, a.clone()).await,
                                None => client.cancel_form(session, id, None).await,
                            };
                            match result {
                                Ok(accepted) => {
                                    if accepted {
                                        log::info!("form {session}/{id} settled");
                                    } else {
                                        log::warn!("form {session}/{id}: сервер не принял ответ");
                                    }
                                    // Drop it from the queue right away; the
                                    // `form.replied`/`form.cancelled` event does
                                    // the same.
                                    {
                                        let mut st = dispatcher.lock().await;
                                        st.forms.pop(id);
                                    }
                                    refresh_forms_json(&dispatcher, &forms).await;
                                }
                                Err(e) => log::error!("form {session}/{id}: {e}"),
                            }
                        }
                        Cmd::Summarize { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            // Show the "compacting" banner the instant the user
                            // asks.
                            if current_session.as_deref() == Some(id.as_str()) {
                                if let Ok(mut g) = compacting.lock() {
                                    *g = true;
                                }
                            }
                            // `POST /summarize` (v1) / `POST /compact` (v2) on the
                            // server only returns once the whole compaction is
                            // DONE (~20 s). Awaiting it here would freeze this loop
                            // — the very loop that reads the SSE stream — so every
                            // live event of the compaction (the growing summary
                            // text) would only be drained after the fact, and then
                            // immediately wiped by the `session.compacted` reload:
                            // the summary bubble would never grow live. Run the
                            // blocking POST off-task so SSE keeps flowing while it
                            // is written.
                            //
                            // The model gate was removed: the v2 server takes the
                            // model from the session itself (its GET/list carry no
                            // `model` field at all), and the v1 server accepts the
                            // ids only as hints — requiring them here silently
                            // blocked every compact on v2.
                            let task_client = client.clone();
                            let task_compacting = compacting.clone();
                            let task_id = id.clone();
                            tokio::spawn(async move {
                                use opencode_client::types::v2::keys as k;
                                let mut provider = String::new();
                                let mut model = String::new();
                                if let Ok(s) = task_client.get_session(&task_id).await {
                                    provider = s[k::MODEL][k::PROVIDER_ID]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    model = s[k::MODEL][k::ID].as_str().unwrap_or("").to_string();
                                }
                                match task_client
                                    .summarize_session(&task_id, &provider, &model)
                                    .await
                                {
                                    Ok(()) => log::info!("summarize {task_id} requested"),
                                    Err(e) => log::error!("summarize_session: {e}"),
                                }
                                // The compaction is over (the buffered
                                // `session.compacted` also clears this for the
                                // shown session; do it here too so a failed or
                                // duplicate path never leaves the banner on).
                                if let Ok(mut g) = task_compacting.lock() {
                                    *g = false;
                                }
                            });
                        }
                        Cmd::Abort { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.abort(id).await {
                                Ok(_) => {
                                    log::info!("abort {id}");
                                    set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                                }
                                Err(e) => log::error!("abort: {e}"),
                            }
                        }
                        Cmd::SetModel { id, provider, model } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.set_model(id, provider, model).await {
                                Ok(()) => {
                                    log::info!("set_model {id} -> {provider}/{model}");
                                    // Record it immediately so the picker/sessions
                                    // list reflect the change even before the
                                    // `session.model.selected` event arrives.
                                    dispatcher.lock().await.session_models.insert(
                                        id.clone(),
                                        (provider.clone(), model.clone()),
                                    );
                                    refresh_sessions(&client, &sessions, &dispatcher).await;
                                }
                                Err(e) => log::error!("set_model: {e}"),
                            }
                        }
                        Cmd::SetMode { id, mode } => {
                            if id.is_empty() {
                                continue;
                            }
                            if !mode.is_empty() {
                                session_agents.insert(id.to_string(), mode.to_string());
                            }
                            match client.set_session_agent(id, mode).await {
                                Ok(()) => log::info!("set_mode {id} -> {mode}"),
                                Err(e) => log::error!("set_mode: {e}"),
                            }
                        }
                        Cmd::Rename { id, title } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.rename_session(id, title).await {
                                Ok(()) => {
                                    log::info!("renamed {id} -> {title:?}");
                                    refresh_sessions(&client, &sessions, &dispatcher).await;
                                }
                                Err(e) => log::error!("rename_session: {e}"),
                            }
                        }
                        Cmd::Delete { id } => {
                            if id.is_empty() {
                                continue;
                            }
                            match client.delete_session(id).await {
                                Ok(()) => {
                                    log::info!("deleted {id}");
                                    if current_session.as_deref() == Some(id.as_str()) {
                                        current_session = None;
                                        persist_last_session(None);
                                        set_current_session(&current_session_shared, &current_session);
                                        if let Ok(mut g) = todos.lock() {
                                            *g = "[]".to_string();
                                        }
                                    }
                                    refresh_sessions(&client, &sessions, &dispatcher).await;
                                }
                                Err(e) => log::error!("delete_session: {e}"),
                            }
                        }
                        Cmd::DeleteAll => {
                            let ids: Vec<String> = match client.list_sessions().await {
                                Ok(v) => v
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|s| {
                                                s["id"].as_str().map(|x| x.to_string())
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                Err(e) => {
                                    log::error!("delete_all: list_sessions: {e}");
                                    Vec::new()
                                }
                            };
                            if ids.is_empty() {
                                log::info!("delete_all: сессий нет");
                            } else {
                                let mut ok = 0;
                                for id in &ids {
                                    match client.delete_session(id).await {
                                        Ok(()) => ok += 1,
                                        Err(e) => log::error!("delete_all: {id}: {e}"),
                                    }
                                }
                                log::info!("delete_all: удалено {ok}/{}", ids.len());
                            }
                            if current_session.is_some() {
                                current_session = None;
                                persist_last_session(None);
                                set_current_session(&current_session_shared, &current_session);
                                if let Ok(mut g) = todos.lock() {
                                    *g = "[]".to_string();
                                }
                            }
                            refresh_sessions(&client, &sessions, &dispatcher).await;
                            // After deleting all sessions, immediately create a new one — the chat must not be empty.
                            match client.create_session(None).await {
                                Ok(sess) => match sess["id"].as_str() {
                                    Some(id) => {
                                        log::info!("delete_all: creating new session {id}");
                                        current_session = Some(id.to_string());
                                        persist_last_session(Some(id));
                                        parts.clear();
                                        assistant_messages.clear();
                                        compaction_messages.clear();
                                        reasonings.clear();
                                        reasoning_order.clear();
                                        text_live.clear();
                                        text_order.clear();
                                        spoken_parts.clear();
                                        last_answer_push = 0;
                                        // A brand-new session: no compaction state
                                        // from the deleted one survives.
                                        compaction_seen = None;
                                        if let Ok(mut g) = compacting.lock() {
                                            *g = false;
                                        }
                                        clear_tools(&dispatcher, &tools).await;
                                        set_current_session(&current_session_shared, &current_session);
                                        if let Ok(mut g) = todos.lock() {
                                            *g = "[]".to_string();
                                        }
                                        // A new session must appear in the sessions list immediately.
                                        refresh_sessions(&client, &sessions, &dispatcher).await;
                                    }
                                    None => log::error!("delete_all: нет id при создании"),
                                },
                                Err(e) => log::error!("delete_all: create_session: {e}"),
                            }
                            request_nav(&nav, 2);
                        }
                        Cmd::VoiceLang { .. }
                        | Cmd::VoiceSelectStt { .. }
                        | Cmd::VoiceSelectTts { .. }
                        | Cmd::VoiceDownload { .. }
                        | Cmd::VoiceDownloadCancel { .. }
                        | Cmd::VoiceDelete { .. }
                        | Cmd::VoiceRecordStart
                        | Cmd::VoiceRecordStop
                        | Cmd::VoiceStt
                        | Cmd::VoiceTts { .. }
                        | Cmd::VoiceTtsMode { .. }
                        | Cmd::VoiceTtsCancel
                        | Cmd::VoiceTtsPlayed { .. }
                        | Cmd::VoiceCatalogUpdate => {
                            voice::run_command(&voice, &command, &worker_pending).await;
                        }

                        Cmd::GetSettings => {
                            let mut st = Settings::load();
                            st.save();
                            if let Ok(mut g) = settings.lock() {
                                *g = st.to_json();
                            }
                        }
                        Cmd::SaveSettings { value } => {
                            let mut value = value.clone();
                            value.save();
                            if let Ok(mut g) = settings.lock() {
                                *g = value.to_json();
                            }
                        }
                        Cmd::SetWorkdir { workdir } => {
                            let mut st = Settings::load();
                            st.workdir = workdir.clone();
                            st.save();
                            if let Ok(mut g) = settings.lock() {
                                *g = st.to_json();
                            }
                        }
                        Cmd::SetProviderKey { provider, key } => {
                            let key = key.trim().to_string();
                            if key.is_empty() {
                                if let Ok(mut q) = worker_pending.lock() {
                                    q.push(svc_line(now_ms(), i18n::K_AUTH_EMPTY, &[]));
                                }
                            } else {
                                match client.set_api_key(&provider, &key).await {
                                    Ok(()) => {
                                        log::info!("auth: saved API key for provider {provider:?}");
                                        if let Ok(mut q) = worker_pending.lock() {
                                            q.push(svc_line(now_ms(), i18n::K_AUTH_SAVED, &[]));
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("auth: failed to save key for {provider:?}: {e}");
                                        if let Ok(mut q) = worker_pending.lock() {
                                            q.push(err_line(now_ms(), i18n::K_AUTH_SAVE_FAILED, &[e]));
                                        }
                                    }
                                }
                            }
                        }
                        Cmd::ConsoleLogin => {
                            if let Err(e) = console_login.start() {
                                log::warn!("console login: {e}");
                            }
                        }
                        Cmd::ConsoleLoginCancel => {
                            console_login.cancel();
                            log::info!("console login: cancelled");
                        }
                    }
                }

                // Device login finished: on success reload the server config so
                // the fresh account (org/models) is used, then tell the user.
                if let Some(ok) = console_login.take_finished() {
                    let msg = console_login.message();
                    if ok {
                        log::info!("console login: success, disposing instances to reload config");
                        if let Err(e) = client.global_dispose().await {
                            log::warn!("console login: /global/dispose failed: {e}");
                        }
                        refresh_models(&client, &models).await;
                        if let Ok(mut q) = worker_pending.lock() {
                            q.push(svc_line(now_ms(), i18n::K_AUTH_SIGNED_IN, &[]));
                        }
                    } else {
                        log::warn!("console login: failed: {msg}");
                        if let Ok(mut q) = worker_pending.lock() {
                            q.push(err_line(now_ms(), i18n::K_AUTH_SIGNIN_FAILED, &[msg]));
                        }
                    }
                }

                for prompt in worker_prompts.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default() {
                    log::info!("QML промпт: {prompt}");
                    set_server_status(&status, &markers::ServerConnectionStatus::Busy);
                    // Create a session if there is none.
                    if current_session.is_none() {
                        match client.create_session(None).await {
                            Ok(sess) => match sess["id"].as_str() {
                                Some(id) => {
                                    current_session = Some(id.to_string());
                                    persist_last_session(Some(id));
                                    set_current_session(&current_session_shared, &current_session);
                                }
                                None => {
                                    let msg = format!("в ответе нет session.id: {sess}");
                                    log::error!("{msg}");
                                    if let Ok(mut q) = worker_pending.lock() {
                                        q.push(svc_line(now_ms(), i18n::K_SESSION_ERROR, &[msg]));
                                    }
                                    set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                                    continue;
                                }
                            },
                            Err(e) => {
                                log::error!("create_session: {e}");
                                let emsg = e.to_string();
                                if let Ok(mut q) = worker_pending.lock() {
                                    q.push(svc_line(
                                        now_ms(),
                                        i18n::K_SESSION_ERROR,
                                        &[emsg],
                                    ));
                                }
                                set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                                continue;
                            }
                        }
                    }
                    if let Some(sid) = current_session.clone() {
                        // New user turn — previous turn's tools disappear from the feed.
                        {
                            let mut st = dispatcher.lock().await;
                            st.tools.clear();
                        }
                        // A new prompt drops the reverted tail server-side
                        // (SessionRevert.cleanup), so the "History reverted"
                        // banner no longer applies — clear it here so EVERY send
                        // path is covered (text input AND voice/PTT), not just
                        // QML `send()`. This is the "clear on new history" hook.
                        if let Ok(mut q) = worker_pending.lock() {
                            q.push(format!("{REVERTED_TAG}0"));
                        }
                        match client.prompt_async(&sid, &prompt, session_agents.get(&sid).cloned()).await {
                            Err(e) => {
                                log::error!("prompt_async: {e}");
                                let emsg = e.to_string();
                                if let Ok(mut q) = worker_pending.lock() {
                                    q.push(svc_line(now_ms(), i18n::K_PROMPT_ERROR, &[emsg]));
                                }
                                set_server_status(&status, &markers::ServerConnectionStatus::Idle);
                            }
                            // v2 returns the user-message id in the response:
                            // hand it straight to QML so the fresh bubble offers
                            // revert/fork, no SSE wait needed.
                            Ok(Some(id)) => {
                                if let Ok(mut q) = worker_pending.lock() {
                                    q.push(format!("{SET_MSG_ID_TAG}{id}"));
                                }
                            }
                            // No id in the answer: a server anomaly on v2
                            // (the id is part of a successful response).
                            Ok(None) => {
                                log::warn!("prompt_async не вернул id пользовательского сообщения");
                            }
                        }
                    }
                }

                // Every ~3 seconds refresh sessions list and TODOs for current session.
                if ticks % 15 == 0 {
                    refresh_sessions(&client, &sessions, &dispatcher).await;
                    if let Some(sid) = current_session.clone() {
                        refresh_todo(&client, &sid, &todos).await;
                    }
                    // Reconcile the form queue with the server (the SSE event is
                    // the fast path; this poll is the safety net).
                    refresh_forms(&client, &dispatcher, &forms, current_session.as_deref()).await;
                    // Same safety net for the permission requests (the SSE
                    // event is the fast path).
                    refresh_permissions(
                        &client,
                        &dispatcher,
                        &permissions,
                        current_session.as_deref(),
                    )
                    .await;
                }

                // Every ~1 second — snapshot of tools and permission requests for QML.
                if ticks % 5 == 0 {
                    refresh_tools_json(&dispatcher, &tools).await;
                    refresh_permissions_json(&dispatcher, &permissions).await;
                    refresh_forms_json(&dispatcher, &forms).await;
                }
            }
        }
    }
}

/// Load compact sessions list into shared buffer for QML.
///
/// `dispatcher` supplies the per-session current model tracked from v2
/// `session.model.selected` / `session.step.started` / history. The v2 session
/// list includes `model` only once the server has bound one, so this fills the
/// gap for sessions whose model we learned another way.
async fn refresh_sessions(
    client: &impl api::OpenCodeApi,
    sessions: &Arc<std::sync::Mutex<String>>,
    dispatcher: &event::dispatcher::SharedState,
) {
    let tracked_models: std::collections::HashMap<String, (String, String)> = {
        let state = dispatcher.lock().await;
        state.session_models.clone()
    };
    match client.list_sessions().await {
        Ok(v) => {
            let compact: Vec<serde_json::Value> = v
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| {
                            let id = s.get(keys::ID)?.clone();
                            let title = s
                                .get(keys::TITLE)
                                .cloned()
                                .unwrap_or(serde_json::Value::String(String::new()));
                            let updated = s
                                .get(keys::TIME)
                                .and_then(|t| t.get(keys::UPDATED))
                                .cloned()
                                .unwrap_or(serde_json::Value::Number(0.into()));
                            // Prefer the server's `model` (v1); fall back to the
                            // model we tracked from `session.model.selected`
                            // (v2, whose list has no `model`).
                            let id_str = id.as_str().unwrap_or("");
                            let tracked = tracked_models.get(id_str);
                            let provider_id = s
                                .get(keys::MODEL)
                                .and_then(|m| m.get(keys::PROVIDER_ID))
                                .cloned()
                                .unwrap_or_else(|| {
                                    serde_json::Value::String(
                                        tracked.map(|(p, _)| p.clone()).unwrap_or_default(),
                                    )
                                });
                            let model_id = s
                                .get(keys::MODEL)
                                .and_then(|m| m.get(keys::ID))
                                .cloned()
                                .unwrap_or_else(|| {
                                    serde_json::Value::String(
                                        tracked.map(|(_, m)| m.clone()).unwrap_or_default(),
                                    )
                                });
                            let cost = s
                                .get(keys::COST)
                                .cloned()
                                .unwrap_or(serde_json::Value::Number(0.into()));
                            let tokens = s.get(keys::TOKENS).cloned().unwrap_or(serde_json::Value::Null);
                            Some(serde_json::json!({
                                "id": id,
                                "title": title,
                                "updated": updated,
                                "providerID": provider_id,
                                "modelID": model_id,
                                "cost": cost,
                                "tokens": tokens,
                            }))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let json = serde_json::Value::Array(compact).to_string();
            if let Ok(mut g) = sessions.lock() {
                *g = json;
            }
        }
        Err(e) => log::error!("list_sessions: {e}"),
    }
}

/// Load session TODOs into shared buffer for QML.
async fn refresh_todo(
    client: &impl api::OpenCodeApi,
    session_id: &str,
    todos: &Arc<std::sync::Mutex<String>>,
) {
    match client.session_todo(session_id).await {
        Ok(v) => {
            if let Ok(mut g) = todos.lock() {
                *g = v.to_string();
            }
        }
        Err(e) => log::error!("session_todo: {e}"),
    }
}

/// Load providers/models and flatten for QML.
async fn refresh_models(
    client: &impl api::OpenCodeApi,
    models: &Arc<std::sync::Mutex<String>>,
) {
    let v = match client.providers().await {
        Ok(v) => v,
        Err(e) => {
            log::error!("providers: {e}");
            return;
        }
    };
    let mut flat: Vec<serde_json::Value> = Vec::new();
    if let Some(providers) = v["providers"].as_array() {
        for p in providers {
            let pid = p["id"].as_str().unwrap_or("");
            let pname = p["name"].as_str().unwrap_or(pid);
            if let Some(models_map) = p["models"].as_object() {
                let mut entries: Vec<(&String, &serde_json::Value)> = models_map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                for (mid, m) in entries {
                    let mname = m["name"].as_str().unwrap_or(mid);
                    flat.push(serde_json::json!({
                        "providerID": pid,
                        "providerName": pname,
                        "modelID": mid,
                        "modelName": mname,
                    }));
                }
            }
        }
    }
    let json = serde_json::Value::Array(flat).to_string();
    if let Ok(mut g) = models.lock() {
        *g = json;
    }
}

/// Scan system sound directories and build JSON `[{name,path}]` for QML selection
/// (SoundEffect plays only WAV).
fn refresh_sounds(sounds: &Arc<std::sync::Mutex<String>>) {
    let dirs = [
        "/usr/share/sounds/jolla-ambient/stereo",
        "/usr/share/sounds/jolla-ringtones/stereo",
        "/usr/share/sounds/freedesktop/stereo",
    ];
    let mut items: Vec<(String, String)> = Vec::new();
    for dir in dirs {
        let mut in_dir: Vec<(String, String)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if !e.path().is_file() {
                    continue;
                }
                let p = e.path();
                let ext = p
                    .extension()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if ext != "wav" {
                    continue;
                }
                let disp = p.display().to_string();
                let fname = p
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let pretty = fname
                    .trim_end_matches(".wav")
                    .replace('_', " ")
                    .replace('-', " ");
                let pretty = pretty
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if i == 0 { c.to_uppercase().collect::<String>() } else { c.to_string() }
                    })
                    .collect::<String>();
                in_dir.push((pretty, format!("file://{disp}")));
            }
        }
        in_dir.sort_by(|a, b| a.0.cmp(&b.0));
        items.extend(in_dir);
    }
    let arr: Vec<serde_json::Value> = items
        .iter()
        .map(|(n, p)| serde_json::json!({ "name": n, "path": p }))
        .collect();
    log::info!("найдено системных звуков: {}", arr.len());
    if let Ok(mut g) = sounds.lock() {
        *g = serde_json::Value::Array(arr).to_string();
    }
}

/// Render a v2 `MessageInfo[]` history (`GET /api/session/{id}/message`) into
/// the same feed lines the v1 `load_history` path emits: user bubbles, `[[think]]`
/// reasoning bubbles, answer text, inline tool rows and compaction notices.
/// `revert_id` hides the reverted tail (id >= boundary) exactly as in v1.
///
/// Returns the feed lines plus the *most recent* assistant model seen in the
/// history (`(providerID, modelID)`), which the caller stores so the
/// sessions/picker list can show a model for a session the server never told us
/// about (v2 session GET/list carry no `model`).
fn history_lines_v2(
    arr: &[serde_json::Value],
    revert_id: Option<&str>,
) -> (Vec<String>, Option<(String, String)>) {
    use opencode_client::types::v2::message::{CompactionStatus, Content, MessageBody, MessageInfo};
    let mut msgs: Vec<MessageInfo> = Vec::new();
    for raw in arr {
        match serde_json::from_value::<MessageInfo>(raw.clone()) {
            Ok(m) => msgs.push(m),
            Err(e) => log::warn!("history v2: message is unparseable ({e}) — skipped"),
        }
    }
    msgs.sort_by_key(|m| m.time.created);

    let mut lines: Vec<String> = Vec::new();
    let mut last_model: Option<(String, String)> = None;
    for m in &msgs {
        // Hide the reverted tail (mirror SessionRevert.cleanup boundaries).
        if let Some(r) = revert_id {
            if !m.id.is_empty() && m.id.as_bytes() >= r.as_bytes() {
                continue;
            }
        }
        let created = m.time.created;
        match &m.body {
            MessageBody::User { text, .. } => {
                if !text.trim().is_empty() {
                    lines.push(proto_user_line(created, &m.id, text));
                }
            }
            MessageBody::Assistant(a) => {
                if let Some(mdl) = &a.model {
                    last_model = Some((mdl.providerID.clone(), mdl.id.clone()));
                }
                // Reasoning first (mirrors the live stream), then answer, then tool rows.
                let reasoning_ts = created;
                let mut reasoning = String::new();
                let mut text = String::new();
                let mut tool_lines: Vec<String> = Vec::new();
                for c in &a.content {
                    match c {
                        Content::Text { text: t } => text.push_str(t),
                        Content::Reasoning { text: t, time } => {
                            let _ = time;
                            reasoning.push_str(t);
                        }
                        Content::Tool { id, name, state, .. } => {
                            if !id.is_empty() {
                                tool_lines.push(proto_tool_line(
                                    created,
                                    id,
                                    name,
                                    state.ui_status(),
                                    &state.description(),
                                ));
                            }
                        }
                        Content::Unknown => log::warn!(
                            "history v2: assistant content type is not implemented in the UI (skipped)"
                        ),
                    }
                }
                if !reasoning.trim().is_empty() {
                    let body = format!("{THINK_TAG}{}", reasoning.trim_end());
                    lines.push(proto_line(reasoning_ts, &body));
                }
                if !text.trim().is_empty() {
                    lines.push(proto_line(created, &text));
                }
                lines.extend(tool_lines);
            }
            MessageBody::Compaction { status, .. } => {
                // The summary text is the compacted history itself — render it
                // as a normal answer bubble (NOT just a generic "compacted"
                // marker), so the bubble that grew live during the compaction
                // does not vanish when this reload replaces the feed. `running`
                // has the growing text; `completed` the final summary.
                match status {
                    CompactionStatus::Running { summary, .. }
                    | CompactionStatus::Completed { summary, .. } => {
                        if !summary.trim().is_empty() {
                            lines.push(proto_line(created, summary.trim_end()));
                        }
                    }
                    // A failed compaction is not a "context compacted" marker: a
                    // benign "nothing to compact yet" carries only an error, so
                    // do not render a misleading success line. (The live feed
                    // already shows the informational notice when it happens.)
                    CompactionStatus::Failed { .. } => {}
                    CompactionStatus::Unknown => log::warn!(
                        "history v2: compaction status is not implemented in the UI (skipped)"
                    ),
                }
            }
            // Switches, synthetic/system/skill/shell/idle messages and unknown
            // kinds do not render in the feed (mirrors the v1 path skipping
            // non-user/assistant roles).
            _ => {}
        }
    }
    (lines, last_model)
}

/// Load session history (info/parts pairs) and push strings to QML queue.
async fn load_history(
    client: &impl api::OpenCodeApi,
    session_id: &str,
    pending: &Arc<std::sync::Mutex<Vec<String>>>,
    dispatcher: &event::dispatcher::SharedState,
) {
    // Revert boundary: messages with `id >= revert.messageID` are hidden. The
    // server does not flag them — it only records the boundary on the session
    // (SessionRevert.cleanup deletes the tail later, on the next prompt), so the
    // filter is ours. The presence of a boundary also drives the QML "Restore"
    // banner (control line `[[reverted]]1`).
    let revert_id = match client.get_session(session_id).await {
        Ok(s) => s[keys::REVERT][keys::MESSAGE_ID].as_str().map(str::to_string),
        Err(e) => {
            log::warn!("get_session (revert state): {e}");
            None
        }
    };
    if let Ok(mut q) = pending.lock() {
        q.push(format!(
            "{REVERTED_TAG}{}",
            if revert_id.is_some() { "1" } else { "0" }
        ));
    }
    match client.session_messages(session_id).await {
        Ok(v) => {
            let arr = v.as_array().cloned().unwrap_or_default();
            // v2 returns `MessageInfo[]` inline (no `{info, parts}` wrapper — the
            // v2 body carries its parts as `content[]`). Route it to the v2
            // renderer so history renders the same way as the v1 path does.
            let is_v2 = arr
                .first()
                .map(|e| e.get(keys::INFO).is_none() && e.get(keys::TYPE).is_some())
                .unwrap_or(false);
            if is_v2 {
                let (lines, model) = history_lines_v2(&arr, revert_id.as_deref());
                log::info!("история сессии {session_id} (v2): {} строк", lines.len());
                // Remember the model the history reveals (the source of truth on
                // v2, where session GET/list carry none) so the picker checkmark
                // works for a session whose model we never saw selected live.
                if let Some(m) = model {
                    dispatcher
                        .lock()
                        .await
                        .session_models
                        .insert(session_id.to_string(), m);
                }
                if let Ok(mut q) = pending.lock() {
                    q.extend(lines);
                }
                return;
            }
            let mut items: Vec<&serde_json::Value> = v.as_array().map(|a| a.iter().collect()).unwrap_or_default();
            items.sort_by_key(|e| {
                e[keys::INFO][keys::TIME][keys::CREATED]
                    .as_u64()
                    .unwrap_or(0)
            });
            let mut lines: Vec<String> = Vec::new();
            for e in items {
                let id = e[keys::INFO][keys::ID].as_str().unwrap_or("");
                // Hide the reverted tail (mirror SessionRevert.cleanup boundaries).
                if let Some(r) = &revert_id {
                    if !id.is_empty() && id.as_bytes() >= r.as_bytes() {
                        continue;
                    }
                }
                let role = e[keys::INFO][keys::ROLE].as_str().unwrap_or("");
                let created = e[keys::INFO][keys::TIME][keys::CREATED]
                    .as_u64()
                    .unwrap_or(0);
                let mut text = String::new();
                // Collect model reasoning separately: they come as parts with
                // `type:"reasoning"` and are shown in the feed as [[think]]
                // BEFORE the answer (same as live stream) — otherwise reasoning from history
                // would be lost after app restart.
                let mut reasoning = String::new();
                let mut reasoning_ts = created;
                // Inline tool calls (TUI-style): one feed line per callID, in
                // part order, so history matches the live stream.
                let mut tool_lines: Vec<String> = Vec::new();
                // Set when the message carries a `type:"compaction"` part (the
                // opencode compaction marker): the message has no text, so the
                // empty-message guard below must still keep its notice line.
                let mut has_compaction = false;
                if let Some(parts) = e[keys::PARTS].as_array() {
                    for p in parts {
                        match p[keys::TYPE].as_str() {
                            Some(keys::PART_TEXT) => {
                                if let Some(t) = p[keys::TEXT].as_str() {
                                    text.push_str(t);
                                }
                            }
                            Some(keys::PART_REASONING) => {
                                if let Some(t) = p[keys::TEXT].as_str() {
                                    if reasoning.is_empty() {
                                        reasoning_ts = p[keys::TIME][keys::START]
                                            .as_u64()
                                            .unwrap_or(created);
                                    }
                                    reasoning.push_str(t);
                                }
                            }
                            Some(keys::PART_TOOL) => {
                                let call = p[keys::CALL_ID].as_str().unwrap_or("");
                                let tool = p[keys::TOOL].as_str().unwrap_or("");
                                match serde_json::from_value::<ToolState>(p[keys::STATE].clone()) {
                                    Ok(st) if !call.is_empty() => tool_lines.push(proto_tool_line(
                                        created,
                                        call,
                                        tool,
                                        st.ui_status(),
                                        &st.description(),
                                    )),
                                    Ok(_) => log::warn!(
                                        "history: tool part without callID is not implemented (skipped)"
                                    ),
                                    Err(e) => log::warn!(
                                        "history: tool part state is unparseable ({e}) — skipped"
                                    ),
                                }
                            }
                            Some(keys::PART_COMPACTION) => {
                                // Compaction marker: the model's summary of the
                                // compressed history arrives as the following
                                // `summary:true` assistant message; the user
                                // message carrying this part only marks where the
                                // compaction happened. Show it as a service
                                // notice (mirrors the TUI's compaction row)
                                // instead of silently dropping it.
                                lines.push(svc_line(created, i18n::K_COMPACTED, &[]));
                                has_compaction = true;
                            }
                            other => log::warn!(
                                "history: part type {other:?} is not implemented in the UI (skipped)"
                            ),
                        }
                    }
                }
                if text.trim().is_empty() && tool_lines.is_empty() && !has_compaction {
                    continue;
                }
                if role == keys::ROLE_USER {
                    // A compaction-only user message has no body — the notice
                    // line was already pushed above, so skip the empty bubble.
                    if !text.trim().is_empty() {
                        // The messageID rides in the line so QML can offer revert/fork.
                        lines.push(proto_user_line(created, id, &text));
                    }
                } else {
                    if !reasoning.trim().is_empty() {
                        let body = format!("{THINK_TAG}{}", reasoning.trim_end());
                        lines.push(proto_line(reasoning_ts, &body));
                    }
                    // Answer = the text parts, exactly as opencode's TUI shows
                    // them (reasoning was already separated above by PART TYPE).
                    if !text.trim().is_empty() {
                        lines.push(proto_line(created, &text));
                    }
                    lines.extend(tool_lines);
                }
            }
            log::info!("история сессии {session_id}: {} строк", lines.len());
            if let Ok(mut q) = pending.lock() {
                q.extend(lines);
            }
        }
        Err(e) => log::error!("session_messages: {e}"),
    }
}

/// Thread-safely set server connection status for QML (no Qt signals).
fn set_server_status(status: &Arc<std::sync::Mutex<String>>, s: &markers::ServerConnectionStatus) {
    if let Ok(mut g) = status.lock() {
        *g = s.to_status_string();
    }
}

/// Publish current session ID for QML (`None` -> empty string).
fn set_current_session(shared: &Arc<std::sync::Mutex<String>>, session: &Option<String>) {
    if let Ok(mut g) = shared.lock() {
        *g = session.clone().unwrap_or_default();
    }
}

/// Request QML to switch carousel page (0..3) on next poll.
fn request_nav(nav: &Arc<std::sync::Mutex<i32>>, page: i32) {
    if let Ok(mut n) = nav.lock() {
        *n = page;
    }
}

/// File where the ID of the last opened session is stored (for auto-open on startup).
fn last_session_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = std::path::PathBuf::from(home).join(".local/share/harbour-opencode");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("last_session"))
}

/// Remember current session (`None` — clear record).
fn persist_last_session(session: Option<&str>) {
    let Some(path) = last_session_path() else { return };
    match session {
        Some(id) => {
            if let Err(e) = std::fs::write(&path, id) {
                log::error!("persist last_session: {e}");
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// On startup, open the last session: check existence, load history.
/// If there is no last session — create a new one (chat must not be empty).
async fn resume_last_session(
    client: &impl api::OpenCodeApi,
    current_session: &mut Option<String>,
    current_session_shared: &Arc<std::sync::Mutex<String>>,
    pending: &Arc<std::sync::Mutex<Vec<String>>>,
    todos: &Arc<std::sync::Mutex<String>>,
    dispatcher: &event::dispatcher::SharedState,
    sessions: &Arc<std::sync::Mutex<String>>,
) {
    let last: Option<String> = last_session_path()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Check if the remembered session still exists.
    let resume_id = match last {
        Some(id) => match client.list_sessions().await {
            Ok(v) => {
                let exists = v
                    .as_array()
                    .map(|a| a.iter().any(|s| s["id"].as_str() == Some(id.as_str())))
                    .unwrap_or(false);
                if exists {
                    Some(id)
                } else {
                    log::info!("последняя сессия {id} больше не существует, создаю новую");
                    None
                }
            }
            Err(e) => {
                log::error!("resume: list_sessions: {e}");
                None
            }
        },
        None => None,
    };

    // No last session (first run or after deleting all) — create a new one.
    let sid = match resume_id {
        Some(id) => id,
        None => match client.create_session(None).await {
            Ok(sess) => match sess["id"].as_str() {
                Some(id) => {
                    log::info!("создаю новую сессию: {id}");
                    id.to_string()
                }
                None => {
                    log::error!("new: в ответе нет session.id");
                    return;
                }
            },
            Err(e) => {
                log::error!("create_session: {e}");
                return;
            }
        },
    };

    log::info!("активная сессия: {sid}");
    *current_session = Some(sid.clone());
    persist_last_session(Some(&sid));
    set_current_session(current_session_shared, &*current_session);
    load_history(client, &sid, pending, dispatcher).await;
    ensure_session_model(client, &sid, dispatcher).await;
    // History may have revealed the session's model (v2): republish the list so
    // the sessions/picker show it right away.
    refresh_sessions(client, sessions, dispatcher).await;
    refresh_todo(client, &sid, todos).await;
}

/// Make sure `session_id` has a model bound, so the picker has something to
/// check. On v2 a session may have no `model` (the server only includes one once
/// a model is bound); when the history revealed none either, bind the server's
/// default model (`GET /api/model/default`). No-op when a model is already
/// known.
async fn ensure_session_model(
    client: &impl api::OpenCodeApi,
    session_id: &str,
    dispatcher: &event::dispatcher::SharedState,
) {
    if dispatcher
        .lock()
        .await
        .session_models
        .contains_key(session_id)
    {
        return;
    }
    let default = match client.default_model().await {
        Ok(m) => m,
        Err(e) => {
            log::warn!("default_model: {e}");
            None
        }
    };
    let Some((provider, model)) = default else {
        return;
    };
    match client.set_model(session_id, &provider, &model).await {
        Ok(()) => {
            log::info!("bound default model {provider}/{model} to {session_id}");
            dispatcher
                .lock()
                .await
                .session_models
                .insert(session_id.to_string(), (provider, model));
        }
        Err(e) => log::warn!("bind default model: {e}"),
    }
}

/// Reset tools snapshot (when switching sessions so old tools don't mix into new history).
async fn clear_tools(
    dispatcher: &event::dispatcher::SharedState,
    tools: &Arc<std::sync::Mutex<String>>,
) {
    {
        let mut st = dispatcher.lock().await;
        st.tools.clear();
    }
    if let Ok(mut g) = tools.lock() {
        *g = "[]".to_string();
    }
}

/// Snapshot of active tools for chat feed.
async fn refresh_tools_json(
    dispatcher: &event::dispatcher::SharedState,
    tools: &Arc<std::sync::Mutex<String>>,
) {
    let st = dispatcher.lock().await;
    let arr: Vec<serde_json::Value> = st
        .tools
        .order
        .iter()
        .filter_map(|call_id| {
            let e = st.tools.states.get(call_id)?;
            Some(serde_json::json!({
                "id": call_id,
                "name": e.name,
                "status": e.state.ui_status(),
                "text": e.state.description(),
            }))
        })
        .collect();
    drop(st);
    let tj = serde_json::Value::Array(arr).to_string();
    if let Ok(mut g) = tools.lock() {
        *g = tj;
    }
}

/// Store the last failed permission answer for the UI (empty — no error).
fn set_permission_error(buf: &Arc<std::sync::Mutex<String>>, msg: &str) {
    if let Ok(mut g) = buf.lock() {
        *g = msg.to_string();
    }
}

/// Snapshot of permission requests for the dialog.
///
/// Each request keeps the server's own shape (`{id, sessionID, action,
/// resources, message, save, source}`) plus the answer options — the dialog
/// shows what is asked (`message`/`resources`) and who asked (`source`).
async fn refresh_permissions_json(
    dispatcher: &event::dispatcher::SharedState,
    permissions: &Arc<std::sync::Mutex<String>>,
) {
    let st = dispatcher.lock().await;
    let pending = &st.permissions.pending;
    let options: Vec<serde_json::Value> = [
        ("Разрешить один раз", PermissionReplyKind::Once),
        ("Всегда разрешать", PermissionReplyKind::Always),
        ("Запретить", PermissionReplyKind::Reject),
    ]
    .iter()
    .map(|(label, kind)| serde_json::json!({ "label": label, "value": kind }))
    .collect();
    let arr: Vec<serde_json::Value> = pending
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "sessionID": p.session_id,
                "action": p.action,
                "resources": p.resources,
                "message": p.message.as_deref().unwrap_or_default(),
                "save": p.save.clone().unwrap_or_default(),
                "source": p.source.as_ref().map(|s| serde_json::json!({
                    "type": s.kind,
                    "messageID": s.message_id,
                    "id": s.id,
                })),
                "options": options,
            })
        })
        .collect();
    drop(st);
    let pj = serde_json::Value::Array(arr).to_string();
    if let Ok(mut g) = permissions.lock() {
        *g = pj;
    }
}

/// Reconcile the permission queue with the server.
///
/// The SSE `permission.asked` event is the fast path; this poll is the safety
/// net (a request created while the client was offline, a lost SSE frame, or a
/// request the server already settled — the ghost card). The server's list is
/// authoritative: the queue is replaced with it, the current session's request
/// first.
async fn refresh_permissions(
    client: &impl api::OpenCodeApi,
    dispatcher: &event::dispatcher::SharedState,
    permissions: &Arc<std::sync::Mutex<String>>,
    current_session: Option<&str>,
) {
    let mut list = match client.list_permissions().await {
        Ok(Some(l)) => l,
        Ok(None) => return,
        Err(e) => {
            log::warn!("list_permissions: {e}");
            // A server without the location route: ask the current session.
            let sid = match current_session {
                Some(s) => s,
                None => return,
            };
            match client.list_session_permissions(sid).await {
                Ok(Some(l)) => l,
                _ => return,
            }
        }
    };
    // The request of the session the user is looking at comes first.
    if let Some(sid) = current_session {
        if let Some(idx) = list.iter().position(|p| p.session_id == sid) {
            let p = list.remove(idx);
            list.insert(0, p);
        }
    }
    {
        let mut st = dispatcher.lock().await;
        st.permissions.replace(list);
    }
    refresh_permissions_json(dispatcher, permissions).await;
}

/// Snapshot of the saved ("always allow") permission rules.
async fn refresh_saved_permissions_json(
    client: &impl api::OpenCodeApi,
    saved: &Arc<std::sync::Mutex<String>>,
) {
    let list = match client.list_saved_permissions().await {
        Ok(Some(l)) => l,
        Ok(None) => return,
        Err(e) => {
            log::warn!("list_saved_permissions: {e}");
            return;
        }
    };
    let arr: Vec<serde_json::Value> = list
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "action": p.action,
                "resource": p.resource,
                "projectID": p.project_id,
            })
        })
        .collect();
    let sj = serde_json::Value::Array(arr).to_string();
    if let Ok(mut g) = saved.lock() {
        *g = sj;
    }
}

/// Snapshot of the pending forms (opencode v2) for the form dialog.
///
/// The forms go to QML in the server's own shape (`Form.Info`), so the QML
/// side reads the field list as it arrived (no re-mapping, no lost fields).
async fn refresh_forms_json(
    dispatcher: &event::dispatcher::SharedState,
    forms: &Arc<std::sync::Mutex<String>>,
) {
    let st = dispatcher.lock().await;
    let arr: Vec<serde_json::Value> = st
        .forms
        .pending
        .iter()
        .filter_map(|f| serde_json::to_value(f).ok())
        .collect();
    drop(st);
    let fj = serde_json::Value::Array(arr).to_string();
    if let Ok(mut g) = forms.lock() {
        *g = fj;
    }
}

/// Reconcile the form queue with the server.
///
/// The SSE `form.created` event is the fast path; this poll is the safety
/// net (a form created while the client was offline, or a form settled
/// elsewhere). The server's list is authoritative: the queue is replaced with
/// it, the current session's form first.
async fn refresh_forms(
    client: &impl api::OpenCodeApi,
    dispatcher: &event::dispatcher::SharedState,
    forms: &Arc<std::sync::Mutex<String>>,
    current_session: Option<&str>,
) {
    let mut list = match client.list_forms().await {
        Ok(Some(l)) => l,
        Ok(None) => return,
        Err(e) => {
            log::warn!("list_forms: {e}");
            // A server without the global route: ask the current session.
            let sid = match current_session {
                Some(s) => s,
                None => return,
            };
            match client.list_session_forms(sid).await {
                Ok(Some(l)) => l,
                _ => return,
            }
        }
    };
    // The form of the session the user is looking at comes first.
    if let Some(sid) = current_session {
        if let Some(idx) = list.iter().position(|f| f.session_id == sid) {
            let f = list.remove(idx);
            list.insert(0, f);
        }
    }
    {
        let mut st = dispatcher.lock().await;
        st.forms.replace(list);
    }
    refresh_forms_json(dispatcher, forms).await;
}

