//! opencode v2 form model (opencode **2.1.4**) — the successor of the v1
//! question model. A form has a title and a list of fields; the reply is
//! `{answer: {fieldKey: value}}`.
//!
//! Source of truth: the vendored OpenAPI 3.1 spec of the real server
//! (`tests/fixtures/openapi-2.1.4.json`, schemas `Form.*`) — the same
//! fixture the client routes are checked against. Field naming follows it
//! exactly: the answer key is `key` (NOT `id`), the label is `title` (NOT
//! `label`), options are `{value,label,description}`.
//!
//! `Field` is a union of six variants upstream that share most properties,
//! so it is modelled as ONE struct with a `type` discriminator
//! ([`Field::kind`]) instead of a serde enum: the QML side switches on the
//! same discriminator, and unknown extra properties are tolerated (the
//! server may add fields later).

use serde::{Deserialize, Serialize};

use crate::types::v2::keys as k;

/// A form shown to the user (`Form.Info`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FormInfo {
    /// `frm_…`
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub fields: Vec<Field>,
}

/// One field of a form (`Form.Field`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Field {
    /// `type` — one of `string|number|integer|boolean|multiselect|external`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The key the answer is stored under (`Form.Answer` is keyed by it).
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// A hidden field is answered with its `default` — the user never sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Visibility condition (`eq`/`neq` against another field's answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<Choice>>,
    /// string: `email|uri|date|date-time`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// number/integer bounds (the server may send "Infinity"/"NaN" — hence
    /// the free-form value instead of `f64`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<serde_json::Value>,
    /// multiselect bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    /// string/multiselect: the user may type an answer of their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<bool>,
    /// external: the URL to open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Field {
    /// Is this field a string-ish one (its answer is a single text value)?
    pub fn is_text(&self) -> bool {
        self.kind == k::FIELD_STRING
    }

    /// Is this field a number-ish one (its answer is a number)?
    pub fn is_number(&self) -> bool {
        self.kind == k::FIELD_NUMBER || self.kind == k::FIELD_INTEGER
    }

    /// Is this field a boolean (its answer is `true`/`false`)?
    pub fn is_boolean(&self) -> bool {
        self.kind == k::FIELD_BOOLEAN
    }

    /// Is this field a multiselect (its answer is an array of option values)?
    pub fn is_multiselect(&self) -> bool {
        self.kind == k::FIELD_MULTISELECT
    }

    /// Is this field an external link (the user opens a URL and acknowledges)?
    pub fn is_external(&self) -> bool {
        self.kind == k::FIELD_EXTERNAL
    }
}

/// One option of a string/multiselect field (`Form.Option`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Choice {
    /// The value stored in the answer.
    pub value: String,
    /// The text shown to the user.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A visibility condition of a field (`Form.When`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Condition {
    /// The field whose answer is compared.
    pub key: String,
    /// `eq` | `neq`.
    pub op: String,
    pub value: serde_json::Value,
}

impl Condition {
    /// Does the condition hold for the given answers?
    ///
    /// The rules are the server's ones (opencode 2.1.4, `Form.When`): an
    /// unanswered field never satisfies a condition (the field stays hidden),
    /// `eq` compares the answer with the value, `neq` is its negation. An
    /// answer that is a list is compared element by element.
    pub fn holds(&self, answers: &serde_json::Map<String, serde_json::Value>) -> bool {
        let current = match answers.get(&self.key) {
            Some(v) => v,
            None => return false,
        };
        let equal = match current {
            serde_json::Value::Array(items) => items.iter().any(|i| i == &self.value),
            other => other == &self.value,
        };
        match self.op.as_str() {
            k::COND_EQ => equal,
            k::COND_NEQ => !equal,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The real 2.1.4 shape: `key`/`title`/`options[].value` (NOT `id`/`label`).
    #[test]
    fn form_info_parses_the_2_0_32_shape() {
        let f: FormInfo = serde_json::from_value(json!({
            "id": "frm_1",
            "sessionID": "ses_1",
            "title": "Which environment?",
            "fields": [
                {"type": "string", "key": "name", "title": "Name",
                 "description": "Your name", "required": true, "placeholder": "type here"},
                {"type": "multiselect", "key": "tags", "title": "Tags",
                 "options": [{"value": "a", "label": "A", "description": "first"},
                             {"value": "b", "label": "B"}],
                 "minItems": 1, "maxItems": 2, "custom": true}
            ]
        }))
        .expect("form deserialises");
        assert_eq!(f.id, "frm_1");
        assert_eq!(f.session_id, "ses_1");
        assert_eq!(f.title, "Which environment?");
        assert_eq!(f.fields.len(), 2);
        assert_eq!(f.fields[0].key, "name");
        assert_eq!(f.fields[0].title.as_deref(), Some("Name"));
        assert_eq!(f.fields[0].required, Some(true));
        assert!(f.fields[0].is_text());
        assert!(f.fields[1].is_multiselect());
        let opts = f.fields[1].options.as_ref().expect("options");
        assert_eq!(opts[0].value, "a");
        assert_eq!(opts[0].label, "A");
        assert_eq!(opts[1].value, "b");
    }

    #[test]
    fn every_field_type_is_recognised() {
        let f: FormInfo = serde_json::from_value(json!({
            "id": "frm_2", "sessionID": "ses_1", "title": "T",
            "fields": [
                {"type": "string", "key": "s"},
                {"type": "number", "key": "n", "minimum": 1, "maximum": 10},
                {"type": "integer", "key": "i"},
                {"type": "boolean", "key": "b", "default": true},
                {"type": "multiselect", "key": "m", "options": [{"value": "x", "label": "X"}]},
                {"type": "external", "key": "e", "url": "https://example.com"}
            ]
        }))
        .expect("form deserialises");
        assert!(f.fields[0].is_text());
        assert!(f.fields[1].is_number());
        assert!(f.fields[2].is_number());
        assert!(f.fields[3].is_boolean());
        assert!(f.fields[4].is_multiselect());
        assert!(f.fields[5].is_external());
        assert_eq!(f.fields[5].url.as_deref(), Some("https://example.com"));
    }

    /// The server may send "Infinity"/"NaN" as a bound — a typed f64 would
    /// drop the whole form.
    #[test]
    fn non_finite_bounds_do_not_break_parsing() {
        let f: FormInfo = serde_json::from_value(json!({
            "id": "frm_3", "sessionID": "ses_1", "title": "T",
            "fields": [{"type": "number", "key": "n", "maximum": "Infinity"}]
        }))
        .expect("form deserialises");
        assert_eq!(
            f.fields[0].maximum,
            Some(serde_json::Value::String("Infinity".into()))
        );
    }

    #[test]
    fn condition_evaluates_against_answers() {
        let c = Condition {
            key: "kind".into(),
            op: k::COND_EQ.into(),
            value: json!("prod"),
        };
        let mut answers = serde_json::Map::new();
        // No answer yet — the condition never holds (the field stays hidden).
        assert!(!c.holds(&answers));
        answers.insert("kind".into(), json!("prod"));
        assert!(c.holds(&answers));
        answers.insert("kind".into(), json!("dev"));
        assert!(!c.holds(&answers));

        // `neq` is the negation of `eq`.
        let neq_dev = Condition {
            key: "kind".into(),
            op: k::COND_NEQ.into(),
            value: json!("dev"),
        };
        assert!(!neq_dev.holds(&answers));
        let neq_prod = Condition {
            key: "kind".into(),
            op: k::COND_NEQ.into(),
            value: json!("prod"),
        };
        assert!(neq_prod.holds(&answers));
        // Even for `neq` a missing answer does not satisfy the condition.
        let empty = serde_json::Map::new();
        assert!(!neq_prod.holds(&empty));

        // A list answer is compared element by element.
        answers.insert("kind".into(), json!(["prod", "staging"]));
        assert!(c.holds(&answers));
        assert!(!neq_prod.holds(&answers));
        assert!(neq_dev.holds(&answers));
    }

    /// The form must survive the round trip to QML unchanged (the QML side
    /// reads the same JSON the server sent).
    #[test]
    fn form_serialises_back_to_the_server_shape() {
        let raw = json!({
            "id": "frm_4", "sessionID": "ses_1", "title": "T",
            "fields": [{"type": "string", "key": "a", "title": "A", "required": true}]
        });
        let f: FormInfo = serde_json::from_value(raw.clone()).expect("deserialises");
        let back = serde_json::to_value(&f).expect("serialises");
        assert_eq!(back["id"], raw["id"]);
        assert_eq!(back["sessionID"], raw["sessionID"]);
        assert_eq!(back["fields"][0]["key"], "a");
        assert_eq!(back["fields"][0]["title"], "A");
        assert_eq!(back["fields"][0]["type"], "string");
    }
}
