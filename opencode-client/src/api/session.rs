use crate::api::OpenCodeClient;

#[derive(serde::Serialize)]
pub struct PromptBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messageID: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub parts: Vec<PromptPart>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
pub enum PromptPart {
    #[serde(rename = "text")]
    Text { text: String },
}

impl OpenCodeClient {
    /// POST /session -> create a new session.
    ///
    /// `title` is optional (`Session.CreateInput.title`): when absent/empty the
    /// server generates the session title itself.
    pub async fn create_session(&self, title: Option<&str>) -> Result<serde_json::Value, String> {
        let url = format!("{}/session", self.base);
        let body = match title {
            Some(t) if !t.trim().is_empty() => serde_json::json!({ "title": t.trim() }),
            _ => serde_json::json!({}),
        };
        let resp = self
            .authed(self.http.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// GET /session -> session list (recent ones usually on top).
    pub async fn list_sessions(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/session", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// PATCH /session/{id} -> rename a session.
    pub async fn rename_session(&self, session_id: &str, title: &str) -> Result<(), String> {
        let url = format!("{}/session/{session_id}", self.base);
        let resp = self
            .authed(self.http.patch(&url))
            .json(&serde_json::json!({ "title": title }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("rename status: {}", resp.status()));
        }
        Ok(())
    }

    /// DELETE /session/{id} -> delete a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let url = format!("{}/session/{session_id}", self.base);
        let resp = self
            .authed(self.http.delete(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("delete status: {}", resp.status()));
        }
        Ok(())
    }

    /// GET /session/{id} -> a single session (incl. `model.providerID`/`model.id`, `share.url`).
    pub async fn get_session(&self, session_id: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/session/{session_id}", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// POST /session/{id}/fork -> a new branch session. With `message_id` the
    /// branch is cut at that message (body `{messageID}`); without it the whole
    /// session is branched.
    pub async fn fork_session(
        &self,
        session_id: &str,
        message_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{}/session/{session_id}/fork", self.base);
        let body = match message_id {
            Some(m) if !m.is_empty() => serde_json::json!({ "messageID": m }),
            _ => serde_json::json!({}),
        };
        let resp = self
            .authed(self.http.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    /// POST /session/{id}/revert -> hide (revert) the tail starting at
    /// `message_id`. Fails while the session is busy.
    pub async fn revert_session(&self, session_id: &str, message_id: &str) -> Result<(), String> {
        let url = format!("{}/session/{session_id}/revert", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .json(&serde_json::json!({ "messageID": message_id }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("revert status: {}", resp.status()));
        }
        Ok(())
    }

    /// POST /session/{id}/unrevert -> restore previously reverted messages.
    pub async fn unrevert_session(&self, session_id: &str) -> Result<(), String> {
        let url = format!("{}/session/{session_id}/unrevert", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("unrevert status: {}", resp.status()));
        }
        Ok(())
    }

    /// POST /session/{id}/share -> session with a link in `share.url`.
    pub async fn share_session(&self, session_id: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/session/{session_id}/share", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// DELETE /session/{id}/share -> revoke link access.
    pub async fn unshare_session(&self, session_id: &str) -> Result<(), String> {
        let url = format!("{}/session/{session_id}/share", self.base);
        let resp = self
            .authed(self.http.delete(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("unshare status: {}", resp.status()));
        }
        Ok(())
    }

    /// POST /session/{id}/summarize -> compact the history (needs the session's providerID and modelID).
    pub async fn summarize_session(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        let url = format!("{}/session/{session_id}/summarize", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .json(&serde_json::json!({
                "providerID": provider_id,
                "modelID": model_id,
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("summarize status: {}", resp.status()));
        }
        Ok(())
    }

    /// GET /config/providers -> `{providers:[{id,name,models:{...}}]}`.
    pub async fn providers(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/config/providers", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// GET /session/{id}/todo -> the agent's task checklist.
    pub async fn session_todo(&self, session_id: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/session/{session_id}/todo", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// POST /api/session/{id}/model -> switch the session model.
    pub async fn set_model(
        &self,
        session_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        let url = format!("{}/api/session/{session_id}/model", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .json(&serde_json::json!({
                "model": { "id": model_id, "providerID": provider_id }
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("set_model status: {}", resp.status()));
        }
        Ok(())
    }

    /// POST /api/session/{id}/agent -> switch the session agent (Build/Plan).
    /// Changes the algorithm of subsequent turns: build — active work
    /// (files/commands), plan — planning without changes.
    pub async fn set_session_agent(&self, session_id: &str, agent: &str) -> Result<(), String> {
        let url = format!("{}/api/session/{session_id}/agent", self.base);
        let resp = self
            .authed(self.http.post(&url))
            .json(&serde_json::json!({ "agent": agent }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("set_session_agent status: {}", resp.status()));
        }
        Ok(())
    }

    /// GET /session/{id}/message -> `[{ info: Message, parts: [...] }]`.
    pub async fn session_messages(&self, session_id: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/session/{session_id}/message", self.base);
        let resp = self
            .authed(self.http.get(&url))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// POST /session/{id}/prompt_async — send a prompt without waiting for a response (SSE will deliver the result).
    /// `agent` (build/plan) must be passed on every prompt: without it the server
    /// resets the session agent to the default ("build"), which silently disables plan mode.
    pub async fn prompt_async(&self, session_id: &str, text: &str, agent: Option<String>) -> Result<(), String> {
        let url = format!("{}/session/{session_id}/prompt_async", self.base);
        let body = PromptBody {
            messageID: None,
            model: None,
            agent,
            parts: vec![PromptPart::Text { text: text.to_string() }],
        };
        let resp = self
            .authed(self.http.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("prompt_async status: {}", resp.status()));
        }
        Ok(())
    }

    /// POST /session/{id}/abort — abort execution.
    pub async fn abort(&self, session_id: &str) -> Result<bool, String> {
        let url = format!("{}/session/{session_id}/abort", self.base);
        let resp = self.authed(self.http.post(&url)).send().await.map_err(|e| e.to_string())?;
        Ok(resp.status().is_success())
    }
}