//! Inline links, bare URLs, images, reference links, and lines that become link rows.

mod common;

use common::*;
use opencode_markdown::segments;

#[test]
fn inline_links_are_coloured_anchors() {
    assert_eq!(
        one_html("see [the docs](https://e.com/x?a=1&b=2) now"),
        format!("see {} now", a("https://e.com/x?a=1&amp;b=2", "the docs"))
    );
}

#[test]
fn hrefs_escape_quotes() {
    assert_eq!(
        one_html("see [t](<https://e.com/\"x'>) now"),
        format!("see {} now", a("https://e.com/&quot;x&#39;", "t"))
    );
}

#[test]
fn unsafe_link_schemes_render_as_plain_text() {
    assert_eq!(one_html("see [t](javascript:alert(1)) now"), "see t now");
    assert_eq!(one_html("see [t](file:///etc/passwd) now"), "see t now");
}

#[test]
fn mailto_and_autolinks_are_links() {
    assert_eq!(one_html("mail <me@e.com> or <https://e.com>"),
        format!("mail {} or {}", a("mailto:me@e.com", "me@e.com"), a("https://e.com", "https://e.com")));
}

#[test]
fn bare_urls_become_links_without_trailing_punctuation() {
    assert_eq!(
        one_html("go to https://e.com/a. Then"),
        format!("go to {}. Then", a("https://e.com/a", "https://e.com/a"))
    );
    assert_eq!(
        one_html("(see https://en.wikipedia.org/wiki/Foo_(bar))"),
        format!("(see {})", a("https://en.wikipedia.org/wiki/Foo_(bar)", "https://en.wikipedia.org/wiki/Foo_(bar)"))
    );
    assert_eq!(
        one_html("a https://e.com/?q=1&r=2, b"),
        format!("a {}, b", a("https://e.com/?q=1&amp;r=2", "https://e.com/?q=1&amp;r=2"))
    );
}

#[test]
fn bare_urls_inside_code_or_links_are_not_linked_again() {
    assert_eq!(one_html("x `https://e.com` y"), format!("x {CODE_OPEN}https://e.com</font> y"));
    assert_eq!(
        one_html("x [https://a.com](https://b.com) y"),
        format!("x {} y", a("https://b.com", "https://a.com"))
    );
}

#[test]
fn images_become_links_never_img() {
    let h = one_html("![a cat](https://e.com/c.png) here");
    assert_eq!(h, format!("{} here", a("https://e.com/c.png", "a cat")));
    assert!(!segments("![x](https://e.com/i.png)", false).iter().any(|s| s.html.contains("<img")));
}

#[test]
fn reference_links_resolve() {
    assert_eq!(
        one_html("see [docs][1] now\n\n[1]: https://e.com"),
        format!("see {} now", a("https://e.com", "docs"))
    );
}

#[test]
fn a_line_that_is_only_a_link_is_a_link_row() {
    assert_eq!(segments("[t](https://e.com)", false), vec![link("t", "https://e.com")]);
    assert_eq!(segments("https://e.com", false), vec![link("https://e.com", "https://e.com")]);
    assert_eq!(segments("- [t](https://e.com)", false), vec![link("t", "https://e.com")]);
    assert_eq!(segments("1. https://e.com", false), vec![link("https://e.com", "https://e.com")]);
    assert_eq!(segments("- a\n  - [t](https://e.com)", false), vec![prose("• a"), ind(link("t", "https://e.com"), 1)]);
}

#[test]
fn link_row_title_is_plain_text() {
    assert_eq!(segments("[**bold** t](https://e.com)", false), vec![link("bold t", "https://e.com")]);
}

#[test]
fn link_lines_are_split_out_of_a_paragraph() {
    assert_eq!(
        segments("Intro\nhttps://e.com\n[b](https://b.com)\nOutro", false),
        vec![prose("Intro"), link("https://e.com", "https://e.com"), link("b", "https://b.com"), prose("Outro")]
    );
}

#[test]
fn a_link_with_more_text_on_its_line_stays_prose() {
    assert_eq!(
        one_html("https://e.com is good"),
        format!("{} is good", a("https://e.com", "https://e.com"))
    );
}

#[test]
fn an_unsafe_link_line_is_prose() {
    assert_eq!(segments("[t](javascript:x)", false), vec![prose("t")]);
}

#[test]
fn a_link_or_image_without_text_shows_its_url() {
    let url = "https://e.com/c.png";
    assert_eq!(one_html(&format!("see ![]({url}) here")), format!("see {} here", a(url, url)));
    assert_eq!(one_html(&format!("see [ ]({url}) here")), format!("see {} here", a(url, url)));
}

#[test]
fn an_unsafe_link_without_text_shows_its_url_as_text() {
    assert_eq!(one_html("see ![](javascript:x) here"), "see javascript:x here");
}
