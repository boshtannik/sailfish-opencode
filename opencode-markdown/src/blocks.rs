//! Rendering one leaf block (paragraph, heading, table, code, ...) into its
//! segments.

use pulldown_cmark::{Alignment, Event};

use crate::inline::{inline_html, table_html};
use crate::link_rows::{link_row, split_lines};
use crate::markup::{esc, TABLE_OPEN};
use crate::segment::{Kind, Segment};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum LeafKind {
    Paragraph,
    /// Inline content of a tight list item (no Paragraph tags around it).
    Implicit,
    Heading(u8),
    Table,
    Html,
    Code,
    Rule,
}

impl LeafKind {
    pub(crate) fn is_card(self) -> bool {
        matches!(self, LeafKind::Code | LeafKind::Table)
    }
}

pub(crate) struct Leaf<'a> {
    pub kind: LeafKind,
    pub events: Vec<Event<'a>>,
    /// Code: the fence language.
    pub lang: String,
    /// Table: the column alignments.
    pub aligns: Vec<Alignment>,
    /// Where the leaf (or the list item it opens) starts in the source.
    pub start: usize,
    pub indent: u8,
    /// The list bullet or number, when this leaf opens a list item.
    pub marker: Option<String>,
    /// The marker is a footnote label, which a link row must not replace.
    pub label: bool,
    pub quoted: bool,
}

pub(crate) fn render(leaf: &Leaf<'_>) -> Vec<Segment> {
    match leaf.kind {
        LeafKind::Code => vec![code(leaf)],
        LeafKind::Table => vec![table(leaf)],
        LeafKind::Html => vec![prose(leaf, html_block(&leaf.events))],
        LeafKind::Rule => vec![prose(leaf, "<hr/>".into())],
        LeafKind::Heading(level) => vec![prose(leaf, heading(level, &leaf.events))],
        LeafKind::Paragraph | LeafKind::Implicit => paragraph(leaf),
    }
}

/// A prose segment for the whole leaf, after its list marker.
fn prose(leaf: &Leaf<'_>, html: String) -> Segment {
    let marker = leaf.marker.as_deref().unwrap_or("");
    Segment { html: quote(leaf, format!("{marker}{html}")), ..Segment::new(Kind::Prose, leaf.indent) }
}

/// Quoted text is italic; its nesting is shown by the indent.
fn quote(leaf: &Leaf<'_>, html: String) -> String {
    if leaf.quoted { format!("<i>{html}</i>") } else { html }
}

fn code(leaf: &Leaf<'_>) -> Segment {
    let mut text = String::new();
    for ev in &leaf.events {
        if let Event::Text(t) = ev {
            text.push_str(t);
        }
    }
    let text = text.trim_end().chars().filter(|&c| c == '\n' || c == '\t' || !c.is_control()).collect();
    Segment { text, lang: leaf.lang.clone(), ..Segment::new(Kind::Code, leaf.indent) }
}

fn table(leaf: &Leaf<'_>) -> Segment {
    let (rows, cells) = table_html(&leaf.events, &leaf.aligns);
    let html = format!("{TABLE_OPEN}{rows}</table>");
    Segment { html: quote(leaf, html), cells, ..Segment::new(Kind::Table, leaf.indent) }
}

fn html_block(events: &[Event<'_>]) -> String {
    let mut raw = String::new();
    for ev in events {
        if let Event::Html(t) | Event::Text(t) = ev {
            raw.push_str(t);
        }
    }
    esc(raw.trim_end_matches('\n')).replace('\n', "<br/>")
}

fn heading(level: u8, events: &[Event<'_>]) -> String {
    let size = match level {
        1 => "+3",
        2 => "+2",
        3 => "+1",
        _ => "+0",
    };
    format!("<font size=\"{size}\"><b>{}</b></font>", inline_html(events))
}

/// Prose, with each line that is only a link split out as a link row. Runs
/// of other lines stay together as one prose segment.
fn paragraph(leaf: &Leaf<'_>) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut marker = paragraph_marker(leaf);
    let flush = |lines: &mut Vec<String>, marker: &mut Option<String>, out: &mut Vec<Segment>| {
        if !lines.is_empty() {
            let html = format!("{}{}", marker.take().unwrap_or_default(), lines.join("<br/>"));
            out.push(Segment { html: quote(leaf, html), ..Segment::new(Kind::Prose, leaf.indent) });
            lines.clear();
        }
    };
    for line in split_lines(&leaf.events) {
        let row = if leaf.label && marker.is_some() { None } else { link_row(line) };
        match row {
            Some((text, url)) => {
                flush(&mut lines, &mut marker, &mut out);
                // The row takes the place of the bullet.
                marker = None;
                out.push(Segment { text, url, ..Segment::new(Kind::Link, leaf.indent) });
            }
            None => lines.push(inline_html(line)),
        }
    }
    flush(&mut lines, &mut marker, &mut out);
    out
}

/// A task box replaces a bullet (not an ordered number).
fn paragraph_marker(leaf: &Leaf<'_>) -> Option<String> {
    let task = matches!(leaf.events.first(), Some(Event::TaskListMarker(_)));
    leaf.marker.clone().filter(|m| !task || m.starts_with("<b>"))
}
