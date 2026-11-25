//! Tables: cards of their own, with alignment, ragged rows, and the plain
//! text of every cell so the UI can measure columns.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn tables_render_as_html_tables_with_alignment() {
    assert_eq!(
        table_html("| a | b | c |\n|:--|--:|---|\n| 1 | **2** | `3` |"),
        format!(
            "{TABLE_OPEN}<tr><th align=\"left\" data-col=\"0\">a</th><th align=\"right\" data-col=\"1\">b</th>\
             <th data-col=\"2\">c</th></tr><tr><td align=\"left\" data-col=\"0\">1</td>\
             <td align=\"right\" data-col=\"1\"><b>2</b></td><td data-col=\"2\">{CODE_OPEN}3</font></td></tr></table>"
        )
    );
}

#[test]
fn ragged_rows_and_escaped_pipes() {
    assert_eq!(
        table_html("| a \\| b | c |\n|---|---|\n| 1 |"),
        format!(
            "{TABLE_OPEN}<tr><th data-col=\"0\">a | b</th><th data-col=\"1\">c</th></tr>\
             <tr><td data-col=\"0\">1</td><td data-col=\"1\"></td></tr></table>"
        )
    );
}

/// The UI measures each column from these to cap its width.
#[test]
fn cells_carry_plain_text_row_by_row_with_ragged_rows_padded() {
    let s = &segments("| a | **b** |\n|---|---|\n| `x` | [l](https://e.com) |\n| 1 |\n| \\| |", false)[0];
    assert_eq!(s.cells, [vec!["a", "b"], vec!["x", "l"], vec!["1", ""], vec!["|", ""]]);
}

#[test]
fn only_tables_carry_cells() {
    for s in segments("text\n\n```\ncode\n```\n\n[l](https://e.com)", true) {
        assert!(s.cells.is_empty(), "{s:#?}");
    }
}

const AB: &str = "<tr><th data-col=\"0\">a</th></tr><tr><td data-col=\"0\">1</td></tr></table>";
const AB_CELLS: &[&[&str]] = &[&["a"], &["1"]];

/// Tables scroll sideways in their own card, so like code they are always
/// gapped from their neighbours.
#[test]
fn a_table_right_after_a_paragraph_line_is_a_gapped_table_segment() {
    assert_eq!(
        segments("Results:\n| a |\n|---|\n| 1 |", false),
        [prose("Results:"), gap(table(&format!("{TABLE_OPEN}{AB}"), AB_CELLS))]
    );
    assert_eq!(segments("| a |\n|---|\n| 1 |\n\nafter", false)[1], gap(prose("after")));
}

#[test]
fn a_table_in_a_list_item_keeps_the_marker_on_its_own() {
    assert_eq!(
        segments("- | a |\n  |---|\n  | 1 |", false),
        vec![prose("•"), gap(ind(table(&format!("{TABLE_OPEN}{AB}"), AB_CELLS), 1))]
    );
}

#[test]
fn a_quoted_table_is_indented_and_italic() {
    assert_eq!(
        segments("> | a |\n> |---|\n> | 1 |", false),
        vec![ind(table(&format!("<i>{TABLE_OPEN}{AB}</i>"), AB_CELLS), 1)]
    );
}

/// Cell text is what the cell shows, so the UI measures the real width.
#[test]
fn cell_text_is_the_visible_text() {
    let cells = |body: &str| segments(body, false)[0].cells.clone();
    assert_eq!(cells("| a<br>b | x &amp; y |\n|---|---|"), [vec!["a<br>b", "x & y"]]);
    assert_eq!(cells("| ![alt](https://e.com/i.png) | https://e.com |\n|---|---|"), [vec!["alt", "https://e.com"]]);
    assert_eq!(cells("| `a<b>` | ~~gone~~ |\n|---|---|"), [vec!["a<b>", "gone"]]);
}

#[test]
fn a_header_only_table_has_one_row() {
    assert_eq!(segments("| a | b |\n|---|---|", false)[0].cells, [vec!["a", "b"]]);
}

#[test]
fn extra_cells_beyond_the_header_are_dropped_like_in_the_html() {
    let s = &segments("| a |\n|---|\n| 1 | 2 |", false)[0];
    assert_eq!(s.cells, [vec!["a"], vec!["1"]]);
    assert!(!s.html.contains('2'), "{}", s.html);
}

/// Every row of `cells` lines up with the `data-col` cells of its HTML row.
#[test]
fn cells_line_up_with_the_data_col_cells() {
    for body in [
        "| a | b | c |\n|---|---|---|\n| 1 |\n| 1 | 2 | 3 |",
        "| x |\n|:-:|\n| **y** |\n| `z` |",
        "| a \\| b | c |\n|---|---|\n| <i>raw</i> | [l](https://e.com) |",
    ] {
        let s = &segments(body, false)[0];
        let rows: Vec<&str> = s.html.split("<tr>").skip(1).collect();
        assert_eq!(rows.len(), s.cells.len(), "{body:?}");
        for (row, cells) in rows.iter().zip(&s.cells) {
            let cols: Vec<String> = (0..cells.len()).map(|c| format!("data-col=\"{c}\"")).collect();
            assert_eq!(row.matches("data-col=").count(), cells.len(), "{row}");
            assert!(cols.iter().all(|c| row.contains(c.as_str())), "{row}");
        }
    }
}
