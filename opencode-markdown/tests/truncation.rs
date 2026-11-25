//! The truncation note: always its own last segment.

mod common;

use common::*;
use opencode_markdown::{segments, Kind, TRUNCATED_NOTE};

#[test]
fn truncation_note_is_its_own_last_segment() {
    let marker = seg(Kind::Marker, "", TRUNCATED_NOTE, "", "", 0, false);
    assert_eq!(segments("abc", true), vec![prose("abc"), marker.clone()]);
    assert_eq!(segments("```\ncode", true), vec![code("code", ""), marker.clone()]);
    assert_eq!(segments("| a |\n|---|\n| 1 |", true).last(), Some(&marker));
    assert_eq!(segments("", true), vec![marker]);
}
