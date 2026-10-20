//! Protocol adapter boundary.
//!
//! `OpenCodeApi` is the single interface the worker stack (worker loop, SSE
//! dispatcher, history loader) talks to. Keeping the concrete protocol behind
//! this trait lets us introduce a v2 adapter next to the current v1
//! implementation without rewiring the rest of the client (see
//! `vault/План-v2.md`).
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

use crate::types::event::PermissionReplyKind;
use crate::types::v2::EventEnvelope;
use crate::types::GlobalEvent;

/// One SSE frame from the server, tagged by protocol version. The worker loop
/// matches on this so the v1 (`/global/event`) and v2 (`/api/event`) paths share
/// one event pump (see `vault/План-v2.md` §8 step 5).
#[derive(Debug, Clone)]
pub enum ClientEvent {
    V1(GlobalEvent),
    V2(EventEnvelope),
}

/// Server protocol adapter (v1 and v2 implementations).
pub trait OpenCodeApi: Clone + Send + Sync + 'static {
    /// Open the server's event stream. v1 reads `/global/event`
    /// ([`GlobalEvent`]); v2 reads `/api/event` ([`EventEnvelope`]). Each adapter
    /// picks its own endpoint, so the worker never hard-codes a version.
    fn event_stream(
        &self,
    ) -> impl Future<
        Output = Result<Pin<Box<dyn Stream<Item = Result<ClientEvent, String>> + Send>>, String>,
    > + Send;

    /// GET /global/health — is the server up?
    fn health(&self) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /global/dispose — drop all instances, releasing resources so the
    /// next request reloads config (e.g. a freshly written credential).
    fn global_dispose(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// POST /session — create a session. Empty/absent title → server names it.
    fn create_session(
        &self,
        title: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// GET /session — session list.
    fn list_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

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

    /// POST /session/{id}/share — create a public link.
    fn share_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// DELETE /session/{id}/share — revoke link access.
    fn unshare_session(&self, session_id: &str)
        -> impl Future<Output = Result<(), String>> + Send;

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

    /// GET /session/{id}/message — full history as `[{info, parts}]`.
    fn session_messages(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send;

    /// POST /session/{id}/prompt_async — send a prompt without waiting.
    ///
    /// Returns the ID the server assigned to the user message, when the protocol
    /// reports it. v1 does not (the id arrives later via the `message.updated`
    /// SSE event, so this returns `None` and the caller waits); v2 returns it in
    /// the `POST .../prompt` response (`{data: SessionInboxUser.id}`), so the
    /// caller never has to guess via SSE.
    fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send;

    /// POST /session/{id}/abort — abort execution.
    fn abort(&self, session_id: &str) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /session/{id}/permissions/{pid} — answer a permission request.
    fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &PermissionReplyKind,
    ) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /question/{id}/reply — answer a question from the model.
    fn answer_question(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> impl Future<Output = Result<bool, String>> + Send;

    /// POST /question/{id}/reject — decline a question from the model.
    fn reject_question(&self, request_id: &str)
        -> impl Future<Output = Result<bool, String>> + Send;

    /// PUT /auth/{provider_id} — store an API-key credential for a provider.
    fn set_api_key(
        &self,
        provider_id: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}
