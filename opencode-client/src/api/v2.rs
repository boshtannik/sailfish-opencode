//! v2 protocol adapter (opencode 2.x).
//!
//! Skeleton for step 3 of the migration (see `vault/План-v2.md` §8): session
//! CRUD, prompt, history and the event-stream boundary live behind the same
//! [`OpenCodeApi`] trait as v1, so the worker picks one by a settings flag.
//!
//! Endpoints follow `vault/Разведка-v2.md` §5. The v2 HTTP surface differs in
//! three ways this module already accounts for:
//!   * prefix `/api`;
//!   * response envelopes `{data: …}` / `{data, cursor}` / `{location, data}`;
//!   * `204 No Content` for mutations (rename/delete/revert/reply…).
//!
//! Type shape migration (message `type` union, `content[]`, compaction message,
//! forms) is step 4; until then read methods return the unwrapped `data` as
//! [`serde_json::Value`] — the same currency the v1 methods use, so callers in
//! `main.rs` compile unchanged.

use std::future::Future;

use reqwest::Client;

use crate::types::event::PermissionReplyKind;

use super::api::OpenCodeApi;
use super::api::ClientEvent;

/// v2 protocol adapter. Cheap to clone (shared `reqwest` pool), like the v1
/// client, so a background task can hold a copy.
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
            if map.contains_key("data") {
                return map.remove("data").unwrap_or(serde_json::Value::Null);
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

    /// PATCH /api/session/{id} `{title}` → 204.
    fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}", self.base, session_id);
            let body = serde_json::json!({ "title": title });
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
                body.insert("before".into(), serde_json::Value::String(m.to_string()));
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
            let body = serde_json::json!({ "messageID": message_id });
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

    /// share has no v2 HTTP route (see `vault/Разведка-v2.md` §5). Kept as an
    /// explicit unimplemented error until step 6 decides its fate.
    fn share_session(
        &self,
        _session_id: &str,
    ) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move { Err("share is not available in the opencode v2 HTTP API".to_string()) }
    }

    /// unshare has no v2 HTTP route — see `share_session`.
    fn unshare_session(
        &self,
        _session_id: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move { Err("unshare is not available in the opencode v2 HTTP API".to_string()) }
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

    /// GET /api/provider → `{location, data: [Provider.Info]}`.
    fn providers(&self) -> impl Future<Output = Result<serde_json::Value, String>> + Send {
        async move {
            let url = format!("{}/api/provider", self.base);
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
                "model": { "providerID": provider_id, "id": model_id }
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

    /// POST /api/session/{id}/agent `{agent}` → 204.
    fn set_session_agent(
        &self,
        session_id: &str,
        agent: &str,
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/agent", self.base, session_id);
            let body = serde_json::json!({ "agent": agent });
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

    /// POST /api/session/{id}/prompt `{text*, agents?}` → `{data: Inbox.User}`.
    /// The response's `data.id` is the user-message ID the server assigned
    /// (same value it later stores as the user `MessageInfo.id`), so the caller
    /// gets revert/fork id without waiting for an SSE event.
    fn prompt_async(
        &self,
        session_id: &str,
        text: &str,
        agent: Option<String>,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send {
        async move {
            let url = format!("{}/api/session/{}/prompt", self.base, session_id);
            let mut body = serde_json::Map::new();
            body.insert("text".into(), serde_json::Value::String(text.to_string()));
            if let Some(a) = agent {
                body.insert("agents".into(), serde_json::json!([a]));
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
                .get("id")
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
            Ok(v.get("interrupted").and_then(|b| b.as_bool()).unwrap_or(false))
        }
    }

    /// POST /api/session/{id}/permission/{reqID}/reply `{decision}` → 204.
    fn answer_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: &PermissionReplyKind,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            let url = format!(
                "{}/api/session/{}/permission/{}/reply",
                self.base, session_id, permission_id
            );
            let decision = match response {
                PermissionReplyKind::Once => "once",
                PermissionReplyKind::Always => "always",
                PermissionReplyKind::Reject => "reject",
            };
            let body = serde_json::json!({ "decision": decision });
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

    /// Forms replace questions in v2 (see `vault/Разведка-v2.md` §5). The form
    /// model is step 4/6 work; until then answering is unimplemented.
    fn answer_question(
        &self,
        _request_id: &str,
        _answers: Vec<Vec<String>>,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            Err("questions are replaced by forms in opencode v2 — not implemented yet".to_string())
        }
    }

    /// See `answer_question`.
    fn reject_question(
        &self,
        _request_id: &str,
    ) -> impl Future<Output = Result<bool, String>> + Send {
        async move {
            Err("questions are replaced by forms in opencode v2 — not implemented yet".to_string())
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

    /// v2: subscribe to `/api/event`, yielding [`EventEnvelope`] frames.
    fn event_stream(
        &self,
    ) -> impl Future<
        Output = Result<
            std::pin::Pin<
                Box<dyn futures_util::Stream<Item = Result<ClientEvent, String>> + Send>,
            >,
            String,
        >,
    > + Send {
        async move {
            let sse = crate::event::sse::SseClient::new(self.base.clone(), self.auth.clone());
            let stream = sse.stream_v2().await?;
            let mapped = futures_util::StreamExt::map(stream, |r| r.map(ClientEvent::V2));
            Ok(Box::pin(mapped)
                as std::pin::Pin<
                    Box<dyn futures_util::Stream<Item = Result<ClientEvent, String>> + Send>,
                >)
        }
    }
}
