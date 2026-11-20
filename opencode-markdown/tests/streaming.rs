//! Streaming: every update, applied to the segments the caller already
//! shows, equals a full parse of the new body, whatever the body does.

mod common;

use common::*;
use opencode_markdown::{segments, Renderer};

/// Documents whose restart points (top-level blocks and list items) cover
/// every block kind, repeated ordered numbers and open fences. Streaming them
/// makes the renderer restart parsing at each of those points.
const SAMPLES: &[&str] = &[
    ANSWER,
    "Steps:
3. not a list

3. three
4. four

```
unclosed

code",
    "para
---

* x
* y

  more y

    indented code

end",
    "1. a
1. b
1. c

<div>
raw
</div>

| a |
|---|
| 1 |

> q
> - i

last",
];

#[test]
fn every_streamed_update_equals_a_full_parse() {
    for doc in SAMPLES {
        stream(&mut Renderer::new(), doc);
    }
}

#[test]
fn a_shrunk_or_edited_body_is_still_correct() {
    let mut r = Renderer::new();
    let (shown, rev) = stream(&mut r, ANSWER);
    let edited = ANSWER.replace("Install", "Uninstall");
    let u = r.render(&edited, false, rev);
    assert_eq!(apply(&shown, &u), segments(&edited, false));
    let cut = &ANSWER[..ANSWER.len() / 2];
    let u2 = r.render(cut, false, u.rev);
    assert_eq!(apply(&segments(&edited, false), &u2), segments(cut, false));
}

#[test]
fn two_interleaved_streams_stay_correct_and_incremental() {
    let other = "Another answer:\n\n- x\n- y\n\n```\ncode\n```\n\nThe end.\n";
    let mut r = Renderer::new();
    let (mut sa, mut ra) = (Vec::new(), 0);
    let (mut sb, mut rb) = (Vec::new(), 0);
    let (ca, cb) = (chunks(ANSWER), chunks(other));
    for i in 0..ca.len().max(cb.len()) {
        if let Some(&c) = ca.get(i) {
            let u = r.render(&ANSWER[..c], false, ra);
            sa = apply(&sa, &u);
            ra = u.rev;
            assert_eq!(sa, segments(&ANSWER[..c], false));
        }
        if let Some(&c) = cb.get(i) {
            let u = r.render(&other[..c], false, rb);
            sb = apply(&sb, &u);
            rb = u.rev;
            assert_eq!(sb, segments(&other[..c], false));
        }
    }
    // Full parses happen only while a body is still a single block.
    let s = r.stats();
    assert!(s.partial_parses > 10 * s.full_parses, "{s:?}");
}

#[test]
fn reference_definitions_force_a_full_parse_and_stay_correct() {
    let body = "See [the docs][1] for more.\n\nMore text here.\n\n[1]: https://e.com\n";
    stream(&mut Renderer::new(), body);
}

#[test]
fn empty_and_garbage_bodies_stream_safely() {
    let mut r = Renderer::new();
    for body in ["", "\n\n", "[[[***```___|||", "|\n|-\n|", "- \n- \n-"] {
        stream(&mut r, body);
        r.render(body, true, 0);
    }
}

#[test]
fn crlf_streams_like_lf() {
    let crlf = ANSWER.replace('\n', "\r\n");
    let mut r = Renderer::new();
    let u = r.render(&crlf, false, 0);
    assert_eq!(u.segs, segments(ANSWER, false));
}

/// A line that first parses as its own block can later join the block above
/// it: a lone `|` becomes a table row, `#` after a quote becomes lazy quote
/// text once more characters arrive.
#[test]
fn a_new_line_absorbed_into_the_previous_block_stays_correct() {
    for (before, after) in [
        ("| a |\n|---|\n| 1 |\n|", "| a |\n|---|\n| 1 |\n| 2 |"),
        ("> quoted\n#", "> quoted\n#tag"),
        ("- item\n#", "- item\n#tag"),
    ] {
        let mut r = Renderer::new();
        let a = r.render(before, false, 0);
        assert_eq!(a.segs, segments(before, false));
        let b = r.render(after, false, a.rev);
        assert_eq!(apply(&a.segs, &b), segments(after, false), "{before:?} -> {after:?}");
    }
}

#[test]
fn reference_definitions_in_ordered_items_or_with_multi_line_labels_stay_correct() {
    stream(&mut Renderer::new(), "See [docs] here.\n\nMore text.\n\n1. [docs]: https://e.com\n\nAfter.\n");
    stream(&mut Renderer::new(), "See [a b] here.\n\nMore text.\n\n[a\nb]: https://e.com\n\nAfter.\n");
}

#[test]
fn blocks_indented_by_up_to_three_spaces_stay_correct() {
    for doc in [
        "Run:\n\n  ```sh\n  ls -l\n  ```\n\nDone.",
        "Run:\n\n   ~~~\n   a\n    b\n   ~~~\n\nDone.",
        "Items:\n\n  - a\n\n    more a\n  - b\n\nDone.",
        "Title\n\n  # Heading\n\n  > quote\n  > more\n\nDone.",
    ] {
        stream(&mut Renderer::new(), doc);
    }
}
