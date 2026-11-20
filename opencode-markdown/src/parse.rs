//! Parsing a body (or the rest of it from a restart point) into segments.

use std::borrow::Cow;

use pulldown_cmark::{Options, Parser};

use crate::builder::Builder;
use crate::segment::{Kind, Segment};

/// A point where parsing can restart: the start of a top-level block or of a
/// top-level list item. Everything before it is closed, so a suffix parse from
/// here yields the same segments as a full parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Boundary {
    /// Byte offset in the source.
    pub at: usize,
    /// Number of segments emitted before this point.
    pub seg: usize,
    /// For an ordered item, the number it shows (source numbers can repeat,
    /// e.g. `1. 1. 1.`, so a restart cannot read it from the text).
    pub next: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct Parsed {
    pub segs: Vec<Segment>,
    pub boundaries: Vec<Boundary>,
}

/// Parse `src[from.at..]` as if it followed the already-rendered
/// `src[..from.at]`; `prev` is the kind of the last segment before it (for
/// the gap rules). Boundaries are indexed as in a full parse.
pub(crate) fn parse_from(src: &str, from: Boundary, prev: Option<Kind>) -> Parsed {
    let mut b = Builder::new(src, prev, from.next);
    for (ev, r) in Parser::new_ext(&src[from.at..], options()).into_offset_iter() {
        b.event(ev, r.start + from.at..r.end + from.at);
    }
    let (segs, mut boundaries) = b.finish();
    for bd in &mut boundaries {
        bd.seg += from.seg;
    }
    Parsed { segs, boundaries }
}

pub(crate) fn parse(src: &str) -> Parsed {
    parse_from(src, Boundary { at: 0, seg: 0, next: None }, None)
}

fn options() -> Options {
    // SUBSCRIPT makes single tildes parse as a separate tag, which is rendered
    // back as literal `~`: with STRIKETHROUGH alone, "~5 min and ~10 MB"
    // style text could be struck through.
    // MATH keeps `$…$` and `$$…$$` away from inline markup and backslash
    // escapes, which would mangle TeX. FOOTNOTES makes `[^1]: x` a footnote
    // definition: otherwise a one-word or URL note is a link reference
    // definition, which hides the line and links every `[^1]`.
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_FOOTNOTES
}

/// Byte ranges must index the text that was parsed, and a lone `\r` would
/// otherwise survive into code blocks, so CR/CRLF are folded first.
pub(crate) fn normalize_newlines(s: &str) -> Cow<'_, str> {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n").into()
    } else {
        s.into()
    }
}
