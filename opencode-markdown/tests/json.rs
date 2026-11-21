//! The JSON the QML delegate reads.

use opencode_markdown::{Renderer, TRUNCATED_NOTE};
use serde_json::{json, Value};

fn json_of(body: &str, truncated: bool) -> Value {
    serde_json::from_str(&Renderer::new().render(body, truncated, false, 0).to_json()).unwrap()
}

#[test]
fn every_segment_field_is_carried() {
    let v = json_of("Hi **you**\n\n```sh\nls\n```", true);
    assert_eq!(v["keep"], json!(0));
    assert!(v["rev"].as_u64().unwrap() > 0);
    assert_eq!(
        v["segs"],
        json!([
            {"kind": "prose", "html": "Hi <b>you</b>", "text": "", "url": "", "lang": "", "indent": 0, "gap": false, "cells": []},
            {"kind": "code", "html": "", "text": "ls", "url": "", "lang": "sh", "indent": 0, "gap": true, "cells": []},
            {"kind": "marker", "html": "", "text": TRUNCATED_NOTE, "url": "", "lang": "", "indent": 0, "gap": false, "cells": []},
        ])
    );
}

#[test]
fn links_and_tables_name_their_kind() {
    let v = json_of("[t](https://e.com)\n\n| a |\n|---|\n| 1 |", false);
    assert_eq!(v["segs"][0]["kind"], json!("link"));
    assert_eq!(v["segs"][0]["url"], json!("https://e.com"));
    assert_eq!(v["segs"][1]["kind"], json!("table"));
    assert!(v["segs"][1]["html"].as_str().unwrap().starts_with("<table"));
    assert_eq!(v["segs"][1]["cells"], json!([["a"], ["1"]]));
}

#[test]
fn an_unchanged_body_sends_no_segments() {
    let mut r = Renderer::new();
    let a = r.render("same", false, false, 0);
    let v: Value = serde_json::from_str(&r.render("same", false, false, a.rev).to_json()).unwrap();
    assert_eq!(v["keep"], json!(1));
    assert_eq!(v["segs"], json!([]));
}

#[test]
fn text_needing_json_escapes_round_trips() {
    let v = json_of("`\"quoted\" \\ back\tslash`", false);
    assert!(v["segs"][0]["html"].as_str().unwrap().contains("&quot;quoted&quot; \\ back\tslash"));
}
