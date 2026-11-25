//! Chat markdown -> display segments with Qt 5.6 RichText HTML.
//!
//! A chat bubble is a column of segments: prose (a Label showing `html`),
//! code and table cards, link rows, and the truncation note. Every leaf block
//! (paragraph, heading, table, code block, ...) and every list item starts a
//! new segment, so while an answer streams only the last, small segment
//! changes. Design: vault/Markdown-rendering.md.
//!
//! Qt 5.6 has no `Text.MarkdownText` and supports only a subset of HTML, so
//! lists, quotes and headings are drawn with plain markup and the segment's
//! `indent`, not with `<ul>`/`<blockquote>`/`<p>`.

mod autolink;
mod blocks;
mod builder;
mod cache;
mod holdback;
mod inline;
mod json;
mod link_rows;
mod markup;
mod parse;
mod renderer;
mod segment;

pub use cache::CACHE_BYTES;
pub use json::{KIND_CODE, KIND_LINK, KIND_MARKER, KIND_PROSE, KIND_TABLE};
pub use renderer::{Renderer, Stats, Update};
pub use segment::{Kind, Segment, MAX_INDENT, TRUNCATED_NOTE};

/// Cut `body` into display segments. `truncated` appends the truncation note
/// as its own segment, so it can never land inside a table, list or open
/// fence the way text appended to the source would.
pub fn segments(body: &str, truncated: bool) -> Vec<Segment> {
    let mut out = parse::parse(&parse::normalize_newlines(body)).segs;
    if truncated {
        out.push(Segment::truncation_note());
    }
    out
}
