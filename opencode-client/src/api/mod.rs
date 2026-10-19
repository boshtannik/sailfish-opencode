pub mod api;
pub mod auth;
pub mod permission;
pub mod question;
pub mod session;

pub use api::OpenCodeApi;

use reqwest::Client;

/// Cheap to clone: `reqwest::Client` shares the connection pool, so the worker
/// can hand a copy to a background task (e.g. the blocking `/summarize` call)
/// without losing the SSE stream.
#[derive(Clone)]
pub struct OpenCodeClient {
    pub http: Client,
    pub base: String,
    pub auth: Option<(String, String)>,
}

impl OpenCodeClient {
    pub fn new(base: String, auth: Option<(String, String)>) -> Self {
        Self {
            http: Client::new(),
            base,
            auth,
        }
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Some((u, p)) => req.basic_auth(u, Some(p)),
            None => req,
        }
    }

    /// GET /global/health
    pub async fn health(&self) -> Result<bool, String> {
        let url = format!("{}/global/health", self.base);
        let resp = self.authed(self.http.get(&url)).send().await.map_err(|e| e.to_string())?;
        Ok(resp.status().is_success())
    }

    /// POST /global/dispose — dispose all instances, releasing resources.
    /// The next request re-creates the instance and reloads config (so a
    /// credential written by `opencode console login` is picked up).
    pub async fn global_dispose(&self) -> Result<(), String> {
        let url = format!("{}/global/dispose", self.base);
        let resp = self.authed(self.http.post(&url)).send().await.map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }
}

/// v1 protocol adapter: delegates to the inherent methods defined across this
/// module's submodules. Inherent methods win in method resolution, so each call
/// below resolves to the concrete v1 implementation.
impl OpenCodeApi for OpenCodeClient {
    async fn health(&self) -> Result<bool, String> {
        OpenCodeClient::health(self).await
    }

    async fn global_dispose(&self) -> Result<(), String> {
        OpenCodeClient::global_dispose(self).await
    }

    async fn create_session(&self, title: Option<&str>) -> Result<serde_json::Value, String> {
        OpenCodeClient::create_session(self, title).await
    }

    async fn list_sessions(&self) -> Result<serde_json::Value, String> {
        OpenCodeClient::list_sessions(self).await
    }

    async fn rename_session(&self, session_id: &str, title: &str) -> Result<(), String> {
        OpenCodeClient::rename_session(self, session_id, title).await
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), String> {
        OpenCodeClient::delete_session(self, session_id).await
    }

    async fn get_session(&self, session_id: &str) -> Result<serde_json::Value, String> {
        OpenCodeClient::get_session(self, session_id).await
    }

    async fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        OpenCodeClient::fork_session(self, session_id, message_id).await
    }

    async fn revert_session(&self, session_id: &str, message_id: &str) -> Result<(), String> {
        OpenCodeClient::revert_session(self, session_id, message_id).await
    }

    async fn unrevert_session(&self, session_id: &str) -> Result<(), String> {
        OpenCodeClient::unrevert_session(self, session_id).await
    }

    async fn share_session(&self, session_id: &str) -> Result<serde_json::Value, String> {
        OpenCodeClient::share_session(self, session_id).await
    }

    async fn unshare_session(&self, session_id: &str) -> Result<(), String> {
        OpenCodeClient::unshare_session(self, session_id).await
    }

    async fn summarize_session(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        OpenCodeClient::summarize_session(self, session_id, provider_id, model_id).await
    }

    async fn providers(&self) -> Result<serde_json::Value, String> {
        OpenCodeClient::providers(self).await
    }

    async fn session_todo(&self, session_id: &str) -> Result<serde_json::Value, String> {
        OpenCodeClient::session_todo(self, session_id).await
    }

    async fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        OpenCodeClient::set_model(self, session_id, provider_id, model_id).await
    }

    async fn set_session_agent(&self, session_id: &str, agent: &str) -> Result<(), String> {
        OpenCodeClient::set_session_agent(self, session_id, agent).await
    }

    async fn session_messages(&self, session_id: &str) -> Result<serde_json::Value, String> {
        OpenCodeClient::session_messages(self, session_id).await
    }

    async fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> Result<(), String> {
        OpenCodeClient::prompt_async(self, session_id, text, agent).await
    }

    async fn abort(&self, session_id: &str) -> Result<bool, String> {
        OpenCodeClient::abort(self, session_id).await
    }

    async fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &crate::types::event::PermissionReplyKind,
    ) -> Result<bool, String> {
        OpenCodeClient::answer_permission(self, session_id, permission_id, response).await
    }

    async fn answer_question(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<bool, String> {
        OpenCodeClient::answer_question(self, request_id, answers).await
    }

    async fn reject_question(&self, request_id: &str) -> Result<bool, String> {
        OpenCodeClient::reject_question(self, request_id).await
    }

    async fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), String> {
        OpenCodeClient::set_api_key(self, provider_id, key).await
    }
}