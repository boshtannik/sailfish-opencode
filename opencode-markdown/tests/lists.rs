//! List items: one segment each, with their bullet or number and nesting.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn every_list_item_is_its_own_tight_segment() {
    assert_eq!(segments("- a\n- b\n+ c", false)[..2], [prose("• a"), prose("• b")]);
    assert_eq!(segments("  * a\n  * b", false), vec![prose("• a"), prose("• b")]);
}

#[test]
fn nested_items_are_indented_with_their_own_bullet() {
    assert_eq!(
        segments("- a\n  - b\n    - c\n- d", false),
        vec![prose("• a"), ind(prose("◦ b"), 1), ind(prose("▪ c"), 2), prose("• d")]
    );
}

#[test]
fn ordered_lists_count_from_their_start() {
    assert_eq!(
        segments("1. a\n1. b\n1. c", false),
        vec![prose("<b>1</b>. a"), prose("<b>2</b>. b"), prose("<b>3</b>. c")]
    );
    assert_eq!(segments("3. a\n4. b", false), vec![prose("<b>3</b>. a"), prose("<b>4</b>. b")]);
}

#[test]
fn a_blank_line_between_items_gaps_only_that_item() {
    assert_eq!(
        segments("1. a\n2. b\n\n3. c", false),
        vec![prose("<b>1</b>. a"), prose("<b>2</b>. b"), gap(prose("<b>3</b>. c"))]
    );
}

#[test]
fn an_indented_paragraph_after_a_blank_line_stays_in_its_item() {
    assert_eq!(
        segments("1. Install\n\n    Run as root.\n\n2. Restart", false),
        vec![
            prose("<b>1</b>. Install"),
            gap(ind(prose("Run as root."), 1)),
            gap(prose("<b>2</b>. Restart")),
        ]
    );
}

#[test]
fn text_then_a_number_is_not_a_list() {
    assert_eq!(one_html("Text\n2. x"), "Text<br/>2. x");
}

#[test]
fn task_items_show_check_boxes() {
    assert_eq!(segments("- [ ] a\n- [x] b", false), vec![prose("☐ a"), prose("☑ b")]);
}

#[test]
fn an_item_that_opens_with_a_quote_or_a_list_shows_its_marker_first() {
    assert_eq!(segments("- > note", false), vec![prose("•"), ind(prose("<i>note</i>"), 2)]);
    assert_eq!(segments("1. - a", false), vec![prose("<b>1</b>."), ind(prose("◦ a"), 1)]);
}
