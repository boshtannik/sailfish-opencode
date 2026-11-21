//! The update as the JSON the QML delegate reads.

use serde_json::{json, Value};

use crate::renderer::Update;
use crate::segment::{Kind, Segment};

impl Update {
    /// `{"rev":n,"keep":k,"segs":[{"kind","html","text","url","lang","indent","gap"}]}`
    pub fn to_json(&self) -> String {
        let segs: Vec<Value> = self.segs.iter().map(segment_json).collect();
        json!({ "rev": self.rev, "keep": self.keep, "segs": segs }).to_string()
    }
}

fn segment_json(s: &Segment) -> Value {
    json!({
        "kind": kind_name(s.kind),
        "html": s.html,
        "text": s.text,
        "url": s.url,
        "lang": s.lang,
        "indent": s.indent,
        "gap": s.gap_before,
        "cells": s.cells,
    })
}

// A segment's `kind` in the JSON; ChatPage.qml mirrors them as `kKind…`.
pub const KIND_PROSE: &str = "prose";
pub const KIND_CODE: &str = "code";
pub const KIND_LINK: &str = "link";
pub const KIND_MARKER: &str = "marker";
pub const KIND_TABLE: &str = "table";

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Prose => KIND_PROSE,
        Kind::Code => KIND_CODE,
        Kind::Link => KIND_LINK,
        Kind::Marker => KIND_MARKER,
        Kind::Table => KIND_TABLE,
    }
}
