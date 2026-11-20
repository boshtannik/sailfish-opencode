//! Fenced and indented code: lifted into code cards wherever it appears.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn fenced_code_is_its_own_segment() {
    assert_eq!(segments("```rust\nfn x() {}\n```", false), vec![code("fn x() {}", "rust")]);
    assert_eq!(segments("~~~\nx\n~~~", false), vec![code("x", "")]);
    assert_eq!(segments("```rust title=x\ny\n```", false), vec![code("y", "rust")]);
    assert_eq!(segments("```\na\n\nb\n```", false), vec![code("a\n\nb", "")]);
}

#[test]
fn a_longer_fence_can_hold_a_shorter_one() {
    assert_eq!(segments("````\n```\ninner\n```\n````", false), vec![code("```\ninner\n```", "")]);
}

#[test]
fn code_keeps_markup_and_whitespace_verbatim() {
    assert_eq!(segments("```\n  <b>**x**</b>\n\ty\n```", false), vec![code("  <b>**x**</b>\n\ty", "")]);
}

#[test]
fn an_unclosed_fence_runs_to_the_end() {
    assert_eq!(segments("```\nabc\ndef", false), vec![code("abc\ndef", "")]);
}

#[test]
fn code_is_always_gapped_from_its_neighbours() {
    assert_eq!(
        segments("intro\n```\nx\n```\nafter", false),
        vec![prose("intro"), gap(code("x", "")), gap(prose("after"))]
    );
}

#[test]
fn indented_code_is_a_code_segment() {
    assert_eq!(segments("text\n\n    code here", false), vec![prose("text"), gap(code("code here", ""))]);
}

#[test]
fn code_inside_a_list_item_is_lifted_and_the_item_continues() {
    assert_eq!(
        segments("1. step:\n   ```sh\n   ls\n   ```\n   then this\n2. next", false),
        vec![
            prose("<b>1</b>. step:"),
            gap(ind(code("ls", "sh"), 1)),
            gap(ind(prose("then this"), 1)),
            prose("<b>2</b>. next"),
        ]
    );
}

#[test]
fn an_item_starting_with_code_keeps_its_marker() {
    assert_eq!(
        segments("- ```\n  x\n  ```", false),
        vec![prose("•"), gap(ind(code("x", ""), 1))]
    );
}

#[test]
fn an_unclosed_fence_inside_a_list_item_is_code() {
    assert_eq!(
        segments("- a\n  ```\n  b", false),
        vec![prose("• a"), gap(ind(code("b", ""), 1))]
    );
}

#[test]
fn code_inside_a_quote_is_lifted() {
    assert_eq!(segments("> ```\n> x\n> ```", false), vec![ind(code("x", ""), 1)]);
}
