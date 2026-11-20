//! Cost: a streaming append re-parses only the end of the message, and a
//! finished message shown again is not parsed again.

mod common;

use common::*;
use opencode_markdown::{segments, Renderer, CACHE_BYTES};

#[test]
fn appends_reparse_only_the_tail() {
    let body = ANSWER.repeat(20);
    let mut r = Renderer::new();
    let mut rev = 0;
    for cut in chunks(&body) {
        rev = r.render(&body[..cut], false, rev).rev;
        // The last top-level block of ANSWER is at most a few hundred bytes.
        assert!(r.stats().last_parsed_bytes < 400, "parsed {} bytes at {cut}", r.stats().last_parsed_bytes);
    }
    assert!(r.stats().partial_parses > r.stats().full_parses * 50);
}

#[test]
fn a_finished_body_shown_again_is_not_parsed_again() {
    let mut r = Renderer::new();
    stream(&mut r, ANSWER);
    // Push it out of the recent states with other messages, as scrolling does.
    for i in 0..50 {
        r.render(&format!("message {i}\n\n- item"), false, 0);
    }
    let before = r.stats();
    let u = r.render(ANSWER, false, 0);
    assert_eq!(u.segs, segments(ANSWER, false));
    let after = r.stats();
    assert_eq!(after.full_parses, before.full_parses);
    assert_eq!(after.partial_parses, before.partial_parses);
}

#[test]
fn the_cache_stays_within_its_byte_bound() {
    let mut r = Renderer::new();
    for i in 0..400 {
        let body = format!("{i}\n\n{}", "word ".repeat(10_000));
        r.render(&body, false, 0);
    }
    assert!(r.stats().cache_bytes <= CACHE_BYTES, "{:?}", r.stats());
}
