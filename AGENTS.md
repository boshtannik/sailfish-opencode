# AGENTS.md — notes for AI agents / contributors

`harbour-opencode` is a native opencode client for
Sailfish OS: Rust backend (`opencode-client/src`) + Silica QML UI
(`opencode-client/qml`). It bundles a native arm64 `opencode` CLI and talks to
`opencode serve` over HTTP + SSE.

## Принципы (developer's standing request)

These are not suggestions — they are the owner's explicit preferences. Keep them.

1. **Enums are exhaustive; keep every enum total.** Do not add an
   `Unknown`/`Other` catch-all to a domain enum (events, statuses, …). The
   compiler must be able to force us to revisit a `match` when the server's type
   union grows. The wire union lives in `opencode-client/src/types/event.rs`
   (`Event`, `#[serde(tag = "type")]`, no fallback).

2. **Match arms must be maximally filled.** Enumerate every variant explicitly.
   Do not collapse unhandled variants into a wildcard `_ =>` arm.

3. **Unimplemented branches must be loud, not silent.** When a branch of a
   `match` is not implemented yet, do not silently ignore it — emit a
   `log::warn!` naming *which* branch was taken and stating that it is *not
   implemented*. The goal: the logs must make the **degree of agreement between
   our UI and the native opencode server** visible at a glance. Example:
   `opencode-client/src/event/dispatcher.rs` (`note_unimplemented`) warns, once
   per event type, `"event '<type>' is not implemented in the UI yet — dispatch
   branch ignores it"`. Same idea in `human_session_error` (unknown error kind) and
   the history loader in `main.rs` (unknown message-part type).
   Rationale: the project may be released as open source; contributors must be
   able to see where the client lags the server.

   **Coverage is still partial.** The server's event union is large and only a
   fraction of event types are actually handled; the rest are logged and
   ignored. Which is which is decided in exactly one place:
   `opencode-client/src/event/dispatcher.rs` — the `dispatch` match is the
   single hub for "what a server event does". New handling starts there
   (streaming/error handlers already live next to it), and this file is where
   the degree of agreement with the server becomes visible.

4. **Log messages are in English.** This is an international / open-source
   project — every `log::warn!` / `log::error!` / `log::info!` message (and
   ideally code comments) is English, never Russian.

5. **Free-form JSON leaves are the only place for `serde_json::Value`.** If the
   server schema itself declares a field open (`additionalProperties: {}`, free
   `options`, …), typing it as `Value` is fine; a closed enum/object must be
   modelled as such.

6. **No raw string literals in logic — name every key/discriminator/value.**
   Never compare against, `match` on, or index a `serde_json::Value` by a bare
   string literal (`v["type"] == "tool"`, `map["created"]`, …). A typo in a
   literal is invisible to the compiler; a typo in a named constant is not.
   Declare constants and reference them:
   - server JSON field names / discriminator values: `opencode-client/src/types/keys.rs`;
   - feed-protocol markers (`[[tool]]`, `[[think]]`, …): `opencode-client/src/markers.rs`
     (mirrored as `k…` constants in the QML, e.g. `kToolTag`);
   - enum-like value sets: `types/tool.rs` (`STATUS_*`, `LABEL_INPUT_KEYS`).
   QML mirrors the Rust constants under the same names; keep both sides in sync.

7. **Fix only what is in our zone — leave model/server/upstream artifacts alone.**
   Before patching a reported defect, classify its origin. If the cause is the
   model or the opencode server (e.g. the model emits a stray `</think>` token
   as a `text` part, or produces duplicate output) and the official TUI behaves
   the same way on the same data, then it is **not our bug** and must **not** be
   "fixed" in the client. Our client mirrors the reference behavior; diverging
   from it to paper over upstream quirks is out of scope. Verify the reference
   behavior first (see the TUI snapshot in `/tmp/octui`, e.g. `src_rows.ts`),
   then either fix our side or explicitly decline. When declining, say so and
   do not touch the code.

## Build / test

- Build the RPM (authoritative gate — the host cannot `cargo check` because the
  build targets aarch64 via `.cargo/config.toml` and needs the cross toolchain):
  `sg docker -c "./mbuild.sh"`.
- Test the host-testable crates (protocol `opencode-proto`, chat markdown
  renderer `opencode-markdown`) on the host: `make test`.
- Test the `opencode-client` library (links Qt/SFOS, so it needs the SDK
  cross toolchain + qemu): `make test-sdk` (`sg docker -c
  "./scripts/test-sdk.sh"`). The host cannot run it.
- The bundled `opencode` CLI version/hash are pinned in `mbuild.sh`
  (`OC_VERSION`, `OC_SHA256`); bump both together and re-verify `serve` on the
  phone.
- This repo is not `rustfmt`-clean historically; do not reformat wholesale.

## More

Design notes and flows live in `vault/` (`План.md`, `Структура.md`,
`Флоу-UI.md`, `Диктовки.md`).
