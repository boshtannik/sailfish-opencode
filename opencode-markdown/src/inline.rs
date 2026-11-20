//! Inline events (and the rows and cells of a table) to RichText.

use pulldown_cmark::{Alignment, Event, Tag, TagEnd};

use crate::autolink::linkify;
use crate::markup::{anchor_open, esc, safe_href, CODE_OPEN};

/// `aligns` are the column alignments when the events are a table's body.
pub(crate) fn inline_html(events: &[Event<'_>], aligns: &[Alignment]) -> String {
    let mut w = Writer { out: String::new(), text: String::new(), links: Vec::new(), table: Table::new(aligns) };
    for ev in events {
        w.event(ev);
    }
    w.flush_text();
    w.out
}

struct Writer<'t> {
    out: String,
    /// Consecutive text events, joined before escaping so a bare URL split
    /// across events is still found.
    text: String,
    links: Vec<OpenLink>,
    table: Table<'t>,
}

/// A link or image whose end has not been reached yet.
struct OpenLink {
    /// Whether it was rendered as an anchor (its URL is safe).
    anchored: bool,
    /// Where its text starts in the output.
    start: usize,
    url: String,
}

impl Writer<'_> {
    fn event(&mut self, ev: &Event<'_>) {
        if !matches!(ev, Event::Text(_)) {
            self.flush_text();
        }
        let out = &mut self.out;
        match ev {
            Event::Text(t) => self.text.push_str(t),
            Event::Code(t) => {
                out.push_str(CODE_OPEN);
                out.push_str(&esc(t));
                out.push_str("</font>");
            }
            Event::SoftBreak | Event::HardBreak => out.push_str("<br/>"),
            // Source HTML is shown as text, never interpreted.
            Event::InlineHtml(t) | Event::Html(t) => out.push_str(&esc(t)),
            // Math is not typeset: it is shown as written, delimiters included.
            Event::InlineMath(t) => out.push_str(&esc(&format!("${t}$"))),
            Event::DisplayMath(t) => out.push_str(&esc(&format!("$${t}$$")).replace('\n', "<br/>")),
            Event::TaskListMarker(done) => out.push_str(if *done { "☑ " } else { "☐ " }),
            Event::FootnoteReference(t) => out.push_str(&esc(&format!("[^{t}]"))),
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(*tag),
            // The builder takes rules out of the stream: one never reaches a leaf.
            Event::Rule => log::warn!("markdown: a rule inside inline content is ignored"),
        }
    }

    fn start(&mut self, tag: &Tag<'_>) {
        let out = &mut self.out;
        match tag {
            Tag::Emphasis => out.push_str("<i>"),
            Tag::Strong => out.push_str("<b>"),
            Tag::Strikethrough => out.push_str("<s>"),
            // Enabled only so single tildes are not struck through.
            Tag::Subscript => out.push('~'),
            Tag::Superscript => out.push('^'),
            // An image is shown as a link to it: Qt could fetch an `<img>`.
            Tag::Link { link_type, dest_url, .. } | Tag::Image { link_type, dest_url, .. } => {
                let href = safe_href(*link_type, dest_url);
                if let Some(url) = &href {
                    out.push_str(&anchor_open(url));
                }
                self.links.push(OpenLink { anchored: href.is_some(), start: out.len(), url: dest_url.to_string() });
            }
            Tag::TableHead => self.table.start_row(out, true),
            Tag::TableRow => self.table.start_row(out, false),
            Tag::TableCell => self.table.start_cell(out),
            // The builder takes block structure out of the stream (and the
            // parser options do not enable definition lists or metadata), so
            // none of these reach a leaf.
            Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::MetadataBlock(_) => log::warn!("markdown: block {tag:?} inside inline content is ignored"),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        let out = &mut self.out;
        match tag {
            TagEnd::Emphasis => out.push_str("</i>"),
            TagEnd::Strong => out.push_str("</b>"),
            TagEnd::Strikethrough => out.push_str("</s>"),
            TagEnd::Subscript => out.push('~'),
            TagEnd::Superscript => out.push('^'),
            TagEnd::Link | TagEnd::Image => {
                let Some(link) = self.links.pop() else { return };
                // Without visible text, the link would show nothing at all.
                if out[link.start..].trim().is_empty() {
                    out.truncate(link.start);
                    out.push_str(&esc(&link.url));
                }
                if link.anchored {
                    out.push_str("</a>");
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => self.table.end_row(out),
            TagEnd::TableCell => self.table.end_cell(out),
            // Their starts are reported in `start`; see there.
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    /// Text inside a link is already linked, so only text outside links is
    /// searched for bare URLs.
    fn flush_text(&mut self) {
        if self.links.is_empty() {
            linkify(&self.text, &mut self.out);
        } else {
            self.out.push_str(&esc(&self.text));
        }
        self.text.clear();
    }
}

struct Table<'t> {
    aligns: &'t [Alignment],
    head: bool,
    col: usize,
}

impl<'t> Table<'t> {
    fn new(aligns: &'t [Alignment]) -> Self {
        Table { aligns, head: false, col: 0 }
    }

    fn cell_tag(&self) -> &'static str {
        if self.head { "th" } else { "td" }
    }

    fn start_row(&mut self, out: &mut String, head: bool) {
        self.head = head;
        self.col = 0;
        out.push_str("<tr>");
    }

    fn start_cell(&self, out: &mut String) {
        let align = match self.aligns.get(self.col) {
            Some(Alignment::Left) => " align=\"left\"",
            Some(Alignment::Right) => " align=\"right\"",
            Some(Alignment::Center) => " align=\"center\"",
            _ => "",
        };
        out.push_str(&format!("<{}{align}>", self.cell_tag()));
    }

    fn end_cell(&mut self, out: &mut String) {
        out.push_str(&format!("</{}>", self.cell_tag()));
        self.col += 1;
    }

    /// Short (ragged) rows still get a cell per column.
    fn end_row(&mut self, out: &mut String) {
        while self.col < self.aligns.len() {
            out.push_str(&format!("<{0}></{0}>", self.cell_tag()));
            self.col += 1;
        }
        out.push_str("</tr>");
    }
}
