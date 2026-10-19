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
//! Uses native `async fn` in traits (RPITIT, stable since Rust 1.75). The trait
//! is not object-safe by design: consumers are generic over `T: OpenCodeApi` and
//! use static dispatch, so there is no runtime cost and no extra dependency.

use crate::types::event::PermissionReplyKind;

/// Server protocol adapter (v1 for now; v2 will be a second implementation).
pub trait OpenCodeApi: Clone + Send + Sync + 'static {
    /// GET /global/health — is the server up?
    async fn health(&self) -> Result<bool, String>;

    /// POST /global/dispose — drop all instances, releasing resources so the
    /// next request reloads config (e.g. a freshly written credential).
    async fn global_dispose(&self) -> Result<(), String>;

    /// POST /session — create a session. Empty/absent title → server names it.
    async fn create_session(&self, title: Option<&str>) -> Result<serde_json::Value, String>;

    /// GET /session — session list.
    async fn list_sessions(&self) -> Result<serde_json::Value, String>;

    /// PATCH /session/{id} — rename a session.
    async fn rename_session(&self, session_id: &str, title: &str) -> Result<(), String>;

    /// DELETE /session/{id} — delete a session.
    async fn delete_session(&self, session_id: &str) -> Result<(), String>;

    /// GET /session/{id} — a single session.
    async fn get_session(&self, session_id: &str) -> Result<serde_json::Value, String>;

    /// POST /session/{id}/fork — branch a session (optionally cut at a message).
    async fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> Result<serde_json::Value, String>;

    /// POST /session/{id}/revert — hide the tail starting at `message_id`.
    async fn revert_session(&self, session_id: &str, message_id: &str) -> Result<(), String>;

    /// POST /session/{id}/unrevert — restore previously reverted messages.
    async fn unrevert_session(&self, session_id: &str) -> Result<(), String>;

    /// POST /session/{id}/share — create a public link.
    async fn share_session(&self, session_id: &str) -> Result<serde_json::Value, String>;

    /// DELETE /session/{id}/share — revoke link access.
    async fn unshare_session(&self, session_id: &str) -> Result<(), String>;

    /// POST /session/{id}/summarize — compact history for the given model.
    async fn summarize_session(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String>;

    /// GET /config/providers — provider/model catalog.
    async fn providers(&self) -> Result<serde_json::Value, String>;

    /// GET /session/{id}/todo — the agent's task checklist.
    async fn session_todo(&self, session_id: &str) -> Result<serde_json::Value, String>;

    /// POST /api/session/{id}/model — switch the session model.
    async fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String>;

    /// POST /api/session/{id}/agent — switch the session agent (build/plan).
    async fn set_session_agent(&self, session_id: &str, agent: &str) -> Result<(), String>;

    /// GET /session/{id}/message — full history as `[{info, parts}]`.
    async fn session_messages(&self, session_id: &str) -> Result<serde_json::Value, String>;

    /// POST /session/{id}/prompt_async — send a prompt without waiting.
    async fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> Result<(), String>;

    /// POST /session/{id}/abort — abort execution.
    async fn abort(&self, session_id: &str) -> Result<bool, String>;

    /// POST /session/{id}/permissions/{pid} — answer a permission request.
    async fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &PermissionReplyKind,
    ) -> Result<bool, String>;

    /// POST /question/{id}/reply — answer a question from the model.
    async fn answer_question(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<bool, String>;

    /// POST /question/{id}/reject — decline a question from the model.
    async fn reject_question(&self, request_id: &str) -> Result<bool, String>;

    /// PUT /auth/{provider_id} — store an API-key credential for a provider.
    async fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), String>;
}
