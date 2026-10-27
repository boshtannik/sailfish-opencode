//! v2 protocol adapter (opencode 2.x).
//!
//! The client's server protocol: session CRUD, prompt, history and the
//! event-stream boundary (see `vault/План-v2.md` §8), all behind the
//! [`OpenCodeApi`] trait the worker stack is generic over.
//!
//! Endpoints follow `vault/Разведка-v2.md` §5. The v2 HTTP surface differs in
//! three ways this module already accounts for:
//!   * prefix `/api`;
//!   * response envelopes `{data: …}` / `{data, cursor}` / `{location, data}`;
//!   * `204 No Content` for mutations (rename/delete/revert/reply…).
//!
//! Type shape migration (message `type` union, `content[]`, compaction message,
//! forms) is step 4; until then read methods return the unwrapped `data` as
//! [`serde_json::Value`], so callers in `main.rs` compile unchanged.

use std::future::Future;

use futures_util::Stream;
use reqwest::Client;

use crate::types::v2::form::FormInfo;
use crate::types::v2::permission::{PermissionReplyKind, PermissionRequest, SavedPermission};
use crate::types::v2::EventEnvelope;
use crate::types::v2::keys as k;

use super::api::OpenCodeApi;

/// v2 protocol adapter. Cheap to clone (shared `reqwest` pool), so a
/// background task can hold a copy.
#[derive(Clone)]
pub struct V2Adapter {
    pub http: Client,
    pub base: String,
    pub auth: Option<(String, String)>,
}

impl V2Adapter {
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

    /// Unwrap a `{data: T}` (or `{location, data}`) envelope into its `data`.
    /// v2 wraps every payload; a bare value is returned as-is so the helper
    /// stays usable for the handful of endpoints that answer unenveloped.
    fn unwrap_data(mut v: serde_json::Value) -> serde_json::Value {
        if let Some(map) = v.as_object_mut() {
            if map.contains_key(k::DATA) {
                return map.remove(k::DATA).unwrap_or(serde_json::Value::Null);
            }
        }
        v
    }

    /// POST with no body, expecting `204 No Content`.
    async fn post_no_content(&self, url: &str) -> Result<(), String> {
        let resp = self
            .authed(self.http.post(url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }

    /// GET /api/model → `{location, data:[Model.Info]}` (the server's model
    /// catalog for the current directory).
    async fn models(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/api/model", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    /// GET a JSON endpoint (the envelope is unwrapped by the caller).
    async fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .authed(self.http.get(url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

impl OpenCodeApi for V2Adapter {
    /// GET /api/info — a bare `ServerInfo`, no envelope. Reachable means up.
    fn health(&self) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!("{}/api/info", self.base);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(resp.status().is_success())
        }
    }

    /// POST /api/location/reload — v2 has no `/global/dispose`; reloading the
    /// location makes the next request pick up freshly written credentials.
    fn global_dispose(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            self.post_no_content(&format!("{}/api/location/reload", self.base))
                .await
        }
    }

    /// POST /api/session `{title?}` → `{data: Session.Info}`.
    fn create_session(
        &self,
        title: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session", self.base);
            let mut body = serde_json::Map::new();
            if let Some(t) = title {
                body.insert("title".into(), serde_json::Value::String(t.to_string()));
            }
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// GET /api/session → `{data: [Session.Info], cursor}`. Callers key into
    /// the array, so expose the plain array in place of the envelope.
    fn list_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session", self.base);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// GET /api/session/active — `{data: {"ses…": {type:"running"}}}`. The
    /// server's own view of which sessions are draining; absent = inactive.
    fn active_sessions(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session/active", self.base);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// POST /api/integration/{id}/connect/oauth `{methodID}` → the attempt
    /// (`{location, data: {attemptID, url, instructions, mode, time}}`).
    fn integration_oauth_connect(
        &self,
        integration_id: &str,
        method_id: &str,
    ) -> impl Future<Output = Result<crate::types::v2::integration::Attempt, String>> + Send {
        async move {
            let url = format!("{}/api/integration/{}/connect/oauth", self.base, integration_id);
            let body = serde_json::json!({ k::METHOD_ID: method_id });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {v}"));
            }
            serde_json::from_value(Self::unwrap_data(v))
                .map_err(|e| format!("connect/oauth answer: {e}"))
        }
    }

    /// GET /api/integration/{id}/connect/oauth/{attemptID} → attempt status.
    fn integration_oauth_status(
        &self,
        integration_id: &str,
        attempt_id: &str,
    ) -> impl Future<Output = Result<crate::types::v2::integration::AttemptStatus, String>> + Send
    {
        async move {
            let url = format!(
                "{}/api/integration/{}/connect/oauth/{}",
                self.base, integration_id, attempt_id
            );
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {v}"));
            }
            serde_json::from_value(Self::unwrap_data(v))
                .map_err(|e| format!("connect/oauth status: {e}"))
        }
    }

    /// POST …/{attemptID}/complete `{code: null}` → 204 (credential persisted).
    fn integration_oauth_complete(
        &self,
        integration_id: &str,
        attempt_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!(
                "{}/api/integration/{}/connect/oauth/{}/complete",
                self.base, integration_id, attempt_id
            );
            let body = serde_json::json!({ k::CODE: serde_json::Value::Null });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(())
        }
    }

    /// DELETE …/{attemptID} → 204 (abandoned attempt).
    fn integration_oauth_cancel(
        &self,
        integration_id: &str,
        attempt_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!(
                "{}/api/integration/{}/connect/oauth/{}",
                self.base, integration_id, attempt_id
            );
            let resp = self
                .authed(self.http.delete(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(())
        }
    }

    /// PATCH /api/session/{id} `{title}` → 204.
    fn rename_session(        &self,
        session_id: &str,
        title: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}", self.base, session_id);
            let body = serde_json::json!({ k::TITLE: title });
            let resp = self
                .authed(self.http.patch(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// DELETE /api/session/{id} → 204.
    fn delete_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}", self.base, session_id);
            let resp = self
                .authed(self.http.delete(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// GET /api/session/{id} → `{data: Session.Info}`.
    fn get_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}", self.base, session_id);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// POST /api/session/{id}/fork `{before?}` → `{data: Session.Info}`.
    fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/fork", self.base, session_id);
            let mut body = serde_json::Map::new();
            if let Some(m) = message_id {
                body.insert(k::BEFORE.into(), serde_json::Value::String(m.to_string()));
            }
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// POST /api/session/{id}/revert/stage `{messageID}` → `{data: Session.Revert}`.
    fn revert_session(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/revert/stage", self.base, session_id);
            let body = serde_json::json!({ k::MESSAGE_ID: message_id });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// POST /api/session/{id}/revert/commit → 204.
    fn unrevert_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            self.post_no_content(&format!(
                "{}/api/session/{}/revert/commit",
                self.base, session_id
            ))
            .await
        }
    }

    /// POST /api/session/{id}/compact → `{data: Inbox.Compaction}`. The model
    /// is taken from the session in v2; the v1 provider/model args are ignored.
    fn summarize_session(
        &self,
        session_id: &str,
        _provider_id: &str,
        _model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/compact", self.base, session_id);
            let resp = self
                .authed(self.http.post(&url).json(&serde_json::json!({})))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// Provider/model catalog, normalised to the v1 `{providers:[{id, name,
    /// models:{id:{name}}}]}` shape that `refresh_models` flattens for QML.
    ///
    /// v2 has no equivalent to v1's `/config/providers`: `/api/provider` needs a
    /// `location[directory]` query and only lists *configured* providers (empty
    /// on the bundled 1.19.0 → the model picker would be blank), while the
    /// catalog actually offered by the server is `GET /api/model`
    /// (`{location, data:[{id, providerID, name, …}]}`, verified live — see
    /// `vault/План-v2.md` §12.2). We therefore build the v1 shape from
    /// `/api/model`, synthesising a provider entry per `providerID` (the display
    /// name is the id — v2 does not expose a provider title we need here).
    fn providers(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let v = self.models().await?;
            Ok(fold_models_into_providers(&v))
        }
    }

    /// todo has no v2 HTTP route (see `vault/Разведка-v2.md` §5). Report an
    /// empty checklist instead of an error: the poll loop calls this often and
    /// an error would spam the log. Real handling is deferred to step 6.
    fn session_todo(
        &self,
        _session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { Ok(serde_json::Value::Array(Vec::new())) }
    }

    /// POST /api/session/{id}/model `{model: Model.Ref}` → 204.
    fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/model", self.base, session_id);
            let body = serde_json::json!({
                k::MODEL: { k::PROVIDER_ID: provider_id, k::ID: model_id }
            });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// GET /api/model/default → `{location, data: Model.Info}`; the model a
    /// fresh session uses. Used to bind a session that has no model yet so the
    /// picker has something to check.
    fn default_model(
        &self,
    ) -> impl Future<Output = Result<Option<(String, String)>, String>> + Send {
        async move {
            let url = format!("{}/api/model/default", self.base);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let d = &v["data"];
            let provider = d[k::PROVIDER_ID].as_str().unwrap_or("");
            let id = d[k::ID].as_str().unwrap_or("");
            if provider.is_empty() || id.is_empty() {
                return Ok(None);
            }
            Ok(Some((provider.to_string(), id.to_string())))
        }
    }

    /// POST /api/session/{id}/agent `{agent}` → 204.
    fn set_session_agent(
        &self,
        session_id: &str,
        agent: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/agent", self.base, session_id);
            let body = serde_json::json!({ k::AGENT: agent });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// GET /api/session/{id}/message → `{data: [Message.Info], cursor}`. v2
    /// messages carry `content[]` instead of `parts` (type migration is step 4);
    /// the raw array is returned for now.
    fn session_messages(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/message", self.base, session_id);
            let resp = self
                .authed(self.http.get(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v))
        }
    }

    /// POST /api/session/{id}/prompt `{prompt: {text*, agents?}}` →
    /// `{data: Inbox.User}`. The request body nests the prompt under a `prompt`
    /// key (the bundled opencode 1.19.0 rejects a flat `{text}` with
    /// `Missing key at ["prompt"]`), and v2 `agents` is an array of
    /// `Prompt.AgentAttachment` objects (`{name}`), not bare name strings —
    /// sending `["build"]` is rejected with 400. The response's `data.id` is
    /// the user-message ID the server assigned (same value it later stores as
    /// the user `MessageInfo.id`), so the caller gets revert/fork id without
    /// waiting for an SSE event.
    fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/prompt", self.base, session_id);
            let mut body = serde_json::Map::new();
            body.insert(k::TEXT.into(), serde_json::Value::String(text.to_string()));
            if let Some(a) = agent {
                body.insert(k::AGENTS.into(), serde_json::json!([{ k::NAME: a }]));
            }
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(Self::unwrap_data(v)
                .get(k::ID)
                .and_then(|id| id.as_str())
                .map(str::to_string))
        }
    }

    /// POST /api/session/{id}/interrupt → `{interrupted: bool}`.
    fn abort(&self, session_id: &str) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/interrupt", self.base, session_id);
            let resp = self
                .authed(self.http.post(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(v.get(k::INTERRUPTED).and_then(|b| b.as_bool()).unwrap_or(false))
        }
    }

    /// POST /api/session/{id}/permission/{reqID}/reply `{decision, message?}`
    /// → 204.
    ///
    /// `message` is the optional reason the server shows to the model (why a
    /// request was rejected).
    fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &PermissionReplyKind,
        message: Option<&str>,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!(
                "{}/api/session/{}/permission/{}/reply",
                self.base, session_id, permission_id
            );
            let decision = match response {
                PermissionReplyKind::Once => k::DECISION_ONCE,
                PermissionReplyKind::Always => k::DECISION_ALWAYS,
                PermissionReplyKind::Reject => k::DECISION_REJECT,
            };
            let mut body = serde_json::json!({ k::DECISION: decision });
            if let Some(m) = message {
                if !m.is_empty() {
                    body[k::MESSAGE] = serde_json::Value::String(m.to_string());
                }
            }
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(true)
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// GET /api/permission/request → `{location, data:[Permission.Request]}` —
    /// the pending permission requests of the server's location (all sessions).
    fn list_permissions(&self) -> impl Future<Output = Result<Option<Vec<PermissionRequest>>, String>> + Send {
        async move {
            let url = format!("{}/api/permission/request", self.base);
            match self.get_json(&url).await {
                Ok(v) => Ok(parse_permission_list(v)),
                Err(e) => Err(e),
            }
        }
    }

    /// GET /api/session/{sessionID}/permission → `{data:[Permission.Request]}`.
    fn list_session_permissions(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<Vec<PermissionRequest>>, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/permission", self.base, session_id);
            match self.get_json(&url).await {
                Ok(v) => Ok(parse_permission_list(v)),
                Err(e) => Err(e),
            }
        }
    }

    /// GET /api/permission/saved → `{data:[PermissionSaved.Info]}` — the rules
    /// the server remembers ("always allow").
    fn list_saved_permissions(
        &self,
    ) -> impl Future<Output = Result<Option<Vec<SavedPermission>>, String>> + Send {
        async move {
            let url = format!("{}/api/permission/saved", self.base);
            match self.get_json(&url).await {
                Ok(v) => Ok(parse_saved_permission_list(v)),
                Err(e) => Err(e),
            }
        }
    }

    /// DELETE /api/permission/saved/{id} → 204 — revoke a stored rule.
    fn remove_saved_permission(&self, id: &str) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!("{}/api/permission/saved/{}", self.base, id);
            let resp = self
                .authed(self.http.delete(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(true)
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// POST /api/session/{sessionID}/form/{formID}/reply `{answer: {…}}` → 204.
    ///
    /// The route is session-scoped in opencode 2.1.4 (the form id alone is not
    /// enough): `GET /api/session/{sessionID}/form/{formID}/reply`.
    fn reply_form(
        &self,
        session_id: &str,
        form_id: &str,
        answer: serde_json::Value,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!(
                "{}/api/session/{}/form/{}/reply",
                self.base, session_id, form_id
            );
            let body = serde_json::json!({ k::ANSWER: answer });
            let resp = self
                .authed(self.http.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(true)
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// DELETE /api/session/{sessionID}/form/{formID}[?message=…] → 204.
    fn cancel_form(
        &self,
        session_id: &str,
        form_id: &str,
        message: Option<&str>,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let mut url = format!("{}/api/session/{}/form/{}", self.base, session_id, form_id);
            if let Some(m) = message {
                url.push_str(&format!("?message={}", urlencode(m)));
            }
            let resp = self
                .authed(self.http.delete(&url))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok(true)
            } else {
                Err(format!("HTTP {}", resp.status()))
            }
        }
    }

    /// GET /api/form → `{location, data:[Form.Info]}` — the pending forms of
    /// the server's location (all sessions).
    fn list_forms(&self) -> impl Future<Output = Result<Option<Vec<FormInfo>>, String>> + Send {
        async move {
            let url = format!("{}/api/form", self.base);
            match self.get_json(&url).await {
                Ok(v) => Ok(parse_form_list(v)),
                Err(e) => Err(e),
            }
        }
    }

    /// GET /api/session/{sessionID}/form → `{data:[Form.Info]}`.
    fn list_session_forms(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<Vec<FormInfo>>, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/form", self.base, session_id);
            match self.get_json(&url).await {
                Ok(v) => Ok(parse_form_list(v)),
                Err(e) => Err(e),
            }
        }
    }

    /// auth moved to credentials/integrations in v2 (see `vault/Разведка-v2.md`
    /// §5); the replacement is step 6 work.
    fn set_api_key(
        &self,
        _provider_id: &str,
        _key: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            Err(
                "auth moved to credentials/integrations in opencode v2 — not implemented yet"
                    .to_string(),
            )
        }
    }

    /// Subscribe to `/api/event`, yielding [`EventEnvelope`] frames.
    fn event_stream(
        &self,
    ) -> impl Future<
        Output = Result<
            std::pin::Pin<
                Box<dyn Stream<Item = Result<EventEnvelope, String>> + Send>,
            >,
            String,
        >,
    > + Send {
        async move {
            let sse = crate::event::sse::SseClient::new(self.base.clone(), self.auth.clone());
            sse.stream().await
        }
    }
}

/// Parse a form list payload (`{data:[Form.Info]}`) into typed forms.
///
/// A malformed entry is skipped: one bad form must not hide the others.
/// `None` means the payload was not a list at all (the caller keeps the queue
/// as it is).
fn parse_form_list(v: serde_json::Value) -> Option<Vec<FormInfo>> {
    let data = V2Adapter::unwrap_data(v);
    let arr = data.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match serde_json::from_value::<FormInfo>(item.clone()) {
            Ok(f) => out.push(f),
            Err(e) => log::warn!("form: не удалось разобрать форму: {e}"),
        }
    }
    Some(out)
}

/// Parse a permission list (`{data:[Permission.Request]}`).
fn parse_permission_list(v: serde_json::Value) -> Option<Vec<PermissionRequest>> {
    let data = V2Adapter::unwrap_data(v);
    let arr = data.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match serde_json::from_value::<PermissionRequest>(item.clone()) {
            Ok(p) => out.push(p),
            Err(e) => log::warn!("permission: не удалось разобрать запрос: {e}"),
        }
    }
    Some(out)
}

/// Parse the saved ("always allow") rules (`{data:[PermissionSaved.Info]}`).
fn parse_saved_permission_list(v: serde_json::Value) -> Option<Vec<SavedPermission>> {
    let data = V2Adapter::unwrap_data(v);
    let arr = data.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match serde_json::from_value::<SavedPermission>(item.clone()) {
            Ok(p) => out.push(p),
            Err(e) => log::warn!("permission: не удалось разобрать сохранённое правило: {e}"),
        }
    }
    Some(out)
}

/// Percent-encode a query-parameter value (the form cancel reason).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the v1-shaped provider catalog (`{providers:[{id, name, models}]}`)
/// from a v2 `/api/model` response (`{location, data:[{id, providerID, name}]}`).
///
/// Models are grouped by `providerID` (providers in first-seen order, models
/// within a provider sorted by id via the `BTreeMap`); each model carries only
/// the `name` field `refresh_models` reads. A model without `id`/`providerID`
/// is skipped. Kept free so it can be unit-tested without an HTTP server.
fn fold_models_into_providers(v: &serde_json::Value) -> serde_json::Value {
    let empty = Vec::new();
    let models = v[k::DATA].as_array().unwrap_or(&empty);
    let mut order: Vec<String> = Vec::new();
    let mut by_provider: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> =
        std::collections::BTreeMap::new();
    for m in models {
        let pid = m[k::PROVIDER_ID].as_str().unwrap_or("");
        let id = m[k::ID].as_str().unwrap_or("");
        if pid.is_empty() || id.is_empty() {
            continue;
        }
        if !by_provider.contains_key(pid) {
            order.push(pid.to_string());
            by_provider.insert(pid.to_string(), serde_json::Map::new());
        }
        let name = m[k::NAME].as_str().unwrap_or(id);
        if let Some(ms) = by_provider.get_mut(pid) {
            ms.insert(id.to_string(), serde_json::json!({ k::NAME: name }));
        }
    }
    let providers: Vec<serde_json::Value> = order
        .iter()
        .map(|pid| {
            let models = by_provider
                .remove(pid)
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::json!({}));
            serde_json::json!({ k::ID: pid, k::NAME: pid, k::MODELS: models })
        })
        .collect();
    serde_json::json!({ k::PROVIDERS: providers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The server's pending permission list (`{location, data:[Permission.Request]}`).
    #[test]
    fn permission_list_parses_requests() {
        let v = serde_json::json!({
            "location": "/home/user/proj",
            "data": [{
                "id": "per_1",
                "sessionID": "ses1",
                "action": "bash",
                "resources": ["ls -la"],
                "save": ["bash"],
                "message": "Run `ls -la`?",
                "source": { "type": "tool", "messageID": "msg_1", "id": "call_1" }
            }]
        });
        let list = parse_permission_list(v).expect("the list is parsed");
        assert_eq!(list.len(), 1);
        let p = &list[0];
        assert_eq!(p.id, "per_1");
        assert_eq!(p.session_id, "ses1");
        assert_eq!(p.action, "bash");
        assert_eq!(p.resources, vec!["ls -la".to_string()]);
        assert_eq!(p.save, Some(vec!["bash".to_string()]));
        assert_eq!(p.message.as_deref(), Some("Run `ls -la`?"));
        let src = p.source.as_ref().expect("the source is parsed");
        assert_eq!(src.kind, "tool");
        assert_eq!(src.message_id, "msg_1");
        assert_eq!(src.id, "call_1");
    }

    /// A broken item does not break the whole list (the server may send a
    /// shape we do not know yet) — the good ones are still shown.
    #[test]
    fn permission_list_skips_broken_items() {
        let v = serde_json::json!({
            "data": [
                { "id": "per_1", "sessionID": "ses1", "action": "bash", "resources": [] },
                { "unexpected": true }
            ]
        });
        let list = parse_permission_list(v).expect("the list is parsed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "per_1");
    }

    /// The saved rules (`{data:[PermissionSaved.Info]}`).
    #[test]
    fn saved_permission_list_parses() {
        let v = serde_json::json!({
            "data": [{
                "id": "saved_1",
                "projectID": "prj_1",
                "action": "bash",
                "resource": "ls",
                "time": { "created": 1.0, "updated": 2.0 }
            }]
        });
        let list = parse_saved_permission_list(v).expect("the list is parsed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "saved_1");
        assert_eq!(list[0].project_id, "prj_1");
        assert_eq!(list[0].action, "bash");
        assert_eq!(list[0].resource, "ls");
        let t = list[0].time.as_ref().expect("the time is parsed");
        assert_eq!(t.created, 1.0);
        assert_eq!(t.updated, 2.0);
    }

    #[test]
    fn fold_models_groups_by_provider() {
        let v = json!({
            "location": { "directory": "/tmp" },
            "data": [
                { "id": "b-model", "providerID": "opencode", "name": "B" },
                { "id": "a-model", "providerID": "opencode", "name": "A" },
                { "id": "x", "providerID": "other", "name": "X" }
            ]
        });
        let out = fold_models_into_providers(&v);
        let provs = out["providers"].as_array().expect("providers array");
        assert_eq!(provs.len(), 2);
        assert_eq!(provs[0]["id"], "opencode");
        assert_eq!(provs[1]["id"], "other");
        // v1 consumes `{id: {name}}`; models sorted by id.
        let opencode = provs[0]["models"].as_object().unwrap();
        let keys: Vec<&String> = opencode.keys().collect();
        assert_eq!(keys, vec!["a-model", "b-model"]);
        assert_eq!(opencode["a-model"]["name"], "A");
    }

    #[test]
    fn fold_models_skips_incomplete_entries() {
        let v = json!({ "data": [
            { "id": "", "providerID": "opencode", "name": "n" },
            { "id": "m", "providerID": "", "name": "n" },
            { "id": "ok", "providerID": "p" }
        ] });
        let out = fold_models_into_providers(&v);
        let provs = out["providers"].as_array().unwrap();
        assert_eq!(provs.len(), 1);
        let models = provs[0]["models"].as_object().unwrap();
        assert!(models.contains_key("ok"));
        // Missing `name` falls back to the id.
        assert_eq!(models["ok"]["name"], "ok");
    }

    #[test]
    fn fold_models_handles_empty_catalog() {
        let out = fold_models_into_providers(&json!({ "data": [] }));
        assert_eq!(out["providers"].as_array().unwrap().len(), 0);
    }
}
