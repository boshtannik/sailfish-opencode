//! `Integration.*` — the v2 credentials/account surface.
//!
//! The v1 client signed the OpenCode account in by spawning
//! `opencode console login` and scraping its output. 2.0.32 dropped that
//! subcommand (it is `auth`/`api` now) and moved the same device flow behind
//! HTTP, so the client drives it directly:
//!
//! 1. `POST /api/integration/opencode/connect/oauth` `{methodID:"device"}` →
//!    [`Attempt`] (verification URL, user code, mode, deadline);
//! 2. `GET …/connect/oauth/{attemptID}` → [`AttemptStatus`] until the browser
//!    side authorizes;
//! 3. `POST …/{attemptID}/complete` persists the credential on the server.
//!
//! See `tests/fixtures/openapi-2.1.4.json` (`/api/integration*`) — the live
//! server answers exactly this shape (verified against `opencode serve` 2.1.4).

use serde::{Deserialize, Deserializer};

/// How the browser side confirms the attempt.
///
/// `auto` — the server polls the provider itself and the attempt completes on
/// its own (the OpenCode Console device flow). `code` — the user must paste a
/// code back (unused by the Console integration, kept because the wire union is
/// total: the server can send it for other integrations).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttemptMode {
    Auto,
    Code,
}

/// `time` of an attempt/status: epoch milliseconds.
///
/// The server serialises non-finite values as the *strings* `"Infinity"`,
/// `"-Infinity"`, `"NaN"` (see `Form` bounds), so a plain `f64` would fail to
/// deserialise. A non-number becomes `None` — "no deadline" — which the login
/// poller treats as "wait until the server says otherwise".
fn epoch_ms<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    })
}

/// `Integration.AttemptEncoded.time`.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct AttemptTime {
    #[serde(default, deserialize_with = "epoch_ms")]
    pub created: Option<f64>,
    #[serde(default, deserialize_with = "epoch_ms")]
    pub expires: Option<f64>,
}

/// `Integration.AttemptEncoded` — the answer of `connect/oauth`.
#[derive(Debug, Clone, Deserialize)]
pub struct Attempt {
    #[serde(rename = "attemptID")]
    pub attempt_id: String,
    /// Verification URL to open in the browser.
    pub url: String,
    /// Human instruction from the server (`"Enter code: XXXX-XXXX"`).
    #[serde(default)]
    pub instructions: String,
    pub mode: AttemptMode,
    #[serde(default)]
    pub time: AttemptTime,
}

impl Attempt {
    /// The user code to show big in the UI: `user_code=` of the verification
    /// URL, falling back to the tail of the server's instruction line.
    pub fn code(&self) -> String {
        if let Some(rest) = self.url.split("user_code=").nth(1) {
            let code = rest.split(['&', '#', ' ']).next().unwrap_or("").trim();
            if !code.is_empty() {
                return code.to_string();
            }
        }
        self.instructions
            .rsplit_once([':', ' '])
            .map(|(_, tail)| tail.trim().to_string())
            .unwrap_or_default()
    }
}

/// `Integration.AttemptStatus` — the wire union is total: four states.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum AttemptStatus {
    /// Waiting for the browser side.
    Pending {
        #[serde(default)]
        time: AttemptTime,
    },
    /// Authorized; the credential can be persisted (`complete`).
    Complete {
        #[serde(default)]
        time: AttemptTime,
    },
    /// The provider rejected the attempt (the message is human-readable).
    Failed {
        message: String,
        #[serde(default)]
        time: AttemptTime,
    },
    /// The user code is no longer valid.
    Expired {
        #[serde(default)]
        time: AttemptTime,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact payload `opencode serve` 2.0.32 answered on
    /// `POST /api/integration/opencode/connect/oauth` (captured live).
    #[test]
    fn console_device_attempt_parses() {
        let a: Attempt = serde_json::from_value(serde_json::json!({
            "attemptID": "con_17a404eb3001w13sUjKVoax5cl",
            "url": "https://opencode.ai/console/device?user_code=VQNH-RMPM&client_id=opencode-cli",
            "instructions": "Enter code: VQNH-RMPM",
            "mode": "auto",
            "time": {"created": 1793052397235u64, "expires": 1793052997235u64}
        }))
        .expect("attempt deserialises");
        assert_eq!(a.attempt_id, "con_17a404eb3001w13sUjKVoax5cl");
        assert_eq!(a.mode, AttemptMode::Auto);
        assert_eq!(a.code(), "VQNH-RMPM");
        assert_eq!(a.time.expires, Some(1793052997235.0));
    }

    /// A non-finite deadline arrives as a string, not a number.
    #[test]
    fn infinite_deadline_does_not_break_parsing() {
        let a: Attempt = serde_json::from_value(serde_json::json!({
            "attemptID": "con_1",
            "url": "https://example.org/?user_code=ABCD",
            "instructions": "",
            "mode": "code",
            "time": {"created": 1, "expires": "Infinity"}
        }))
        .expect("attempt deserialises");
        assert_eq!(a.time.expires, None);
        assert_eq!(a.code(), "ABCD");
    }

    #[test]
    fn status_union_is_total() {
        let pending: AttemptStatus = serde_json::from_value(serde_json::json!({
            "status": "pending", "time": {"created": 1, "expires": 2}
        }))
        .unwrap();
        assert!(matches!(pending, AttemptStatus::Pending { .. }));

        let complete: AttemptStatus = serde_json::from_value(serde_json::json!({
            "status": "complete", "time": {"created": 1, "expires": 2}
        }))
        .unwrap();
        assert!(matches!(complete, AttemptStatus::Complete { .. }));

        let failed: AttemptStatus = serde_json::from_value(serde_json::json!({
            "status": "failed", "message": "denied", "time": {"created": 1, "expires": 2}
        }))
        .unwrap();
        assert!(matches!(failed, AttemptStatus::Failed { ref message, .. } if message == "denied"));

        let expired: AttemptStatus = serde_json::from_value(serde_json::json!({
            "status": "expired", "time": {"created": 1, "expires": 2}
        }))
        .unwrap();
        assert!(matches!(expired, AttemptStatus::Expired { .. }));
    }

    /// The code comes from the URL; without one it is the instruction tail.
    #[test]
    fn code_falls_back_to_the_instruction() {
        let a = Attempt {
            attempt_id: "con".into(),
            url: "https://example.org/device".into(),
            instructions: "Enter code: WXYZ-1234".into(),
            mode: AttemptMode::Auto,
            time: AttemptTime::default(),
        };
        assert_eq!(a.code(), "WXYZ-1234");
    }
}
