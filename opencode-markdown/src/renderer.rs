//! Incremental rendering for the chat feed.
//!
//! The feed has no stable id per bubble (a line is only its text), so state
//! is keyed by content: a body that starts with a recently rendered body is
//! an append, and is re-parsed only from near that body's end. Callers pass
//! back the `rev` of what they show, and get only the segments after the
//! unchanged ones.

use std::collections::VecDeque;

use crate::cache::Cache;
use crate::holdback::holdback;
use crate::parse::{normalize_newlines, parse, parse_from, Parsed};
use crate::segment::Segment;

/// Recently rendered bodies kept for appends and for `keep` diffs: normally
/// one or two bubbles are streaming, the rest are delegates scrolling in.
const STATES: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub full_parses: u64,
    pub partial_parses: u64,
    /// Source bytes parsed by the latest call (0 for a reuse).
    pub last_parsed_bytes: usize,
    pub cache_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// Identifies this result; pass it back as `prev_rev` next time.
    pub rev: u64,
    /// Number of leading segments of the `prev_rev` result that are unchanged.
    pub keep: usize,
    /// The segments after those.
    pub segs: Vec<Segment>,
}

struct State {
    rev: u64,
    body: String,
    truncated: bool,
    parsed: Parsed,
}

impl State {
    /// What the caller shows for this state.
    fn shown(&self) -> Vec<Segment> {
        let mut segs = self.parsed.segs.clone();
        if self.truncated {
            segs.push(Segment::truncation_note());
        }
        segs
    }
}

/// Where a result came from: the state it reuses and how many of that
/// state's leading segments it took over unchanged.
struct Base {
    rev: u64,
    kept: usize,
}

#[derive(Default)]
pub struct Renderer {
    /// Most recent first.
    states: VecDeque<State>,
    cache: Cache,
    stats: Stats,
    last_rev: u64,
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> Stats {
        Stats { cache_bytes: self.cache.bytes(), ..self.stats }
    }

    /// Render `body`. `live` means the bubble is still streaming, so an
    /// unfinished tail that could change the block structure is held back.
    /// `prev_rev` is the revision the caller currently shows (0 when none).
    pub fn render(&mut self, body: &str, truncated: bool, live: bool, prev_rev: u64) -> Update {
        let body = normalize_newlines(body);
        let body = if live { holdback(&body) } else { &body }.to_string();
        self.stats.last_parsed_bytes = 0;

        let (parsed, base) = self.parse_reusing(&body);
        self.last_rev += 1;
        let state = State { rev: self.last_rev, body, truncated, parsed };
        let mut segs = state.shown();
        let keep = self.unchanged(prev_rev, &base, &segs);
        let rev = state.rev;
        self.remember(state, base);
        Update { rev, keep, segs: segs.split_off(keep) }
    }

    /// The parse of `body`, reusing earlier work where possible.
    fn parse_reusing(&mut self, body: &str) -> (Parsed, Option<Base>) {
        if let Some(s) = self.states.iter().find(|s| s.body == body) {
            let base = Base { rev: s.rev, kept: s.parsed.segs.len() };
            return (s.parsed.clone(), Some(base));
        }
        if let Some(p) = self.cache.get(body) {
            return (p.clone(), None);
        }
        if let Some((parsed, base, bytes)) = self.parse_append(body) {
            self.stats.partial_parses += 1;
            self.stats.last_parsed_bytes = bytes;
            return (parsed, Some(base));
        }
        self.stats.full_parses += 1;
        self.stats.last_parsed_bytes = body.len();
        (parse(body), None)
    }

    /// When `body` extends a recent state, re-parse only its end.
    fn parse_append(&self, body: &str) -> Option<(Parsed, Base, usize)> {
        // Link reference definitions apply to the whole document: a later
        // `[1]: url` changes earlier segments, so no restart is safe.
        if has_reference_definition(body) {
            return None;
        }
        let st = self
            .states
            .iter()
            .filter(|s| body.starts_with(s.body.as_str()))
            .max_by_key(|s| s.body.len())?;
        // Restart one block before the last: a line that parsed as the last
        // block can still be absorbed into the block above it once more text
        // arrives (a lone `|` becoming a table row, `#` becoming lazy quote
        // text). Absorption reaches only that one block, whose own first line
        // is complete.
        let i = st.parsed.boundaries.len().checked_sub(2)?;
        let from = st.parsed.boundaries[i];
        let prev = from.seg.checked_sub(1).map(|k| st.parsed.segs[k].kind);
        let tail = parse_from(body, from, prev);
        let mut segs = st.parsed.segs[..from.seg].to_vec();
        segs.extend(tail.segs);
        let mut boundaries = st.parsed.boundaries[..i].to_vec();
        boundaries.extend(tail.boundaries);
        let base = Base { rev: st.rev, kept: from.seg };
        Some((Parsed { segs, boundaries }, base, body.len() - from.at))
    }

    /// How many leading segments of the caller's `prev_rev` result `segs`
    /// leaves unchanged.
    fn unchanged(&self, prev_rev: u64, base: &Option<Base>, segs: &[Segment]) -> usize {
        let Some(prev) = self.states.iter().find(|s| prev_rev != 0 && s.rev == prev_rev) else {
            return 0;
        };
        let note = prev.truncated.then(Segment::truncation_note);
        let shown: Vec<&Segment> = prev.parsed.segs.iter().chain(note.iter()).collect();
        // Segments taken over from the caller's own state need no comparison.
        let mut k = match base {
            Some(b) if b.rev == prev.rev => b.kept.min(shown.len()),
            _ => 0,
        };
        while k < segs.len() && k < shown.len() && *shown[k] == segs[k] {
            k += 1;
        }
        k
    }

    fn remember(&mut self, state: State, base: Option<Base>) {
        // An append replaces the state it grew from, so the ring holds one
        // entry per stream and not every intermediate prefix.
        if let Some(b) = base {
            self.states.retain(|s| s.rev != b.rev);
        }
        self.states.retain(|s| s.body != state.body);
        self.states.push_front(state);
        while self.states.len() > STATES {
            if let Some(old) = self.states.pop_back() {
                // Bodies leaving the ring are mostly finished messages.
                self.cache.insert(old.body, old.parsed);
            }
        }
    }
}

/// Conservative: a `]:` anywhere, and a line that could start a link
/// reference definition (`[label`, also inside quotes and list items). The
/// `]:` is not looked for on that line: a label can span lines.
fn has_reference_definition(body: &str) -> bool {
    body.contains("]:")
        && body.lines().any(|l| {
            l.trim_start_matches(|c: char| {
                c.is_whitespace() || c.is_ascii_digit() || matches!(c, '>' | '-' | '*' | '+' | '.' | ')')
            })
            .starts_with('[')
        })
}
