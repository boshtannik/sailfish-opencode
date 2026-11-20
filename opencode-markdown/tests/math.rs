//! TeX math between `$` or `$$` is shown as written: markdown does not touch
//! it, and it is not typeset.

mod common;

use common::*;

#[test]
fn inline_math_is_not_formatted() {
    assert_eq!(one_html("so $a*b*c$ holds"), "so $a*b*c$ holds");
    assert_eq!(one_html("so $a_1 + b_2$ holds"), "so $a_1 + b_2$ holds");
}

#[test]
fn backslashes_in_math_are_kept() {
    assert_eq!(one_html(r"set $\{x\} \\ y$"), r"set $\{x\} \\ y$");
}

#[test]
fn math_is_html_escaped() {
    assert_eq!(one_html("if $a<b & c>d$"), "if $a&lt;b &amp; c&gt;d$");
}

#[test]
fn display_math_keeps_its_lines() {
    assert_eq!(
        one_html("$$\n\\begin{matrix} a \\\\ b \\end{matrix}\n$$"),
        r"$$<br/>\begin{matrix} a \\ b \end{matrix}<br/>$$"
    );
}

#[test]
fn dollar_amounts_are_not_math() {
    assert_eq!(one_html("pay $5 for **this** and $10 for that"), "pay $5 for <b>this</b> and $10 for that");
    assert_eq!(one_html("from $5 to $10, *roughly*"), "from $5 to $10, <i>roughly</i>");
}

#[test]
fn math_inside_code_is_code() {
    assert_eq!(one_html("run `echo $a*b*c$`"), format!("run {CODE_OPEN}echo $a*b*c$</font>"));
}
