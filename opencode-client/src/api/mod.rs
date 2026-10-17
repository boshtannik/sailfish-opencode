pub mod auth;
pub mod permission;
pub mod question;
pub mod session;

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