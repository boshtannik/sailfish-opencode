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

// ── Streaming ────────────────────────────────────────────────────────────

use opencode_markdown::{Renderer, Update};

/// What QML does with an update: keep the first `keep`, replace the rest.
pub fn apply(shown: &[Segment], u: &Update) -> Vec<Segment> {
    assert!(u.keep <= shown.len(), "keep {} > shown {}", u.keep, shown.len());
    let mut v = shown[..u.keep].to_vec();
    v.extend(u.segs.iter().cloned());
    v
}

pub const ANSWER: &str = "# Plan\n\nIntro with **bold**, `code` and https://e.com.\n\n\
1. Install the package\n1. Configure it\n\n    Run this as root.\n\n1. Restart\n\n\
- a\n  - nested\n- [a link](https://e.com)\n- ```sh\n  ls -l\n  ```\n  after code\n\n\
> a quote\n> - with a list\n\n| col a | col b |\n|:--|--:|\n| 1 | **2** |\n| 3 | 4 |\n\n\
***\n\n```rust\nfn main() {\n\n    println!(\"hi\");\n}\n```\n\nSteps:\n- one\n- two\n\nDone.\n";

/// Deterministic token-sized cut points (1..=7 bytes apart, on char
/// boundaries).
pub fn chunks(s: &str) -> Vec<usize> {
    let mut cuts = Vec::new();
    let mut i = 0;
    let mut seed = 7u32;
    while i < s.len() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        i = (i + 1 + (seed >> 16) as usize % 7).min(s.len());
        while !s.is_char_boundary(i) {
            i += 1;
        }
        cuts.push(i);
    }
    cuts
}

/// Stream `body` through `r` the way the feed does and check
/// every step against a full parse. Returns what is shown and its rev.
pub fn stream(r: &mut Renderer, body: &str) -> (Vec<Segment>, u64) {
    let mut shown = Vec::new();
    let mut rev = 0;
    for cut in chunks(body) {
        let u = r.render(&body[..cut], false, rev);
        shown = apply(&shown, &u);
        assert_eq!(shown, segments(&body[..cut], false), "after {:?}", &body[..cut]);
        rev = u.rev;
    }
    (shown, rev)
}
