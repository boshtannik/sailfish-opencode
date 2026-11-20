//! Lines that are exactly one link become link rows (with a copy button)
//! instead of prose.

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::markup::safe_href;

/// Split inline events at line breaks that are not inside a span, so each
/// source line can be checked on its own.
pub(crate) fn split_lines<'e, 'a>(events: &'e [Event<'a>]) -> Vec<&'e [Event<'a>]> {
    let mut lines = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, ev) in events.iter().enumerate() {
        match ev {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::SoftBreak | Event::HardBreak if depth == 0 => {
                lines.push(&events[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    lines.push(&events[start..]);
    lines
}

/// `(title, url)` when the line is exactly one safe link or one bare URL.
pub(crate) fn link_row(line: &[Event<'_>]) -> Option<(String, String)> {
    let line: Vec<&Event> = line
        .iter()
        .filter(|e| !matches!(e, Event::Text(t) if t.trim().is_empty()))
        .collect();
    match line.first() {
        Some(Event::Start(Tag::Link { .. })) => single_link(&line),
        Some(Event::Text(_)) => bare_url(&line),
        _ => None,
    }
}

fn single_link(line: &[&Event<'_>]) -> Option<(String, String)> {
    let (Some(Event::Start(Tag::Link { link_type, dest_url, .. })), Some(Event::End(TagEnd::Link))) =
        (line.first(), line.last())
    else {
        return None;
    };
    let inner = &line[1..line.len() - 1];
    // A second link on the line makes it prose.
    if inner.iter().any(|e| matches!(e, Event::Start(Tag::Link { .. }))) {
        return None;
    }
    let url = safe_href(*link_type, dest_url)?;
    let title = inner
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            _ => None,
        })
        .collect();
    Some((title, url))
}

fn bare_url(line: &[&Event<'_>]) -> Option<(String, String)> {
    let mut text = String::new();
    for e in line {
        let Event::Text(t) = e else { return None };
        text.push_str(t);
    }
    let t = text.trim();
    let is_url = (t.starts_with("http://") || t.starts_with("https://"))
        && t.len() > "https://".len()
        && !t.chars().any(char::is_whitespace);
    is_url.then(|| (t.to_string(), t.to_string()))
}
