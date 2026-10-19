//! opencode v2 form model (see `vault/Разведка-v2.md` §1.6) — replaces the v1
//! question model. A form has a title and a list of fields; the reply is
//! `{answer: {fieldID: value}}`.
//!
//! `Field` is a union by `type`; the answer value type depends on the field, so
//! `Answer` is a free-form JSON map validated by the caller.

use serde::Deserialize;


/// A form shown to the user (`Form.Info`).
#[derive(Debug, Clone, Deserialize)]
pub struct FormInfo {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub title: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub fields: Vec<Field>,
}

/// `Field` — union by `type`
/// (`string|number|integer|boolean|multiselect|external`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Field {
    #[serde(rename = "string")]
    String {
        id: String,
        label: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        required: Option<bool>,
    },
    #[serde(rename = "number")]
    Number {
        id: String,
        label: String,
        #[serde(default)]
        required: Option<bool>,
    },
    #[serde(rename = "integer")]
    Integer {
        id: String,
        label: String,
        #[serde(default)]
        required: Option<bool>,
    },
    #[serde(rename = "boolean")]
    Boolean {
        id: String,
        label: String,
        #[serde(default)]
        required: Option<bool>,
    },
    #[serde(rename = "multiselect")]
    Multiselect {
        id: String,
        label: String,
        #[serde(default)]
        options: Vec<String>,
        #[serde(default)]
        required: Option<bool>,
    },
    #[serde(rename = "external")]
    External {
        id: String,
        label: String,
    },
    #[serde(other)]
    Unknown,
}

/// Form state: `{status:"pending"}` or `{status:"answered", answer}` or
/// `{status:"cancelled", message?}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status")]
pub enum FormState {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "answered")]
    Answered { answer: serde_json::Value },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(other)]
    Unknown,
}
