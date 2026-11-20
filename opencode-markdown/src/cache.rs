//! Rendered finished messages, so one scrolled back into view is not parsed
//! again.

use std::collections::HashMap;

use crate::parse::Parsed;

/// Upper bound for the cache: bodies plus rendered text.
pub const CACHE_BYTES: usize = 4 << 20;

#[derive(Default)]
pub(crate) struct Cache {
    map: HashMap<String, Parsed>,
    bytes: usize,
}

impl Cache {
    pub(crate) fn get(&self, body: &str) -> Option<&Parsed> {
        self.map.get(body)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn insert(&mut self, body: String, parsed: Parsed) {
        let size = body.len()
            + parsed
                .segs
                .iter()
                .map(|s| s.html.len() + s.text.len() + s.url.len() + s.lang.len())
                .sum::<usize>();
        if size > CACHE_BYTES {
            return;
        }
        // Dropping everything at the bound is simpler than tracking recency,
        // and the cache only saves re-parsing messages scrolled back into view.
        if self.bytes + size > CACHE_BYTES {
            self.map.clear();
            self.bytes = 0;
        }
        if self.map.insert(body, parsed).is_none() {
            self.bytes += size;
        }
    }
}
