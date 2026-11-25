//! The display segments a chat body is cut into.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A Label showing `html`.
    Prose,
    /// A code card showing `text` verbatim, with a copy button.
    Code,
    /// A row showing `text` that opens `url`, with a copy button.
    Link,
    /// The truncation note, `text`.
    Marker,
    /// A table: `html` shown unwrapped in a horizontally scrolling card,
    /// since a phone in portrait has too little width for most tables.
    Table,
}

impl Kind {
    /// Code and tables are cards of their own, always gapped from their
    /// neighbours and never carrying a list bullet.
    pub fn is_card(self) -> bool {
        matches!(self, Kind::Code | Kind::Table)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub kind: Kind,
    /// Prose and table: Qt RichText HTML. Empty for other kinds.
    pub html: String,
    /// Code: the verbatim code. Link: the plain-text title. Marker: its text.
    pub text: String,
    /// Link only.
    pub url: String,
    /// Code: the fence's language (first word of the info string).
    pub lang: String,
    /// Nesting depth (list items and quotes) for the left margin, at most
    /// `MAX_INDENT`.
    pub indent: u8,
    /// Draw the inter-segment gap above this segment.
    pub gap_before: bool,
    /// Table only: each cell's plain text, row by row (the header first).
    pub cells: Vec<Vec<String>>,
}

pub const TRUNCATED_NOTE: &str = "… [text truncated]";

/// Deeper nesting shows at this indent: each level costs a margin, and a
/// phone in portrait runs out of width long before the source runs out of
/// levels.
pub const MAX_INDENT: u8 = 8;

impl Segment {
    pub(crate) fn new(kind: Kind, indent: u8) -> Self {
        Segment {
            kind,
            html: String::new(),
            text: String::new(),
            url: String::new(),
            lang: String::new(),
            indent,
            gap_before: false,
            cells: Vec::new(),
        }
    }

    pub(crate) fn truncation_note() -> Self {
        Segment { text: TRUNCATED_NOTE.into(), ..Segment::new(Kind::Marker, 0) }
    }
}
