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

// ---- Tool-call input fields (used for the short tool label) ----
pub const COMMAND: &str = "command";
pub const FILE_PATH: &str = "filePath";
pub const PATH: &str = "path";
pub const PATTERN: &str = "pattern";
pub const QUERY: &str = "query";
pub const URL: &str = "url";
pub const PROMPT: &str = "prompt";
pub const DESCRIPTION: &str = "description";

// ---- Session object fields (`GET /session[/{id}]`) ----
pub const TITLE: &str = "title";
pub const UPDATED: &str = "updated";
pub const MODEL: &str = "model";
pub const PROVIDER_ID: &str = "providerID";
pub const SHARE: &str = "share";
pub const COST: &str = "cost";
pub const TOKENS: &str = "tokens";
