pub mod api;
pub mod auth;
pub mod permission;
pub mod question;
pub mod session;
pub mod v2;

pub use api::OpenCodeApi;
pub use v2::V2Adapter;

use std::future::Future;

use reqwest::Client;

/// Which server protocol adapter the worker should speak. Selected by
/// `settings.api_version`; unknown values fall back to v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiVersion {
    V1,
    V2,
}

impl ApiVersion {
    /// Parse the settings value. Case-insensitive; anything other than `v2`
    /// means v1, so a typo never breaks a working install.
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "v2" | "2" => ApiVersion::V2,
            "v1" | "1" => ApiVersion::V1,
            other => {
                log::warn!("settings: unknown api_version {other:?} — falling back to v1");
                ApiVersion::V1
            }
        }
    }
}

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
    fn health(&self) -> impl Future<Output = Result<bool, String>> + Send {
        async move { OpenCodeClient::health(self).await }
    }

    fn global_dispose(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::global_dispose(self).await }
    }

    fn create_session(
        &self,
        title: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::create_session(self, title).await }
    }

    fn list_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::list_sessions(self).await }
    }

    fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::rename_session(self, session_id, title).await }
    }

    fn delete_session(&self, session_id: &str) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::delete_session(self, session_id).await }
    }

    fn get_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::get_session(self, session_id).await }
    }

    fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::fork_session(self, session_id, message_id).await }
    }

    fn revert_session(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::revert_session(self, session_id, message_id).await }
    }

    fn unrevert_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::unrevert_session(self, session_id).await }
    }

    fn share_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::share_session(self, session_id).await }
    }

    fn unshare_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::unshare_session(self, session_id).await }
    }

    fn summarize_session(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::summarize_session(self, session_id, provider_id, model_id).await }
    }

    fn providers(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::providers(self).await }
    }

    fn session_todo(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::session_todo(self, session_id).await }
    }

    fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::set_model(self, session_id, provider_id, model_id).await }
    }

    fn set_session_agent(
        &self,
        session_id: &str,
        agent: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::set_session_agent(self, session_id, agent).await }
    }

    fn session_messages(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { OpenCodeClient::session_messages(self, session_id).await }
    }

    fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::prompt_async(self, session_id, text, agent).await }
    }

    fn abort(&self, session_id: &str) -> impl Future<Output = Result<bool, String>> + Send {
        async move { OpenCodeClient::abort(self, session_id).await }
    }

    fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &crate::types::event::PermissionReplyKind,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            OpenCodeClient::answer_permission(self, session_id, permission_id, response).await
        }
    }

    fn answer_question(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move { OpenCodeClient::answer_question(self, request_id, answers).await }
    }

    fn reject_question(
        &self,
        request_id: &str,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move { OpenCodeClient::reject_question(self, request_id).await }
    }

    fn set_api_key(
        &self,
        provider_id: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { OpenCodeClient::set_api_key(self, provider_id, key).await }
    }

    /// v1: subscribe to `/global/event`, yielding [`GlobalEvent`] frames.
    fn event_stream(
        &self,
    ) -> impl Future<
        Output = Result<
            std::pin::Pin<
                Box<dyn futures_util::Stream<Item = Result<api::ClientEvent, String>> + Send>,
            >,
            String,
        >,
    > + Send {
        async move {
            let sse = crate::event::sse::SseClient::new(self.base.clone(), self.auth.clone());
            let stream = sse.stream().await?;
            // Wrap each v1 frame in the shared `ClientEvent` enum so the worker
            // sees one stream type regardless of protocol version.
            let mapped = futures_util::StreamExt::map(stream, |r| r.map(api::ClientEvent::V1));
            Ok(Box::pin(mapped)
                as std::pin::Pin<
                    Box<dyn futures_util::Stream<Item = Result<api::ClientEvent, String>> + Send>,
                >)
        }
    }
}
