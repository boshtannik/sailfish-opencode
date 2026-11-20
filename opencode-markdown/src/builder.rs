//! Turns parser events into segments: tracks list and quote nesting, gives
//! every leaf block and list item its own segment, decides gaps, and records
//! the restart points.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

use crate::blocks::{self, Leaf, LeafKind};
use crate::markup::esc;
use crate::parse::Boundary;
use crate::segment::{Kind, Segment, MAX_INDENT};

enum Ctx {
    Quote,
    List { next: Option<u64> },
    /// `marker` is taken by the item's first leaf; `start` is where the item
    /// begins in the source, for the blank-line check before that leaf.
    /// `label` marks a footnote definition, whose marker is its label.
    Item { marker: Option<String>, start: usize, label: bool },
}

pub(crate) struct Builder<'a> {
    /// The whole source; event ranges index it even for a suffix parse.
    src: &'a str,
    stack: Vec<Ctx>,
    leaf: Option<Leaf<'a>>,
    out: Vec<Segment>,
    /// Kind of the segment before the parsed part (suffix parses).
    prev: Option<Kind>,
    /// Number the first top-level ordered item shows (suffix parses).
    resume: Option<u64>,
    boundaries: Vec<Boundary>,
}

impl<'a> Builder<'a> {
    pub(crate) fn new(src: &'a str, prev: Option<Kind>, resume: Option<u64>) -> Self {
        Builder { src, stack: Vec::new(), leaf: None, out: Vec::new(), prev, resume, boundaries: Vec::new() }
    }

    /// The segments and restart points; boundary `seg`s count from 0.
    pub(crate) fn finish(mut self) -> (Vec<Segment>, Vec<Boundary>) {
        self.close_leaf();
        (self.out, self.boundaries)
    }

    pub(crate) fn event(&mut self, ev: Event<'a>, r: Range<usize>) {
        if self.stack.is_empty() && starts_block(&ev) {
            let at = line_start(self.src, r.start);
            self.boundaries.push(Boundary { at, seg: self.out.len(), next: None });
        }
        match ev {
            Event::Start(Tag::Paragraph) => self.open(LeafKind::Paragraph, r.start),
            Event::Start(Tag::Heading { level, .. }) => self.open(LeafKind::Heading(level as u8), r.start),
            Event::Start(Tag::HtmlBlock) => self.open(LeafKind::Html, r.start),
            Event::Start(Tag::Table(aligns)) => {
                self.open(LeafKind::Table, r.start);
                self.leaf_mut().aligns = aligns;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.open(LeafKind::Code, r.start);
                if let CodeBlockKind::Fenced(info) = kind {
                    self.leaf_mut().lang = info.split_whitespace().next().unwrap_or("").to_string();
                }
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::HtmlBlock | TagEnd::Table | TagEnd::CodeBlock,
            ) => self.close_leaf(),
            Event::Rule => {
                self.open(LeafKind::Rule, r.start);
                self.close_leaf();
            }
            Event::Start(Tag::BlockQuote(_)) => self.enter(Ctx::Quote),
            Event::Start(Tag::List(start)) => self.start_list(start),
            Event::Start(Tag::Item) => self.start_item(r.start),
            Event::End(TagEnd::Item) => self.end_item(),
            Event::Start(Tag::FootnoteDefinition(label)) => {
                self.close_implicit();
                let marker = Some(esc(&format!("[^{label}]: ")));
                self.stack.push(Ctx::Item { marker, start: r.start, label: true });
            }
            Event::End(TagEnd::FootnoteDefinition) => self.end_item(),
            Event::End(TagEnd::BlockQuote(_) | TagEnd::List(_)) => self.leave(),
            inline @ (Event::Start(
                Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::TableHead
                | Tag::TableRow
                | Tag::TableCell,
            )
            | Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell,
            )
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::TaskListMarker(_)) => {
                if self.leaf.is_none() {
                    self.open(LeafKind::Implicit, r.start);
                }
                self.leaf_mut().events.push(inline);
            }
            // The parser options do not enable these, so they never arrive.
            Event::Start(
                tag @ (Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::MetadataBlock(_)),
            ) => log::warn!("markdown: {tag:?} is not implemented; only its content is shown"),
            Event::End(
                TagEnd::DefinitionList
                | TagEnd::DefinitionListTitle
                | TagEnd::DefinitionListDefinition
                | TagEnd::MetadataBlock(_),
            ) => {}
        }
    }

    fn quotes(&self) -> usize {
        self.stack.iter().filter(|c| matches!(c, Ctx::Quote)).count()
    }

    fn items(&self) -> usize {
        self.stack.iter().filter(|c| matches!(c, Ctx::Item { .. })).count()
    }

    fn leaf_mut(&mut self) -> &mut Leaf<'a> {
        self.leaf.as_mut().expect("an open leaf")
    }

    fn enter(&mut self, ctx: Ctx) {
        self.close_implicit();
        // A quote or list that opens an item has no leaf to carry the item's
        // marker, so the marker goes on its own line before it.
        if let Some((marker, item_start, _)) = self.take_item_marker() {
            self.push_marker_only(marker.trim_end().to_string(), item_start);
        }
        self.stack.push(ctx);
    }

    fn leave(&mut self) {
        self.close_implicit();
        self.stack.pop();
    }

    fn start_list(&mut self, start: Option<u64>) {
        let next = match (self.stack.is_empty(), start) {
            (true, Some(_)) => self.resume.take().or(start),
            _ => start,
        };
        self.enter(Ctx::List { next });
    }

    fn start_item(&mut self, at: usize) {
        self.close_implicit();
        if let [Ctx::List { next }] = self.stack[..] {
            let at = line_start(self.src, at);
            self.boundaries.push(Boundary { at, seg: self.out.len(), next });
        }
        let marker = self.next_marker();
        self.stack.push(Ctx::Item { marker: Some(marker), start: at, label: false });
    }

    /// The bullet or number of the next item of the innermost list.
    fn next_marker(&mut self) -> String {
        let depth = self.stack.iter().filter(|c| matches!(c, Ctx::List { .. })).count();
        match self.stack.iter_mut().rev().find(|c| matches!(c, Ctx::List { .. })) {
            Some(Ctx::List { next: Some(n) }) => {
                *n += 1;
                format!("<b>{}</b>. ", *n - 1)
            }
            _ => match depth {
                0 | 1 => "• ",
                2 => "◦ ",
                _ => "▪ ",
            }
            .to_string(),
        }
    }

    fn end_item(&mut self) {
        self.close_implicit();
        // An empty item still shows its bullet or number.
        if let Some(Ctx::Item { marker: Some(m), start, .. }) = self.stack.last() {
            let (m, start) = (m.trim_end().to_string(), *start);
            self.push_marker_only(m, start);
        }
        self.stack.pop();
    }

    fn open(&mut self, kind: LeafKind, start: usize) {
        self.close_implicit();
        let quotes = self.quotes();
        let depth = quotes + self.items();
        let mut leaf = Leaf {
            kind,
            events: Vec::new(),
            lang: String::new(),
            aligns: Vec::new(),
            start,
            indent: indent(depth),
            marker: None,
            label: false,
            quoted: quotes > 0,
        };
        if let Some((marker, item_start, label)) = self.take_item_marker() {
            if kind.is_card() {
                // A card cannot carry a bullet: show it on its own.
                self.push_marker_only(marker.trim_end().to_string(), item_start);
            } else {
                leaf.marker = Some(marker);
                leaf.label = label;
                // The marker sits at the item's own depth, outside it.
                leaf.indent = indent(depth - 1);
                leaf.start = item_start;
            }
        }
        self.leaf = Some(leaf);
    }

    /// The enclosing item's marker, if no leaf of the item has taken it yet.
    fn take_item_marker(&mut self) -> Option<(String, usize, bool)> {
        match self.stack.last_mut() {
            Some(Ctx::Item { marker, start, label }) => marker.take().map(|m| (m, *start, *label)),
            _ => None,
        }
    }

    fn close_implicit(&mut self) {
        if self.leaf.as_ref().is_some_and(|l| l.kind == LeafKind::Implicit) {
            self.close_leaf();
        }
    }

    fn close_leaf(&mut self) {
        let Some(leaf) = self.leaf.take() else { return };
        for (i, s) in blocks::render(&leaf).into_iter().enumerate() {
            // Later segments of one leaf are lines of the same block, never
            // after a blank line.
            let start = if i == 0 { Some(leaf.start) } else { None };
            self.push(s, start);
        }
    }

    /// The item marker of a list item, alone on its line (before a card, or
    /// for an empty item). Its indent is the item's own.
    fn push_marker_only(&mut self, marker: String, start: usize) {
        let s = Segment { html: marker, ..Segment::new(Kind::Prose, indent(self.quotes() + self.items() - 1)) };
        self.push(s, Some(start));
    }

    /// Append a segment, deciding its gap: cards are always gapped, other
    /// segments only after a blank source line (`start` is where it begins).
    fn push(&mut self, mut s: Segment, start: Option<usize>) {
        if let Some(prev) = self.out.last().map(|p| p.kind).or(self.prev) {
            s.gap_before = s.kind.is_card()
                || prev.is_card()
                || start.is_some_and(|at| blank_line_before(self.src, at));
        }
        self.out.push(s);
    }
}

/// The indent of a segment `depth` quotes and items deep.
fn indent(depth: usize) -> u8 {
    depth.min(MAX_INDENT.into()) as u8
}

/// A block that can start at the top level (a list starts with its item).
fn starts_block(ev: &Event<'_>) -> bool {
    matches!(
        ev,
        Event::Rule
            | Event::Start(
                Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::HtmlBlock
                    | Tag::Table(_)
                    | Tag::CodeBlock(_)
                    | Tag::BlockQuote(_)
            )
    )
}

/// Whether the source line just above the one holding `start` is blank
/// (only whitespace and quote markers). Block ranges from the parser run on
/// past the blank lines that follow a block, so the gap is judged from the
/// next segment's side.
fn blank_line_before(src: &str, start: usize) -> bool {
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    if line_start == 0 {
        return false;
    }
    let prev_start = src[..line_start - 1].rfind('\n').map_or(0, |i| i + 1);
    src[prev_start..line_start - 1].chars().all(|c| c.is_whitespace() || c == '>')
}

/// The start of the line holding `at`, when only indentation precedes it.
/// A block's range begins after its indent, but the indent is part of the
/// block: a fence indented by two strips two spaces from its code lines, so
/// a parse restarted past the indent would keep them.
fn line_start(src: &str, at: usize) -> usize {
    let head = &src[..at];
    let indent = head.len() - head.trim_end_matches([' ', '\t']).len();
    let start = at - indent;
    if start == 0 || src.as_bytes()[start - 1] == b'\n' {
        start
    } else {
        at
    }
}
