use qmetaobject::*;
use std::sync::{Arc, Mutex};

use crate::cmd::Cmd;
use crate::i18n::{err_line, now_ms};
use crate::settings::Settings;
use crate::types::event::PermissionReplyKind;

/// Diagnostic marker written by the QML main thread: which poll() phase is
/// running right now. `t` is `now_ms()` at the moment of the last write.
#[derive(Default, Clone)]
pub struct GuiMark {
    pub s: String,
    pub t: u64,
}

/// Bridge Rust-core ←→ QML.
///
/// Threads: the QML engine lives in the main thread and calls the `drain_messages`
/// / `send_prompt` methods. The tokio worker (SSE) concurrently writes to `pending_messages`
/// via `Arc<Mutex<Vec>>`. Qt signals are not emitted from other threads.
#[derive(Default, QObject)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),
    /// Current status: "idle" | "busy" | "connecting" | "error".
    session_status: qt_property!(String; NOTIFY session_status_changed),
    session_status_changed: qt_signal!(),
    last_text: qt_property!(String; NOTIFY last_text_changed),
    last_text_changed: qt_signal!(),
    message_received: qt_signal!(message: String),

    /// Queue of ready strings for QML (worker → QML).
    #[allow(dead_code)]
    pending_messages: Arc<Mutex<Vec<String>>>,
    /// Queue of outgoing prompts (QML → worker).
    #[allow(dead_code)]
    outgoing_prompts: Arc<Mutex<Vec<String>>>,
    /// Status written by the worker (non-Qt thread) and read by QML via polling.
    /// We don't emit Qt signals from foreign threads — hence a separate Arc.
    #[allow(dead_code)]
    status_shared: Arc<Mutex<String>>,
    /// Session list (JSON `[{id,title,updated}]`), written by the worker, read by QML.
    #[allow(dead_code)]
    sessions_shared: Arc<Mutex<String>>,
    /// Commands from QML to the worker: `"new"` | `"open:<sessionID>"`.
    #[allow(dead_code)]
    commands_shared: Arc<Mutex<Vec<String>>>,
    /// id of the current session (written by the worker, read by QML). Empty string — no session.
    #[allow(dead_code)]
    current_session_shared: Arc<Mutex<String>>,
    /// True while a compaction of the shown session runs (written by the worker,
    /// polled by QML to show the activity banner). `POST /summarize` blocks the
    /// worker for the whole compaction (~20 s), so the only reliable progress
    /// signal is this flag — set the instant the user asks, cleared on
    /// `session.compacted`/idle.
    #[allow(dead_code)]
    compacting_shared: Arc<Mutex<bool>>,
    /// QML request to change the carousel page (`take_nav` takes and clears it; <0 — none).
    #[allow(dead_code)]
    nav_shared: Arc<Mutex<i32>>,
    /// TODO tasks of the current session (JSON), written by the worker, read by QML.
    #[allow(dead_code)]
    todo_shared: Arc<Mutex<String>>,
    /// Flat model list (JSON), written by the worker, read by QML.
    #[allow(dead_code)]
    models_shared: Arc<Mutex<String>>,
    /// List of available system sounds (JSON `[{name,path}]`), written by the worker, read by QML.
    #[allow(dead_code)]
    sounds_shared: Arc<Mutex<String>>,
    /// JSON status of the voice module (model catalog + progress), written by the worker.
    #[allow(dead_code)]
    voice_status_shared: Arc<Mutex<String>>,
    /// Snapshot of active tools for the chat feed (JSON), written by the worker.
    #[allow(dead_code)]
    tools_shared: Arc<Mutex<String>>,
    /// Queue of permission requests for the dialog (JSON), written by the worker.
    #[allow(dead_code)]
    permissions_shared: Arc<Mutex<String>>,
    /// Pending model questions for the dialog (JSON), written by the worker.
    #[allow(dead_code)]
    questions_shared: Arc<Mutex<String>>,
    /// JSON settings status (directory handling, etc.), written by the worker/backend.
    #[allow(dead_code)]
    settings_shared: Arc<Mutex<String>>,
    /// QML poll() progress marker (diagnostics), written by the main thread.
    #[allow(dead_code)]
    gui_mark_shared: Arc<Mutex<GuiMark>>,
    /// OpenCode Console device-login state/child (shared with the worker).
    #[allow(dead_code)]
    console_login: crate::auth::ConsoleLogin,

    /// QML: take and clear accumulated messages (polling).
    /// We separate elements with the Record Separator control character (`\x1e`), not
    /// `\n`: AI response texts may contain newlines that with
    /// `join("/`split`("\n")` would tear the message into pieces in the feed.
    drain_messages: qt_method!(fn drain_messages(&self) -> QString {
        const RS: char = '\u{1e}';
        let out = self
            .pending_messages
            .lock()
            .map(|mut q| std::mem::take(&mut *q).join(&RS.to_string()))
            .unwrap_or_default();
        QString::from(out)
    }),
    /// QML: accept a prompt from the user.
    send_prompt: qt_method!(fn send_prompt(&self, text: QString) {
        let t = text.to_string().trim().to_string();
        log::info!("QML send_prompt -> {t:?}");
        if !t.is_empty() {
            if let Ok(mut q) = self.outgoing_prompts.lock() {
                q.push(t);
            }
        }
    }),
    /// QML: record where poll() is right now (diagnostics for GUI stalls).
    gui_mark: qt_method!(fn gui_mark(&self, m: QString) {
        if let Ok(mut g) = self.gui_mark_shared.lock() {
            g.s = m.to_string();
            g.t = now_ms();
        }
    }),
    /// QML: current status (read-only, updated by the worker).
    status_text: qt_method!(fn status_text(&self) -> QString {
        let v = self.status_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: session list as JSON (read-only, updated by the worker).
    sessions_json: qt_method!(fn sessions_json(&self) -> QString {
        let v = self.sessions_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: create a new session. `title` is optional — an empty string lets
    /// the server generate the session title automatically.
    new_session: qt_method!(fn new_session(&self, title: QString) {
        let title = title.to_string();
        log::info!("QML new_session title={title:?}");
        let title = if title.trim().is_empty() { None } else { Some(title) };
        self.push_command(Cmd::New { title }.to_value());
    }),
    /// QML: open a session by id (loads the history).
    open_session: qt_method!(fn open_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML open_session -> {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Open { id }.to_value());
        }
    }),
    /// QML: rename a session.
    rename_session: qt_method!(fn rename_session(&self, id: QString, title: QString) {
        let id = id.to_string();
        let title = title.to_string();
        log::info!("QML rename_session {id:?} -> {title:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Rename { id, title }.to_value());
        }
    }),
    /// QML: delete a session.
    delete_session: qt_method!(fn delete_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML delete_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Delete { id }.to_value());
        }
    }),
    /// QML: delete all sessions.
    delete_all_sessions: qt_method!(fn delete_all_sessions(&self) {
        log::info!("QML delete_all_sessions");
        self.push_command(Cmd::DeleteAll.to_value());
    }),
    /// QML: fork a session (create a branch).
    fork_session: qt_method!(fn fork_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML fork_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Fork { id, message: None }.to_value());
        }
    }),
    /// QML: fork a session at a message (branch from that point).
    fork_from: qt_method!(fn fork_from(&self, id: QString, message: QString) {
        let id = id.to_string();
        let message = message.to_string();
        log::info!("QML fork_from {id:?} @ {message:?}");
        if !id.is_empty() && !message.is_empty() {
            self.push_command(Cmd::Fork { id, message: Some(message) }.to_value());
        }
    }),
    /// QML: revert the session tail starting at a message.
    revert_to: qt_method!(fn revert_to(&self, id: QString, message: QString) {
        let id = id.to_string();
        let message = message.to_string();
        log::info!("QML revert_to {id:?} @ {message:?}");
        if !id.is_empty() && !message.is_empty() {
            self.push_command(Cmd::Revert { id, message }.to_value());
        }
    }),
    /// QML: restore a previously reverted tail.
    unrevert_session: qt_method!(fn unrevert_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML unrevert_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Unrevert { id }.to_value());
        }
    }),
    /// QML: share a session (get a link).
    share_session: qt_method!(fn share_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML share_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Share { id }.to_value());
        }
    }),
    /// QML: revoke link access.
    unshare_session: qt_method!(fn unshare_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML unshare_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Unshare { id }.to_value());
        }
    }),
    /// QML: summarize (compact) the session history.
    summarize_session: qt_method!(fn summarize_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML summarize_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Summarize { id }.to_value());
        }
    }),
    /// QML: abort execution in the session.
    abort_session: qt_method!(fn abort_session(&self, id: QString) {
        let id = id.to_string();
        log::info!("QML abort_session {id:?}");
        if !id.is_empty() {
            self.push_command(Cmd::Abort { id }.to_value());
        }
    }),
    /// QML: id of the current session (read-only, updated by the worker).
    current_session_id: qt_method!(fn current_session_id(&self) -> QString {
        let v = self.current_session_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: true while the shown session is being compacted (polled by the
    /// activity banner in ChatPage).
    compaction_active: qt_method!(fn compaction_active(&self) -> bool {
        self.compacting_shared.lock().map(|g| *g).unwrap_or(false)
    }),
    /// QML: TODO tasks of the current session as JSON (read-only).
    todo_json: qt_method!(fn todo_json(&self) -> QString {
        let v = self.todo_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: flat model list as JSON (read-only).
    models_json: qt_method!(fn models_json(&self) -> QString {
        let v = self.models_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: list of available system sounds as JSON (read-only).
    sounds_json: qt_method!(fn sounds_json(&self) -> QString {
        let v = self.sounds_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: voice module status as JSON (read-only).
    voice_status_json: qt_method!(fn voice_status_json(&self) -> QString {
        let v = self.voice_status_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: switch the session model.
    set_model: qt_method!(fn set_model(&self, id: QString, provider: QString, model: QString) {
        let id = id.to_string();
        let provider = provider.to_string();
        let model = model.to_string();
        log::info!("QML set_model {id:?} -> {provider}/{model}");
        if !id.is_empty() && !provider.is_empty() && !model.is_empty() {
            self.push_command(Cmd::SetModel { id, provider, model }.to_value());
        }
    }),
    /// QML: switch the session mode/agent (Build | Plan).
    set_mode: qt_method!(fn set_mode(&self, id: QString, mode: QString) {
        let id = id.to_string();
        let mode = mode.to_string();
        log::info!("QML set_mode {id:?} -> {mode:?}");
        if !id.is_empty() {
            self.push_command(Cmd::SetMode { id, mode }.to_value());
        }
    }),
    /// QML: take the requested carousel page (or -1).
    take_nav: qt_method!(fn take_nav(&self) -> i32 {
        self.nav_shared
            .lock()
            .map(|mut n| {
                let v = *n;
                *n = -1;
                v
            })
            .unwrap_or(-1)
    }),
    /// QML: voice command (cmd + payload: model id / lang). Async — worker.
    /// The string from QML is validated via `Cmd` — unknown values are discarded.
    voice_command: qt_method!(fn voice_command(&self, cmd: QString, val: QString) {
        let name = cmd.to_string();
        let val = val.to_string();
        match Cmd::from_name_id(&name, &val) {
            Some(action) if action.is_voice() => {
                log::info!("QML voice {action:?} {val:?}");
                self.push_command(action.to_value());
            }
            Some(action) => log::warn!("QML voice: не голосовая команда {action:?}"),
            None => log::warn!("QML voice: неизвестная команда {name:?}"),
        }
    }),
    /// QML: active tools of the current turn as JSON `[{id,name,status,text}]`.
    tools_json: qt_method!(fn tools_json(&self) -> QString {
        let v = self.tools_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: permission requests as JSON `[{id,sessionID,action,resources,options}]`.
    permissions_json: qt_method!(fn permissions_json(&self) -> QString {
        let v = self.permissions_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: pending questions from the model as JSON (read-only).
    questions_json: qt_method!(fn questions_json(&self) -> QString {
        let v = self.questions_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: answer a question (`id` = requestID). `answers` is JSON
    /// `[[label,…],…]` in question order; empty string means "reject".
    answer_question: qt_method!(fn answer_question(&self, id: QString, answers: QString) {
        let id = id.to_string();
        let answers = answers.to_string();
        log::info!("QML answer_question {id:?}");
        if id.is_empty() {
            return;
        }
        if answers.is_empty() {
            self.push_command(Cmd::Question { id, answers: Vec::new(), reject: true }.to_value());
        } else {
            let parsed: Vec<Vec<String>> = serde_json::from_str(&answers).unwrap_or_default();
            self.push_command(Cmd::Question { id, answers: parsed, reject: false }.to_value());
        }
    }),
    /// QML: app settings as JSON `{"workdir":"...","..."}`.
    settings_json: qt_method!(fn settings_json(&self) -> QString {
        let v = self.settings_shared.lock().map(|s| s.clone()).unwrap_or_default();
        QString::from(v)
    }),
    /// QML: request current settings.
    get_settings: qt_method!(fn get_settings(&self) {
        log::info!("QML get_settings");
        self.push_command(Cmd::GetSettings.to_value());
    }),
    /// QML: save settings (JSON settings object).
    save_settings: qt_method!(fn save_settings(&self, json: QString) {
        let json = json.to_string();
        log::info!("QML save_settings -> {json:?}");
        if !json.is_empty() {
            match serde_json::from_str::<Settings>(&json) {
                Ok(value) => self.push_command(Cmd::SaveSettings { value }.to_value()),
                Err(e) => log::warn!("QML save_settings: неверный JSON: {e}"),
            }
        }
    }),
    /// QML: set the working directory (path string).
    set_workdir: qt_method!(fn set_workdir(&self, path: QString) {
        let path = path.to_string();
        log::info!("QML set_workdir -> {path:?}");
        self.push_command(Cmd::SetWorkdir { workdir: path }.to_value());
    }),
    /// QML: URL of the OpenCode API-key page (for `Qt.openUrlExternally`).
    auth_url: qt_method!(fn auth_url(&self) -> QString {
        QString::from(crate::auth::OPENCODE_AUTH_URL)
    }),
    /// QML: whether an OpenCode provider key is already stored on the device.
    provider_key_set: qt_method!(fn provider_key_set(&self) -> bool {
        crate::auth::has_api_key(crate::auth::OPENCODE_PROVIDER)
    }),
    /// QML: store the OpenCode provider API key (via the server's PUT /auth).
    save_provider_key: qt_method!(fn save_provider_key(&self, key: QString) {
        let key = key.to_string().trim().to_string();
        log::info!("QML save_provider_key (len={})", key.len());
        if !key.is_empty() {
            self.push_command(
                Cmd::SetProviderKey {
                    provider: crate::auth::OPENCODE_PROVIDER.to_string(),
                    key,
                }
                .to_value(),
            );
        }
    }),
    /// QML: current OpenCode Console device-login state as JSON
    /// (`{state,url,code,message}`).
    console_login_json: qt_method!(fn console_login_json(&self) -> QString {
        QString::from(self.console_login.status_json())
    }),
    /// QML: start the device login (`opencode console login`).
    start_console_login: qt_method!(fn start_console_login(&self) {
        log::info!("QML start_console_login");
        self.push_command(Cmd::ConsoleLogin.to_value());
    }),
    /// QML: cancel an in-progress device login.
    cancel_console_login: qt_method!(fn cancel_console_login(&self) {
        log::info!("QML cancel_console_login");
        self.push_command(Cmd::ConsoleLoginCancel.to_value());
    }),
    /// QML: answer a permission request (`response`: once | always | reject).
    answer_permission: qt_method!(fn answer_permission(&self, session: QString, id: QString, response: QString) {
        let session = session.to_string();
        let id = id.to_string();
        let response = response.to_string();
        log::info!("QML answer_permission {session:?} {id:?} -> {response:?}");
        if !session.is_empty() && !id.is_empty() && !response.is_empty() {
            match PermissionReplyKind::from_name(&response) {
                Some(response) => {
                    self.push_command(Cmd::Permission { session, id, response }.to_value());
                }
                None => log::warn!("QML answer_permission: неизвестный ответ {response:?}"),
            }
        }
    }),
    /// QML (dev): generate a mock server error into the feed, as if a
    /// `session.error` with the corresponding type arrived. Useful for debugging the GUI without
    /// a real server. We put the lines through the same `human_session_error`
    /// as real events, so the bubble format is guaranteed to match.
    /// Available only in debug builds (`cargo build` without `--release`).
    #[cfg(debug_assertions)]
    mock_error: qt_method!(fn mock_error(&self, kind: QString) {
        let kind = kind.to_string();
        let payload = match kind.as_str() {
            "ProviderAuthError" => serde_json::json!({
                "name": "ProviderAuthError",
                "data": { "providerID": "anthropic", "message": "No API key found" }
            }),
            "APIError401" => serde_json::json!({
                "name": "APIError",
                "data": { "message": "Unauthorized", "statusCode": 401, "isRetryable": false }
            }),
            "APIError429" => serde_json::json!({
                "name": "APIError",
                "data": {
                    "message": "Rate limit exceeded",
                    "statusCode": 429,
                    "isRetryable": true,
                    "responseBody": "Free usage exceeded. Resets at 14:32 UTC"
                }
            }),
            "MessageOutputLengthError" => serde_json::json!({
                "name": "MessageOutputLengthError",
                "data": {}
            }),
            "MessageAbortedError" => serde_json::json!({
                "name": "MessageAbortedError",
                "data": { "message": "Response aborted by user" }
            }),
            "UnknownError" => serde_json::json!({
                "name": "UnknownError",
                "data": { "message": "Unexpected upstream failure" }
            }),
            other => {
                log::warn!("QML mock_error: неизвестный тип {other:?}");
                return;
            }
        };
        let msg = crate::types::event::human_session_error(Some(&payload));
        log::info!("QML mock_error {kind:?} -> {msg:?}");
        if let Ok(mut q) = self.pending_messages.lock() {
            q.push(err_line(now_ms(), crate::i18n::K_SERVER_ERROR, &[msg]));
        }
    }),
    /// QML (dev): generate a mock access request, as if the agent requested
    /// permission. Goes through the worker (Cmd::MockPermission) so the permission
    /// lands in the dispatcher queue and isn't overwritten by the periodic snapshot.
    /// Available only in debug builds.
    #[cfg(debug_assertions)]
    mock_permission: qt_method!(fn mock_permission(&self) {
        log::info!("QML mock_permission");
        self.push_command(Cmd::MockPermission.to_value());
    }),
    /// QML (dev): whether this is a debug build (true = mock buttons available).
    // The method always exists so the result can be read in release too
    // (there it's just false); the mock methods themselves are absent in release.
    dev_tools: qt_method!(fn dev_tools(&self) -> bool {
        cfg!(debug_assertions)
    }),
}

/// Access from Rust tasks (not from QML, safe from any thread).
impl AppBridge {
    /// Main thread: update status + QML signal.
    pub fn set_session_status_rs(&mut self, s: String) {
        self.session_status = s.into();
        self.session_status_changed();
    }

    /// Worker (non-Qt thread): push a string onto the queue for QML.
    pub fn push_message(&self, text: &str) {
        if let Ok(mut q) = self.pending_messages.lock() {
            q.push(text.to_string());
        }
    }

    /// Worker: take outgoing prompts from QML.
    pub fn take_prompts(&self) -> Vec<String> {
        self.outgoing_prompts.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }

    /// Clones of the queues for passing to the tokio worker (Send + 'static).
    pub fn queues(&self) -> (Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<String>>>) {
        (self.pending_messages.clone(), self.outgoing_prompts.clone())
    }

    /// Status handle for the worker (can be written from any thread).
    pub fn status_handle(&self) -> Arc<Mutex<String>> {
        self.status_shared.clone()
    }

    /// Thread-safely set the status (without Qt signals).
    pub fn set_status_shared(&self, s: &str) {
        if let Ok(mut g) = self.status_shared.lock() {
            *g = s.to_string();
        }
    }

    /// Session list handle for the worker.
    pub fn sessions_handle(&self) -> Arc<Mutex<String>> {
        self.sessions_shared.clone()
    }

    /// Command queue handle for the worker.
    pub fn commands_handle(&self) -> Arc<Mutex<Vec<String>>> {
        self.commands_shared.clone()
    }

    /// Current session id handle for the worker.
    pub fn current_session_handle(&self) -> Arc<Mutex<String>> {
        self.current_session_shared.clone()
    }

    /// Compaction-in-progress flag handle for the worker.
    pub fn compacting_handle(&self) -> Arc<Mutex<bool>> {
        self.compacting_shared.clone()
    }

    /// Navigation request handle for the worker.
    pub fn nav_handle(&self) -> Arc<Mutex<i32>> {
        self.nav_shared.clone()
    }

    /// TODO tasks handle for the worker.
    pub fn todo_handle(&self) -> Arc<Mutex<String>> {
        self.todo_shared.clone()
    }

    /// Model list handle for the worker.
    pub fn models_handle(&self) -> Arc<Mutex<String>> {
        self.models_shared.clone()
    }

    /// System sound list handle for the worker.
    pub fn sounds_handle(&self) -> Arc<Mutex<String>> {
        self.sounds_shared.clone()
    }

    /// Voice module status handle for the worker.
    pub fn voice_status_handle(&self) -> Arc<Mutex<String>> {
        self.voice_status_shared.clone()
    }

    /// Tools snapshot handle for the worker.
    pub fn tools_handle(&self) -> Arc<Mutex<String>> {
        self.tools_shared.clone()
    }

    /// Permission queue handle for the worker.
    pub fn permissions_handle(&self) -> Arc<Mutex<String>> {
        self.permissions_shared.clone()
    }

    /// Question queue handle for the worker.
    pub fn questions_handle(&self) -> Arc<Mutex<String>> {
        self.questions_shared.clone()
    }

    /// Settings handle for the worker.
    pub fn settings_handle(&self) -> Arc<Mutex<String>> {
        self.settings_shared.clone()
    }

    /// Diagnostic GUI-mark handle for the dumper thread in main().
    pub fn gui_mark_handle(&self) -> Arc<Mutex<GuiMark>> {
        self.gui_mark_shared.clone()
    }

    /// Device-login handle for the worker (start/cancel/read status).
    pub fn console_login_handle(&self) -> crate::auth::ConsoleLogin {
        self.console_login.clone()
    }

    /// Push a JSON command onto the queue for the worker.
    fn push_command(&self, v: serde_json::Value) {
        if let Ok(mut q) = self.commands_shared.lock() {
            q.push(v.to_string());
        }
    }
}
