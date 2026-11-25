//! Bodies that are empty, garbage, oversized, deeply nested or use other line
//! endings.

mod common;

use common::*;
use opencode_markdown::{segments, MAX_INDENT};

#[test]
fn empty_and_blank_bodies_have_no_segments() {
    assert_eq!(segments("", false), vec![]);
    assert_eq!(segments("  \n\n \t\n", false), vec![]);
}

#[test]
fn crlf_line_endings_behave_like_lf() {
    assert_eq!(segments("a\r\nb\r\n\r\nc", false), segments("a\nb\n\nc", false));
}

#[test]
fn control_characters_never_reach_the_html() {
    let h = one_html("a\u{0}b\u{7}c\u{1b}d");
    assert!(h.contains('a') && h.contains('d'), "{h}");
    assert!(!h.chars().any(|c| c.is_control()), "{h:?}");
}

#[test]
fn garbage_markup_does_not_panic() {
    for body in ["[[[***```___|||", "*_*_*_`", "|\n|-\n|", "<", "&", "\\", "1.", "- ", "> ", "#", "](", "![", "~~~~"] {
        let _ = segments(body, false);
        let _ = segments(body, true);
    }
}

#[test]
fn a_feed_cap_sized_body_renders() {
    let para = "Some **bold** text with `code` and a [link](https://example.com).\n\n";
    let body: String = para.repeat(60_000 / para.len() + 1);
    let s = segments(&body[..60_000], false);
    assert!(s.len() > 500);
}

#[test]
fn deep_nesting_does_not_panic_and_its_indent_is_capped() {
    let bodies = ["> ", "- ", "1. ", "> - "].map(|p| format!("{}x", p.repeat(300)));
    // Quote and item counts that only overflow once added up.
    let mixed = [format!("{}- x", "> ".repeat(255)), format!("{}> x", "- ".repeat(255))];
    for body in bodies.iter().chain(&mixed) {
        let prefix = &body[..body.len().min(8)];
        for s in segments(body, false) {
            assert!(s.indent <= MAX_INDENT, "{prefix:?}: {s:?}");
        }
    }
}
