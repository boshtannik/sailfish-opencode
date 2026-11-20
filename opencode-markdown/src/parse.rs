//! Parsing a body into segments.

use std::borrow::Cow;

use pulldown_cmark::{Options, Parser};

use crate::builder::Builder;
use crate::segment::Segment;

pub(crate) fn parse(src: &str) -> Vec<Segment> {
    let mut b = Builder::new(src);
    for (ev, r) in Parser::new_ext(src, options()).into_offset_iter() {
        b.event(ev, r);
    }
    b.finish()
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
