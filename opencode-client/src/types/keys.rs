//! JSON field-name and discriminator constants for the opencode server objects
//! the client reads (session messages, their `info`, and message parts). Named
//! here so logic never compares or indexes by a raw string literal — a typo in a
//! bare `"..."` is invisible to the compiler, a typo in a `keys::`-constant is
//! not. Keep this in sync with `types/message.rs` / `types/tool.rs` `serde`
//! attributes and with the server's OpenAPI schema (`/doc`).

// ---- session-message item: `{ info: Message, parts: [Part] }` ----
pub const INFO: &str = "info";
pub const PARTS: &str = "parts";
pub const ID: &str = "id";
pub const ROLE: &str = "role";

// ---- `info.time` ----
pub const TIME: &str = "time";
pub const CREATED: &str = "created";
pub const START: &str = "start";

// ---- session-level revert marker ----
pub const REVERT: &str = "revert";
pub const MESSAGE_ID: &str = "messageID";

// ---- Part: `type` discriminator + its values ----
pub const TYPE: &str = "type";
pub const PART_TEXT: &str = "text";
pub const PART_REASONING: &str = "reasoning";
pub const PART_TOOL: &str = "tool";
pub const PART_COMPACTION: &str = "compaction";

// ---- Part fields ----
pub const TEXT: &str = "text";
pub const CALL_ID: &str = "callID";
pub const TOOL: &str = "tool";
pub const STATE: &str = "state";

// ---- Message role discriminator values ----
pub const ROLE_USER: &str = "user";

// ---- Message `mode` discriminator values ----
pub const MODE_COMPACTION: &str = "compaction";
