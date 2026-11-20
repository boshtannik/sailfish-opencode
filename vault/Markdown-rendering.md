# Chat markdown rendering — design

Revised after a design review; performance while streaming is a primary
requirement.

## Problem

Chat bubbles render markdown with a hand-written, line-by-line regex converter
in `opencode-client/qml/pages/ChatPage.qml` (`mdInline` / `mdBlock`), after
`splitBody` has cut the body into segments at blank lines, fenced code blocks
and whole-line links. Because each source line is converted on its own:

- tables are not recognised: `| a | b |` and `|---|---|` show as raw text;
- nothing that spans lines works: tables, nested lists, list items that
  continue after a blank line;
- `-`/`*` bullets must start at column 0 (indented bullets and `+` are not
  lists); `~~strike~~`, `__bold__`, task lists and backslash escapes are
  missing;
- inline code is not protected, so `` `a**b**c` `` renders bold;
- fences are only found at column 0 and only as exactly three backticks, so
  a code block inside a list item, a `~~~` fence or a four-backtick fence
  shows as prose with literal fence lines and no copy button.

What works today and must keep working: bold-italic `***x***`, indented
ordered items, a hard line break at every single newline, bare URLs in prose
as links, link colour `#82c4ff`, inline-code colour `#e0e0e0`, think/service
styling, copy buttons on code blocks, link rows and the whole bubble.

`Text.MarkdownText` needs Qt 5.14; Sailfish OS ships Qt 5.6 (assumed from the
build image; to verify on the device with `rpm -q qt5-qtdeclarative`). Qt 5.6
`Text.RichText` renders HTML `<table>`, so the fix is a real parser emitting
Qt-compatible HTML.

## Overview

A new Rust crate without Qt dependencies parses with `pulldown-cmark` and cuts the
body into display segments with ready HTML. It keeps a little state so that a
streaming update only parses and renders the end of the message, and tells
QML which leading segments are unchanged. QML only applies the change.

## Crate: `opencode-markdown`

A separate crate next to `opencode-proto`, depending on `pulldown-cmark`
(`default-features = false`, version pinned; MSRV checked against the SDK's
rustc) and `serde_json` for the JSON QML reads. `opencode-client` links Qt
and builds for aarch64, so its tests only run under `make test-sdk`; a crate
without Qt runs under `make test` on the host. `make test` gains this crate,
and everything with logic lives in it: the client only forwards calls.

One module per job: `parse` (entry, restart points), `builder` (block
structure, nesting, gaps), `blocks` (one leaf block to segments),
`link_rows`, `inline` (inline markup and table cells), `markup` (escaping,
safe links), `autolink` (bare URLs), `renderer` and `cache` (incremental
updates), `holdback`, `json`.

### Segments

```text
Segment {
    kind:     prose | code | link | marker | table
    html:     String  // prose, table: Qt-RichText HTML; empty otherwise
    text:     String  // code: verbatim code; link: title; marker: text
    url:      String  // link only
    lang:     String  // code: fence language
    indent:   u8      // list and quote nesting depth
    gapBefore: bool   // QML draws the inter-segment gap only when true
}
```

Boundaries:

- between top-level blocks (paragraph, heading, table, quote, rule, ...);
- between **top-level list items**, so a long list is not one segment that is
  laid out again on every token;
- fenced and indented code blocks are lifted into their own `code`
  segments, also inside list items and quotes. The rest of the item after
  the code continues as a new prose segment with the item's `indent` and no
  bullet. Indented code is lifted because `<pre>` in a wrapping Label does
  not wrap and would overflow the bubble;
- tables become their own `table` segments, also inside list items and
  quotes, shown unwrapped in a horizontally scrolling card: a phone in
  portrait has too little width for most tables, and squeezing columns to
  word width makes them unreadable. Like a code card, a table never carries
  a list bullet; an item that starts with one shows its bullet on its own;
- a line that is exactly one link becomes a `link` segment: `[title](url)`,
  a bare URL, and those behind a `-`, `*`, `+` or `N.` marker, at any
  nesting depth;
- when the body was cut at the feed cap (60 000 chars), QML passes
  `truncated`; the parser sees only the cut body and the "… [text
  truncated]" note is its own trailing `marker` segment. Appended to the
  source, as today, it would land inside a table, paragraph or open fence.

### Spacing

Today `bodyCol.spacing` puts a gap at every segment boundary. With list
items as segments that would space every tight list, so:

- QML's Column spacing becomes 0 and each delegate gets a top margin of
  `Theme.paddingSmall` when `gapBefore` is set;
- `gapBefore` is true before and after every `code` and `table` segment
  (the cards), and otherwise
  only where the source has a blank line before the segment;
- the first segment never has a gap;
- lists always render tight. CommonMark's loose-list rule would re-space
  items already on screen as soon as a later blank line arrives.

Inside one segment, paragraphs (several in one item or quote) are joined
with `<br/><br/>`, never `<p>`, whose default Qt margins would differ from
the segment gaps.

### HTML for Qt 5.6 RichText

Keep today's device-proven markup:

| Source | HTML |
|---|---|
| soft line break | `<br/>`, as today and as GitHub comments; CommonMark would join the lines |
| bold / italic / strike | `<b>` / `<i>` / `<s>` (`<del>` is not in the Qt subset). Only `~~` strikes; `~5 min` must stay text |
| inline code | `<font face='monospace' color='#e0e0e0'>`, content escaped, never formatted |
| link, autolink, bare URL | `<a href="..." color="#82c4ff">`. Bare URLs are linked by our own pass over text events (pulldown-cmark has no GFM autolink extension), never inside code, trailing punctuation outside the link |
| heading | sized bold text, as today; `Para\n---` is a setext H2 |
| list item | bullet `•` or number written as text (Qt 5.6 support for `<ol start>` is unverified), indented by nesting depth |
| task item | `☐` / `☑` glyphs |
| quote | italic, prefixed as today (exact styling decided in the device spike) |
| table | `<table border="1" cellspacing="0" cellpadding="...">`, `<th>`, `<td align>` |
| image | alt text as a link to the URL. Never `<img>`: Qt RichText may fetch remote images (tracking, layout jumps) |
| raw HTML (block or inline) | escaped text, newlines as `<br/>` |
| hrefs | `"`, `'`, `<`, `&` escaped; `javascript:` and other non-http(s)/mailto schemes are not linked |

Table borders on a dark ambiance still need a look on the device.

Behaviour changes to state in the PR (they match CommonMark and the opencode
TUI): `__init__` renders as bold "init"; `Text\n2. x` is not a list;
`1. 1. 1.` renumbers to 1, 2, 3; `Para\n---` is a heading, not a rule.

## Performance: avoid re-rendering

Today only the paragraph being written is laid out again, and `MdCache.js`
reuses HTML when a delegate scrolls back in. The new path must do at least
as little work, per update, as today: cost proportional to the **end** of
the message, not its length.

### Rust: incremental parse with a checkpoint

The feed has no stable per-bubble id (a line is just its text), so state is
keyed by content.

- **Result cache.** Finished bodies (exact text → segments) in a bounded
  cache (4 MB of bodies plus rendered text, dropped whole at the bound),
  replacing `MdCache.js`. A delegate that scrolls back in costs one hash
  lookup. Bodies enter it when they leave the recent states below.
- **Recent states.** The last 8 rendered bodies, each with its segments and
  its **boundaries**: where every top-level block and top-level list item
  starts, how many segments precede it, and the number an ordered item
  shows there (source numbers can repeat, `1. 1. 1.`). An append replaces
  the state it grew from, so a stream holds one entry, not every prefix.
- **Append update.** When a new body starts with a recent state's body, Rust
  re-parses only from that state's **second-to-last** boundary and keeps
  the segments before it. The last block is not enough: a line that parsed
  as its own block can still join the block above once more text arrives
  (a lone `|` becoming a table row, `#` after a quote becoming lazy quote
  text). Only that one block can absorb it, and its first line is complete,
  so blocks before it are closed (the reference-definition exception is
  below).
- **Anything else** (an edit, a revert, a different message): full parse.
- **Link reference definitions** apply to the whole document, so a later
  `[1]: url` can change an earlier segment. If a body contains a definition,
  the parse is full for that body. LLM output rarely uses them.

### Streaming holdback

QML passes `live` for a bubble that is still streaming (it has a part id
and the session is busy). While live and the body does not end in a
newline, its last line may still change the block structure, and Rust
withholds it when it is ambiguous: only `-`, `=`, `*`, `+`, `_` and spaces;
only `#`; digits with an optional `.`/`)`; one or two backticks or tildes;
anything starting with `|`; or, after an optional list marker, a line that
may become a link row or a reference definition (`[…`, `http(s)://…` or a
prefix of `https://`). That removes the visible flips, for example
`Steps:\n-` flashing as a heading, or a link row turning back into prose.

Table rows start with `|`, so a row is shown once it is complete and a long
table is laid out once per row, not once per token. A complete `|` line
with no `|` line above it may be a header whose delimiter row has not
arrived; it is withheld too, so it never flashes as raw `| a | b |` text.

When the bubble stops being live, the binding re-renders it once with
`live` false and everything is shown.

### What Rust returns

`render(body, truncated, live, prev_rev) -> {rev, keep, segs}`: "keep your first
`keep` segments of the result `prev_rev`, replace the rest with `segs`".
Every result has a `rev`; QML stores it per delegate and passes it back.
When Rust no longer has `prev_rev` (or it is 0), it returns `keep: 0` and
every segment: correct by construction, just slower. Only the changed tail
is serialised and parsed in QML.

`Update::to_json` encodes it for QML. The client exposes `render` on its
own `markdown` context object, not on the `RefCell` app bridge, so it cannot
hit an active mutable borrow; the object only forwards to the crate.

### QML: apply only the change

`syncSegments` removes segments after `keep`, then for the tail sets only
the fields that differ (as today), so a Label whose `html` is unchanged is
not laid out again. A segment whose `kind` changes is recreated; with the
holdback this is rare.

### Measurement

The existing `gui_mark` diagnostics time each update. On the device, compare
today and the new path while streaming: a short answer, a 60 000-char answer,
a long table and a long list. The new path must not be slower than today on
any of them. Rust-side, a host test checks that the bytes parsed per
streamed update depend on the tail, not on the body length.

## Known remaining behaviour

- A table is one segment, laid out again when it changes (once per row,
  see above). Lists and quotes are not: every list item and every block
  inside a quote is its own segment.
- After the holdback, the remaining flips are rare: an earlier segment can
  change when a reference definition arrives, and an unclosed fence renders
  the rest of the message as code until it closes (as today).
- QML has no test harness: `syncSegments`, the spacing and the table layout
  are checked on the device by streaming a fixture through the feed.

## Tests (written first)

Rust tests in `opencode-markdown/tests/`, one file per topic, through the
crate's public API only, run by `make test`:

- **Blocks:** tables (alignment, inline markup in cells, ragged rows,
  escaped `\|`), a table right after a paragraph line; nested lists,
  indented bullets, `+`; an indented paragraph after a blank line stays in
  its item; numbering counted per list across lifted code and blank lines;
  setext headings; quotes containing lists and code; task lists.
- **Inline:** soft breaks become `<br/>`; inline code is not formatted
  inside; escapes; `~~x~~` strikes, `~5` does not; bare URLs linked with
  trailing punctuation left outside, never inside code; images never
  produce `<img>`; raw HTML escaped, including `<div>\n- a\n- b` and `<pre>`
  at the start of a line; `"`, `'` and `javascript:` in hrefs.
- **Segments:** fenced code (` ``` `, `~~~`, four backticks holding ```,
  info string with spaces) at top level, in list items and in quotes, and
  an unclosed fence inside a list item; indented code; link rows in every
  supported form; `gapBefore` per the spacing rules; `indent`; the
  truncation `marker`.
- **Input:** empty, whitespace only, CRLF, tabs, NUL and other control
  characters, invalid markup, a 60 000-char body.
- **Streaming:** a sample answer fed in token-sized chunks; for each step,
  segments before the restart point are unchanged, `{keep, segs}` applied to
  the previous result equals a full parse of the new body, and each
  holdback case (`Steps:\n-`, `a | b\n---`, `1. a\n2`, a link line followed
  by more text, `[1]: `) never shows its transient form.
- **Incremental equals full:** for every step, incremental output is
  identical to a full parse; a non-append update falls back to a full parse.
- **Caches:** an exact repeat is a cache hit; the caches stay within their
  byte bound.

## Commit plan

1. This design.
2. Add `opencode-markdown` with full parsing, segmenting and HTML, and
   `make test` running its tests. No app change.
3. Add the incremental `Renderer` and its cache, which needs parses that
   restart at a top-level boundary.
4. Add the streaming holdback.
5. Encode updates as the JSON QML reads.
6. Expose `render` to QML on its own context object (`make test-sdk`
   builds it).
7. Switch `ChatPage.qml` to the Rust segments, together with the
   `gapBefore` spacing that per-item segments need. `mdCode` and `htmlEsc`
   stay: the code card and tool lines use them. Tables show as prose.
8. Remove the now unused JS converter (`mdInline`, `mdBlock`, `mdRender`,
   `splitBody`) and `MdCache.js`.
9. Show tables in their horizontally scrolling card.
