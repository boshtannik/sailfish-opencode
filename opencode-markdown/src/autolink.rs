//! Bare http(s) URLs in text become links. pulldown-cmark only links
//! `<http://…>`; it has no GFM extended-autolink option.

use crate::markup::{anchor, esc};

/// Escape `s` into `out`, turning bare http(s) URLs into anchors.
pub(crate) fn linkify(s: &str, out: &mut String) {
    let mut i = 0;
    while let Some(start) = next_url(s, i) {
        let url = trim_url_end(url_candidate(&s[start..]));
        let scheme = if url.starts_with("https://") { "https://".len() } else { "http://".len() };
        if url.len() <= scheme {
            out.push_str(&esc(&s[i..start + scheme]));
            i = start + scheme;
            continue;
        }
        out.push_str(&esc(&s[i..start]));
        out.push_str(&anchor(url, &esc(url)));
        i = start + url.len();
    }
    out.push_str(&esc(&s[i..]));
}

/// Byte offset of the next `http://`/`https://` at a word start.
fn next_url(s: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(off) = s[at..].find("http") {
        let p = at + off;
        let starts_word = s[..p]
            .chars()
            .last()
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '"' | '\''));
        if starts_word && (s[p..].starts_with("http://") || s[p..].starts_with("https://")) {
            return Some(p);
        }
        at = p + "http".len();
    }
    None
}

/// The URL-looking run at the start of `s`.
fn url_candidate(s: &str) -> &str {
    let len = s
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
        .unwrap_or(s.len());
    &s[..len]
}

/// Drop trailing punctuation, which belongs to the sentence. A closing
/// bracket stays only when the URL opened one (Wikipedia-style `Foo_(bar)`).
fn trim_url_end(mut url: &str) -> &str {
    while let Some(c) = url.chars().last() {
        let unbalanced = |open: char| url.matches(open).count() < url.matches(c).count();
        let trim = match c {
            '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '*' | '_' | '~' => true,
            ')' => unbalanced('('),
            ']' => unbalanced('['),
            _ => false,
        };
        if !trim {
            break;
        }
        url = &url[..url.len() - c.len_utf8()];
    }
    url
}
