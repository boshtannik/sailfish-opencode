use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Deserialize)]
pub struct ModelRef {
    pub id: String,
    pub providerID: String,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Prompt {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionReason {
    Auto,
    Manual,
}

/// Turns a server error (`properties.error`) into a human-readable
/// message for the chat feed. Error types — a discriminated union
/// `{name, data}` from the server's OpenAPI spec (`/doc`), see the SDK types.
/// If `error` is absent — an empty string (we ignore the event).
pub fn human_session_error(error: Option<&serde_json::Value>) -> String {
    let Some(e) = error else { return String::new() };
    let name = e.pointer("/name").and_then(|v| v.as_str()).unwrap_or("");
    let data_message = e
        .pointer("/data/message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status_code = e
        .pointer("/data/statusCode")
        .and_then(|v| v.as_u64())
        .unwrap_or_default();

    match name {
        "ProviderAuthError" => {
            let provider = e
                .pointer("/data/providerID")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if data_message.is_empty() {
                format!("Провайдер {provider} не авторизован: добавьте ключ API")
            } else {
                format!("Провайдер {provider} не авторизован: {data_message}")
            }
        }
        "APIError" => {
            let is_retryable = e
                .pointer("/data/isRetryable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // 401/402/429 — quotas/access; the rest — a third-party incident.
            let tag = match status_code {
                401 | 402 | 429 => "ошибка доступа".to_string(),
                _ if !data_message.is_empty() => "ошибка провайдера".to_string(),
                _ => String::new(),
            };
            let mut s = String::new();
            if !tag.is_empty() {
                s = format!("[{tag}] ");
            }
            if status_code != 0 {
                s.push_str(&format!("код {status_code}; "));
            }
            if data_message.is_empty() {
                s.push_str("сервер ИИ не ответил (сторонняя ошибка)");
            } else {
                s.push_str(&data_message);
            }
            // Details are usually in `responseBody` (e.g. "the limit resets at 14:32").
            if let Some(body) = e.pointer("/data/responseBody").and_then(|v| v.as_str()) {
                let body = body.trim();
                if !body.is_empty() && !body.contains(data_message.trim()) {
                    s.push_str(". ");
                    s.push_str(body);
                }
            }
            if is_retryable {
                s.push_str(" (можно повторить)");
            }
            s
        }
        "MessageOutputLengthError" => {
            "Ответ превысил допустимую длину вывода модели".to_string()
        }
        "MessageAbortedError" => {
            if data_message.is_empty() {
                "Ответ прерван".to_string()
            } else {
                format!("Ответ прерван: {data_message}")
            }
        }
        "UnknownError" => {
            if data_message.is_empty() {
                "Неизвестная ошибка сервера".to_string()
            } else {
                data_message
            }
        }
        _ => {
            // Unknown type — log it (visibility into new server error kinds)
            // and return the raw object (for debugging new fields).
            log::warn!(
                "session.error: error type {name:?} is not implemented in the UI (showing raw object)"
            );
            if let Some(e) = e.as_object() {
                serde_json::to_string(e).unwrap_or_else(|_| "ошибка сервера".into())
            } else if !name.is_empty() {
                format!("{name}: {data_message}")
            } else {
                "ошибка сервера".to_string()
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionV2Source {
    #[serde(rename = "type")]
    pub kind: String,
    pub messageID: String,
    pub callID: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReplyKind {
    Once,
    Always,
    Reject,
}

impl PermissionReplyKind {
    /// Parse the QML-supplied reply name (`once` | `always` | `reject`).
    /// Serde is the single source of the names.
    pub fn from_name(name: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionTool {
    pub messageID: String,
    pub callID: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionInfo {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multiple: Option<bool>,
    pub custom: Option<bool>,
}