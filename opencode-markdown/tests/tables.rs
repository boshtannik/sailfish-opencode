//! Tables: cards of their own, with alignment and ragged rows.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn tables_render_as_html_tables_with_alignment() {
    assert_eq!(
        table_html("| a | b | c |\n|:--|--:|---|\n| 1 | **2** | `3` |"),
        format!(
            "{TABLE_OPEN}<tr><th align=\"left\">a</th><th align=\"right\">b</th><th>c</th></tr>\
             <tr><td align=\"left\">1</td><td align=\"right\"><b>2</b></td><td>{CODE_OPEN}3</font></td></tr></table>"
        )
    );
}

#[test]
fn ragged_rows_and_escaped_pipes() {
    assert_eq!(
        table_html("| a \\| b | c |\n|---|---|\n| 1 |"),
        format!("{TABLE_OPEN}<tr><th>a | b</th><th>c</th></tr><tr><td>1</td><td></td></tr></table>")
    );
}

const AB: &str = "<tr><th>a</th></tr><tr><td>1</td></tr></table>";

/// Tables scroll sideways in their own card, so like code they are always
/// gapped from their neighbours.
#[test]
fn a_table_right_after_a_paragraph_line_is_a_gapped_table_segment() {
    assert_eq!(
        segments("Results:\n| a |\n|---|\n| 1 |", false),
        [prose("Results:"), gap(table(&format!("{TABLE_OPEN}{AB}")))]
    );
    assert_eq!(segments("| a |\n|---|\n| 1 |\n\nafter", false)[1], gap(prose("after")));
}

#[test]
fn a_table_in_a_list_item_keeps_the_marker_on_its_own() {
    assert_eq!(
        segments("- | a |\n  |---|\n  | 1 |", false),
        vec![prose("•"), gap(ind(table(&format!("{TABLE_OPEN}{AB}")), 1))]
    );
}

#[test]
fn a_quoted_table_is_indented_and_italic() {
    assert_eq!(
        segments("> | a |\n> |---|\n> | 1 |", false),
        vec![ind(table(&format!("<i>{TABLE_OPEN}{AB}</i>")), 1)]
    );
}
