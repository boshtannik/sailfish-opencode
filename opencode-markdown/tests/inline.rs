//! Inline markup: line breaks, emphasis, code spans, escaping and source HTML.

mod common;

use common::*;

#[test]
fn single_newlines_stay_line_breaks() {
    assert_eq!(one_html("one\ntwo\nthree"), "one<br/>two<br/>three");
}

#[test]
fn emphasis_strong_and_bold_italic() {
    assert_eq!(
        one_html("*i* _j_ **b** __c__ ***d***"),
        "<i>i</i> <i>j</i> <b>b</b> <b>c</b> <i><b>d</b></i>"
    );
}

#[test]
fn double_tilde_strikes_but_single_tilde_is_text() {
    assert_eq!(one_html("~~gone~~"), "<s>gone</s>");
    assert_eq!(one_html("takes ~5 min and ~10 MB"), "takes ~5 min and ~10 MB");
}

#[test]
fn inline_code_is_escaped_and_never_formatted() {
    assert_eq!(one_html("`a**b**<c>`"), format!("{CODE_OPEN}a**b**&lt;c&gt;</font>"));
}

#[test]
fn text_is_html_escaped() {
    assert_eq!(one_html("a < b & c > d \"q\""), "a &lt; b &amp; c &gt; d &quot;q&quot;");
}

#[test]
fn backslash_escapes_show_the_literal_character() {
    assert_eq!(one_html("\\*not italic\\*"), "*not italic*");
}

#[test]
fn inline_html_is_shown_as_text() {
    assert_eq!(one_html("x <b>y</b>"), "x &lt;b&gt;y&lt;/b&gt;");
}

#[test]
fn html_blocks_are_shown_as_text() {
    assert_eq!(one_html("<div>\n- a\n- b"), "&lt;div&gt;<br/>- a<br/>- b");
    assert_eq!(one_html("<pre>\nx\n</pre>"), "&lt;pre&gt;<br/>x<br/>&lt;/pre&gt;");
}
