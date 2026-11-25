//! Contract of the streaming holdback: while a bubble is live, an unfinished
//! last line that could still change the block structure is not shown yet,
//! so the feed never flashes a transient form. A finished bubble shows all.

mod common;

use common::*;
use opencode_markdown::{segments, Kind, Renderer, Segment};

fn live(body: &str) -> Vec<Segment> {
    Renderer::new().render(body, false, true, 0).segs
}

#[test]
fn ambiguous_unfinished_last_lines_are_withheld() {
    for (body, shown) in [
        ("Steps:\n-", "Steps:\n"),
        ("Steps:\n- ", "Steps:\n"),
        ("Title\n==", "Title\n"),
        ("a | b\n---", "a | b\n"),
        ("| a | b |\n|--", ""),
        ("| a | b |\n|---|---|\n| 1 | 2", "| a | b |\n|---|---|\n"),
        ("1. a\n2", "1. a\n"),
        ("1. a\n12.", "1. a\n"),
        ("text\n`", "text\n"),
        ("text\n~~", "text\n"),
        ("> quoted\n#", "> quoted\n"),
        ("See\n[1]: ", "See\n"),
        ("Intro\nhttps://e.co", "Intro\n"),
        ("Intro\n- [lin", "Intro\n"),
        ("Intro\n* * ", "Intro\n"),
        ("Intro\n[docs](https://e.com)", "Intro\n"),
        ("Intro\n[docs](https://e.com) ", "Intro\n"),
        ("Intro\n[docs](https://e.com/x_(y)", "Intro\n"),
        ("Intro\n[a [b] c", "Intro\n"),
        ("Intro\n[docs][1]", "Intro\n"),
        ("Intro\n[docs]", "Intro\n"),
        ("Intro\n[1]: https://e.com and", "Intro\n"),
        ("Intro\nhttps://e.com/a ", "Intro\n"),
    ] {
        assert_eq!(live(body), segments(shown, false), "{body:?}");
    }
}

#[test]
fn ordinary_unfinished_lines_stream_as_they_are_typed() {
    for body in ["Hello wor", "- item one is be", "Para\n\nNext para st", "| a | b |\n|---|---|\n| 1 | 2 |\n\nAfter"] {
        assert_eq!(live(body), segments(body, false), "{body:?}");
    }
}

#[test]
fn a_line_led_by_a_link_streams_once_text_follows_the_link() {
    for body in [
        "Intro\n[docs](https://e.com) and then mo",
        "Intro\n- [docs](https://e.com) explains i",
        "Intro\n[a [b] c](https://e.com/x_(y)) the",
        "Intro\n[docs][1] and mo",
        "Intro\n[docs] and mo",
        "Intro\n[docs]x",
        "Intro\nhttps://e.com is go",
    ] {
        assert_eq!(live(body), segments(body, false), "{body:?}");
    }
}

#[test]
fn a_streamed_link_led_line_shows_before_its_newline_and_never_as_a_link_row() {
    let line = "[the docs](https://e.com/x) explain this in detail";
    let body = format!("Intro\n{line}\n");
    let mut r = Renderer::new();
    let (mut shown, mut rev) = (Vec::new(), 0);
    for end in 1..=body.len() {
        let u = r.render(&body[..end], false, true, rev);
        shown = apply(&shown, &u);
        rev = u.rev;
        assert!(!shown.iter().any(|s| s.kind == Kind::Link), "link row at {:?}", &body[..end]);
    }
    let before_newline = &body[..body.len() - 1];
    assert_eq!(live(before_newline), segments(before_newline, false));
    assert_eq!(shown, segments(&body, false));
}

#[test]
fn a_finished_line_is_never_withheld() {
    for body in ["Steps:\n-\n", "1. a\n2\n", "Intro\nhttps://e.com\n"] {
        assert_eq!(live(body), segments(body, false), "{body:?}");
    }
}

#[test]
fn a_finished_bubble_shows_its_last_line() {
    let mut r = Renderer::new();
    let a = r.render("The answer is:\n42", false, true, 0);
    assert_eq!(a.segs, segments("The answer is:\n", false));
    let b = r.render("The answer is:\n42", false, false, a.rev);
    assert_eq!(apply(&a.segs, &b), segments("The answer is:\n42", false));
}

#[test]
fn streaming_never_shows_a_transient_heading_or_link_flip() {
    let body = "Steps:\n- one\n- two\n\nIntro\nhttps://e.com is good\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    let mut r = Renderer::new();
    let (mut shown, mut rev) = (Vec::new(), 0);
    for end in 1..=body.len() {
        let u = r.render(&body[..end], false, true, rev);
        shown = apply(&shown, &u);
        rev = u.rev;
        assert!(!shown.iter().any(|s| s.html.contains("<font size")), "heading at {:?}", &body[..end]);
        assert!(!shown.iter().any(|s| s.kind == Kind::Link), "link row at {:?}", &body[..end]);
        assert!(!shown.iter().any(|s| s.html.starts_with('|')), "raw table row at {:?}", &body[..end]);
    }
    assert_eq!(shown, segments(body, false));
}

#[test]
fn holdback_streams_stay_equal_to_a_full_parse_of_what_is_shown() {
    let body = "# T\n\n1. a\n1. b\n\n    more\n\n- x\n  - y\n\n```\ncode\n```\n\n> q\n#tag\n\n| a |\n|---|\n| 1 |\n";
    let mut r = Renderer::new();
    let (mut shown, mut rev) = (Vec::new(), 0);
    for end in 1..=body.len() {
        let u = r.render(&body[..end], false, true, rev);
        shown = apply(&shown, &u);
        rev = u.rev;
        let full = Renderer::new().render(&body[..end], false, true, 0).segs;
        assert_eq!(shown, full, "at {:?}", &body[..end]);
    }
}

#[test]
fn a_possible_table_header_waits_for_its_delimiter_row() {
    assert_eq!(live("Results:\n| a | b |\n"), segments("Results:\n", false));
    assert_eq!(live("Results:\n| a | b |\n|---|-"), segments("Results:\n", false));
    let table = "Results:\n| a | b |\n|---|---|\n";
    assert_eq!(live(table), segments(table, false));
}
