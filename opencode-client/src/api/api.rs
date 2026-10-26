//! Protocol adapter boundary.
//!
//! `OpenCodeApi` is the single interface the worker stack (worker loop, SSE
//! dispatcher, history loader) talks to: the only server protocol is opencode
//! 2.x (`V2Adapter`, see `vault/План-v2.md`), and this trait keeps the HTTP
//! surface out of the worker loop.
//!
//! Method set is deliberately limited to what consumers actually call — it is
//! not a 1:1 mirror of the HTTP surface.
//!
//! Signatures return `impl Future<…> + Send` (RPITIT, stable since Rust 1.75)
//! rather than plain `async fn`. The `Send` bound is required: the worker spawns
//! a blocking call (`compact`) onto the runtime, and `tokio::spawn` needs a
//! `Send` future — which `async fn` in a trait does not imply. The trait is not
//! object-safe by design: consumers are generic over `A: OpenCodeApi` and use
//! static dispatch, so there is no runtime cost and no extra dependency.

use std::future::Future;
use std::pin::Pin;

use futures_util::Stream;

use crate::types::v2::form::FormInfo;
use crate::types::v2::permission::{PermissionReplyKind, PermissionRequest, SavedPermission};
use crate::types::v2::EventEnvelope;

/// Server protocol adapter (opencode 2.x).
pub trait OpenCodeApi: Clone + Send + Sync + 'static {
    /// Open the server's event stream: `GET /api/event`, yielding
    /// [`EventEnvelope`] frames.
    fn event_stream(
        &self,
    ) -> impl Future<
        Output = Result<Pin<Box<dyn Stream<Item = Result<EventEnvelope, String>> + Send>>, String>,
    > + Send;

    /// GET /api/info — is the server up?
    fn health(&self) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /api/location/reload — reload the location, releasing resources so
    /// the next request reloads config (e.g. a freshly written credential).
    fn global_dispose(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// POST /session — create a session. Empty/absent title → server names it.
    fn create_session(
        &self,
        title: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// GET /session — session list.
    fn list_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// GET /api/session/active — sessions the server is currently running,
    /// keyed by session id (`{data: {"ses…": {type: "running"}}}`). The
    /// authoritative answer to "is this session working", used to repair the
    /// turn state on (re)connect.
    fn active_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// PATCH /session/{id} — rename a session.
    fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// DELETE /session/{id} — delete a session.
    fn delete_session(&self, session_id: &str) -> impl Future<Output = Result<(), String>> + Send;

    /// GET /session/{id} — a single session.
    fn get_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// POST /session/{id}/fork — branch a session (optionally cut at a message).
    fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// POST /session/{id}/revert — hide the tail starting at `message_id`.
    fn revert_session(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// POST /session/{id}/unrevert — restore previously reverted messages.
    fn unrevert_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// POST /session/{id}/summarize — compact history for the given model.
    fn summarize_session(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// GET /config/providers — provider/model catalog.
    fn providers(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// GET /session/{id}/todo — the agent's task checklist.
    fn session_todo(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// POST /api/session/{id}/model — switch the session model.
    fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// POST /api/session/{id}/agent — switch the session agent (build/plan).
    fn set_session_agent(
        &self,
        session_id: &str,
        agent: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// The server's default model `(providerID, modelID)`, if the protocol
    /// exposes one (`GET /api/model/default`). Returns `None` when the answer
    /// carries no complete pair (the caller keeps whatever the session says).
    fn default_model(&self) -> impl Future<Output = Result<Option<(String, String)>, String>> + Send {
        async { Ok(None) }
    }

    /// GET /session/{id}/message — full history as `[{info, parts}]`.
    fn session_messages(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// POST /session/{id}/prompt — send a prompt without waiting.
    ///
    /// Returns the ID the server assigned to the user message when the response
    /// reports one (`POST .../prompt` answers `{data: SessionInboxUser.id}`), so
    /// the caller never has to guess it via SSE; `None` means the answer carried
    /// no id (a server anomaly worth logging at the call site).
    fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send;

    /// POST /session/{id}/abort — abort execution.
    fn abort(&self, session_id: &str) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /api/session/{sessionID}/permission/{requestID}/reply — answer a
    /// permission request. The body is `{decision, message?}` (v1 used
    /// `{response}`); `message` is the optional reason shown to the model.
    fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &PermissionReplyKind,
        message: Option<&str>,
    ) -> impl Future<Output = Result<bool, String>> + Send;

    /// GET /api/permission/request — the pending permission requests of the
    /// server's location (all sessions). Used to reconcile the queue with the
    /// server (the poll); `None` means "the server did not tell" (keep the
    /// queue as it is).
    fn list_permissions(&self) -> impl Future<Output = Result<Option<Vec<PermissionRequest>>, String>> + Send;

    /// GET /api/session/{sessionID}/permission — the pending permission
    /// requests of one session. A fallback for `list_permissions` (a server
    /// without the location route).
    fn list_session_permissions(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<Vec<PermissionRequest>>, String>> + Send;

    /// GET /api/permission/saved — the stored "always allow" rules.
    fn list_saved_permissions(
        &self,
    ) -> impl Future<Output = Result<Option<Vec<SavedPermission>>, String>> + Send;

    /// DELETE /api/permission/saved/{id} — revoke a stored rule.
    fn remove_saved_permission(&self, id: &str) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /api/session/{sessionID}/form/{formID}/reply — answer a form from
    /// the model. `answer` is the `{fieldKey: value}` map (see `Form.Answer`).
    ///
    /// The route is session-scoped: the form id alone is not enough.
    fn reply_form(
        &self,
        session_id: &str,
        form_id: &str,
        answer: serde_json::Value,
    ) -> impl Future<Output = Result<bool, String>> + Send;

    /// DELETE /api/session/{sessionID}/form/{formID} — cancel a form (the
    /// model gets `message` as the reason, if any).
    fn cancel_form(
        &self,
        session_id: &str,
        form_id: &str,
        message: Option<&str>,
    ) -> impl Future<Output = Result<bool, String>> + Send;

    /// GET /api/form — the pending forms of the server's location (all
    /// sessions). Used to reconcile the form queue with the server (the poll);
    /// `None` means "the server did not tell" (keep the queue as it is).
    fn list_forms(&self) -> impl Future<Output = Result<Option<Vec<FormInfo>>, String>> + Send;

    /// GET /api/session/{sessionID}/form — the pending forms of one session.
    /// A fallback for `list_forms` (a server without the global route).
    fn list_session_forms(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<Vec<FormInfo>>, String>> + Send;

    /// PUT /auth/{provider_id} — store an API-key credential for a provider.
    fn set_api_key(
        &self,
        provider_id: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

