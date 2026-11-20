//! Builders for expected segments, shared by the test files.
// Each test file uses only some of these.
#![allow(dead_code)]

use opencode_markdown::{segments, Kind, Segment};

pub const CODE_OPEN: &str = "<font face='monospace' color='#e0e0e0'>";

pub fn seg(kind: Kind, html: &str, text: &str, url: &str, lang: &str, indent: u8, gap: bool) -> Segment {
    Segment {
        kind,
        html: html.into(),
        text: text.into(),
        url: url.into(),
        lang: lang.into(),
        indent,
        gap_before: gap,
    }
}
pub fn prose(html: &str) -> Segment {
    seg(Kind::Prose, html, "", "", "", 0, false)
}
pub fn code(text: &str, lang: &str) -> Segment {
    seg(Kind::Code, "", text, "", lang, 0, false)
}
pub fn link(text: &str, url: &str) -> Segment {
    seg(Kind::Link, "", text, url, "", 0, false)
}
pub fn gap(mut s: Segment) -> Segment {
    s.gap_before = true;
    s
}
pub fn ind(mut s: Segment, n: u8) -> Segment {
    s.indent = n;
    s
}
pub fn a(url: &str, text: &str) -> String {
    format!("<a href=\"{url}\" color=\"#82c4ff\">{text}</a>")
}
pub fn one_html(body: &str) -> String {
    let s = segments(body, false);
    assert_eq!(s.len(), 1, "expected one segment for {body:?}, got {s:#?}");
    assert_eq!(s[0].kind, Kind::Prose, "{s:#?}");
    s[0].html.clone()
}

pub const TABLE_OPEN: &str = "<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\">";

/// The HTML of a body that is exactly one table segment.
pub fn table_html(body: &str) -> String {
    let s = segments(body, false);
    assert_eq!(s.len(), 1, "expected one segment for {body:?}, got {s:#?}");
    assert_eq!(s[0].kind, Kind::Table, "{s:#?}");
    s[0].html.clone()
}
pub fn table(html: &str) -> Segment {
    seg(Kind::Table, html, "", "", "", 0, false)
}
