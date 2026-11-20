//! Footnotes: a reference shows as written, and a definition shows its label
//! and text where it stands. They are not linked or collected at the end.

mod common;

use common::*;
use opencode_markdown::{segments, Renderer};

#[test]
fn a_reference_shows_as_written() {
    assert_eq!(segments("see [^1] here\n\n[^1]: a note", false)[0], prose("see [^1] here"));
    assert_eq!(one_html("see [^x] here"), "see [^x] here");
}

#[test]
fn a_definition_shows_its_label_and_text() {
    assert_eq!(segments("x [^1]\n\n[^1]: Some note text.", false), vec![prose("x [^1]"), gap(prose("[^1]: Some note text."))]);
}

#[test]
fn a_one_word_definition_is_not_a_link_definition() {
    assert_eq!(segments("x [^n]\n\n[^n]: Word.", false), vec![prose("x [^n]"), gap(prose("[^n]: Word."))]);
}

#[test]
fn a_url_definition_keeps_its_label_and_links_the_url() {
    let url = "https://e.com/x";
    assert_eq!(
        segments(&format!("x [^1]\n\n[^1]: {url}"), false),
        vec![prose("x [^1]"), gap(prose(&format!("[^1]: {}", a(url, url))))]
    );
}

#[test]
fn a_definition_label_is_html_escaped() {
    assert_eq!(segments("[^a&b]: note", false), vec![prose("[^a&amp;b]: note")]);
}

#[test]
fn a_definition_continues_in_its_indented_paragraphs() {
    assert_eq!(
        segments("[^1]: first\n\n    second\n\nafter", false),
        vec![prose("[^1]: first"), gap(ind(prose("second"), 1)), gap(prose("after"))]
    );
}

#[test]
fn streamed_footnotes_equal_a_full_parse() {
    let body = "Claim one[^1] and two[^b].\n\n[^1]: https://e.com/source\n[^b]: Another note.\n\nAfter.\n";
    let (shown, _) = stream(&mut Renderer::new(), body);
    assert_eq!(shown, segments(body, false));
}
