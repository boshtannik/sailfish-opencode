//! What part of a still-streaming body is safe to show. A line can flip
//! meaning as it is typed (`Steps:\n-` is a heading until the bullet's text
//! arrives), so such a line is held back until it is complete.

/// `body` without an unfinished last line that may still turn into something
/// else, and without a possible table header whose delimiter row has not
/// arrived (it would flash as raw `| a | b |` text).
pub(crate) fn holdback(body: &str) -> &str {
    without_table_header(without_ambiguous_tail(body))
}

fn without_ambiguous_tail(body: &str) -> &str {
    if body.is_empty() || body.ends_with('\n') {
        return body;
    }
    let start = body.rfind('\n').map_or(0, |i| i + 1);
    if ambiguous(&body[start..]) { &body[..start] } else { body }
}

fn without_table_header(body: &str) -> &str {
    let content = body.strip_suffix('\n').unwrap_or(body);
    let start = content.rfind('\n').map_or(0, |i| i + 1);
    let above = content[..start].strip_suffix('\n').map(last_line);
    if is_pipe_line(&content[start..]) && !above.is_some_and(is_pipe_line) {
        &body[..start]
    } else {
        body
    }
}

fn last_line(s: &str) -> &str {
    &s[s.rfind('\n').map_or(0, |i| i + 1)..]
}

fn is_pipe_line(l: &str) -> bool {
    l.trim_start_matches([' ', '>']).starts_with('|')
}

/// An unfinished line that may still change the block structure.
fn ambiguous(line: &str) -> bool {
    let t = line.trim_start_matches([' ', '>']).trim_end();
    !t.is_empty() && (is_marker_or_rule(t) || is_ordered_number(t) || may_become_link_row(strip_list_marker(t)))
}

/// A list marker, setext underline, rule, heading start, fence start or
/// table row in the making.
fn is_marker_or_rule(t: &str) -> bool {
    t.chars().all(|c| matches!(c, '-' | '=' | '*' | '+' | '_' | ' '))
        || t.chars().all(|c| c == '#')
        || t.starts_with('|')
        || matches!(t, "`" | "``" | "~" | "~~")
}

/// `2`, `12.` or `3)`: the start of an ordered item.
fn is_ordered_number(t: &str) -> bool {
    let digits = t.trim_end_matches(['.', ')']);
    !digits.is_empty() && digits.len() + 1 >= t.len() && digits.chars().all(|c| c.is_ascii_digit())
}

/// A line that may still become a link row or a reference definition: so far
/// nothing but one link, one bare URL or a `[label]:`. Text after the link
/// settles it as prose, so such a line is not held until its newline.
fn may_become_link_row(rest: &str) -> bool {
    if let Some(label) = rest.strip_prefix('[') {
        return match after_close(label, '[', ']') {
            Some(tail) => link_still_open(tail),
            None => true,
        };
    }
    if rest.starts_with("http://") || rest.starts_with("https://") {
        return !rest.contains(char::is_whitespace);
    }
    rest.len() < "https://".len() && "https://".starts_with(rest)
}

/// Whether what follows a link's `[text]` is still only the rest of that
/// link (`(url)` or `[ref]`, open or closed), or a definition's `:`.
fn link_still_open(tail: &str) -> bool {
    let rest = match tail.chars().next() {
        Some(':') => return true,
        Some('(') => after_close(&tail[1..], '(', ')'),
        Some('[') => after_close(&tail[1..], '[', ']'),
        _ => Some(tail),
    };
    match rest {
        Some(r) => r.trim().is_empty(),
        None => true,
    }
}

/// The text after the `shut` that closes a bracket already open before `s`,
/// counting nested pairs and skipping backslash escapes; `None` while it is
/// still open.
fn after_close(s: &str, open: char, shut: char) -> Option<&str> {
    let mut depth = 1;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == open {
            depth += 1;
        } else if c == shut {
            depth -= 1;
            if depth == 0 {
                return Some(&s[i + shut.len_utf8()..]);
            }
        }
    }
    None
}

fn strip_list_marker(t: &str) -> &str {
    if let Some(r) = t.strip_prefix(['-', '*', '+']).and_then(|r| r.strip_prefix(' ')) {
        return r.trim_start();
    }
    let n = t.chars().take_while(char::is_ascii_digit).count();
    match t[n..].strip_prefix(['.', ')']).and_then(|r| r.strip_prefix(' ')) {
        Some(r) if n > 0 => r.trim_start(),
        _ => t,
    }
}
