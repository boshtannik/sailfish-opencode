//! Headings, paragraphs, rules and quotes, and the gaps between them.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn headings_are_sized_bold_text() {
    assert_eq!(one_html("# T"), "<font size=\"+3\"><b>T</b></font>");
    assert_eq!(one_html("## T"), "<font size=\"+2\"><b>T</b></font>");
    assert_eq!(one_html("### T"), "<font size=\"+1\"><b>T</b></font>");
    assert_eq!(one_html("#### T *i*"), "<font size=\"+0\"><b>T <i>i</i></b></font>");
}

#[test]
fn a_dashed_line_under_a_paragraph_makes_it_a_heading() {
    assert_eq!(one_html("Para\n---"), "<font size=\"+2\"><b>Para</b></font>");
}

#[test]
fn paragraphs_and_rules_are_separate_segments_with_source_gaps() {
    assert_eq!(
        segments("a\n\n***\n\nb", false),
        vec![prose("a"), gap(prose("<hr/>")), gap(prose("b"))]
    );
}

#[test]
fn quotes_are_italic_and_indented() {
    assert_eq!(segments("> hi\n> there", false), vec![ind(prose("<i>hi<br/>there</i>"), 1)]);
    assert_eq!(
        segments("> - a\n> - b", false),
        vec![ind(prose("<i>• a</i>"), 1), ind(prose("<i>• b</i>"), 1)]
    );
}
