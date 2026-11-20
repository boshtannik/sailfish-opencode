//! RichText markup primitives: escaping and safe links.

use pulldown_cmark::LinkType;

const LINK_COLOR: &str = "#82c4ff";
pub(crate) const CODE_OPEN: &str = "<font face='monospace' color='#e0e0e0'>";
pub(crate) const TABLE_OPEN: &str = "<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\">";

/// Escape text for RichText; control characters are dropped because Qt
/// renders them as boxes.
pub(crate) fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\n' | '\t' => o.push(c),
            c if c.is_control() => {}
            c => o.push(c),
        }
    }
    o
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('\'', "&#39;")
}

/// The link target to use, or None for schemes that must not be opened.
pub(crate) fn safe_href(kind: LinkType, dest: &str) -> Option<String> {
    let url = if kind == LinkType::Email { format!("mailto:{dest}") } else { dest.to_string() };
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")).then_some(url)
}

pub(crate) fn anchor_open(url: &str) -> String {
    format!("<a href=\"{}\" color=\"{LINK_COLOR}\">", esc_attr(url))
}

pub(crate) fn anchor(url: &str, inner_html: &str) -> String {
    format!("{}{inner_html}</a>", anchor_open(url))
}
