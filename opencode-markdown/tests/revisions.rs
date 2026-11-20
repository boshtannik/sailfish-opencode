//! Revisions: `keep` tells the caller how many leading segments of what it
//! shows are unchanged, so it touches only the rest.

mod common;

use common::*;
use opencode_markdown::{segments, Renderer};

#[test]
fn an_unchanged_body_keeps_everything() {
    let mut r = Renderer::new();
    let first = r.render(ANSWER, false, false, 0);
    let again = r.render(ANSWER, false, false, first.rev);
    assert_eq!(again.keep, first.segs.len());
    assert!(again.segs.is_empty());
}

#[test]
fn an_unknown_previous_revision_gets_everything() {
    let mut r = Renderer::new();
    let u = r.render(ANSWER, false, false, 12345);
    assert_eq!(u.keep, 0);
    assert_eq!(u.segs, segments(ANSWER, false));
}

#[test]
fn an_append_keeps_the_unchanged_leading_segments() {
    let mut r = Renderer::new();
    let a = r.render("para one\n\npara two", false, false, 0);
    let b = r.render("para one\n\npara two grows", false, false, a.rev);
    assert_eq!(b.keep, 1);
    assert_eq!(b.segs.len(), 1);
}

#[test]
fn toggling_truncation_only_touches_the_marker() {
    let mut r = Renderer::new();
    let a = r.render(ANSWER, false, false, 0);
    let b = r.render(ANSWER, true, false, a.rev);
    assert_eq!(b.keep, a.segs.len());
    assert_eq!(b.segs, vec![segments("", true)[0].clone()]);
    let c = r.render(ANSWER, false, false, b.rev);
    assert_eq!(c.keep, a.segs.len());
    assert!(c.segs.is_empty());
}
