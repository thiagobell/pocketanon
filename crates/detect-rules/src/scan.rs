//! Matching helpers shared by all patterns. Everything works on byte offsets of the chunk text.

use pocketanon_core::Span;
use regex::{Match, Regex};

/// A raw rule match before it becomes a [`pocketanon_core::Candidate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Found {
    pub span: Span,
    pub confidence: f32,
}

impl Found {
    pub fn new(start: usize, end: usize, confidence: f32) -> Self {
        Found { span: Span::new(start, end), confidence }
    }
}

/// Space-like characters accepted between digit groups (space, NBSP, narrow NBSP).
pub(crate) fn is_space_like(c: char) -> bool {
    matches!(c, ' ' | '\u{00A0}' | '\u{202F}')
}

pub(crate) fn prev_char(text: &str, i: usize) -> Option<char> {
    text[..i].chars().next_back()
}

pub(crate) fn next_char(text: &str, i: usize) -> Option<char> {
    text[i..].chars().next()
}

/// `true` when no alphanumeric character touches the range on either side.
pub(crate) fn alnum_bounded(text: &str, start: usize, end: usize) -> bool {
    !prev_char(text, start).is_some_and(char::is_alphanumeric)
        && !next_char(text, end).is_some_and(char::is_alphanumeric)
}

fn glued(a: Option<char>, b: Option<char>, seps: &[char]) -> bool {
    a.is_some_and(|c| is_space_like(c) || seps.contains(&c)) && b.is_some_and(|d| d.is_ascii_digit())
}

/// `true` when a number starting at `start` is not the tail of a longer one: no alphanumeric
/// character right before it, and no digit behind a single separator (`1234 5678` must not
/// yield `5678`).
pub(crate) fn start_bounded(text: &str, start: usize, seps: &[char]) -> bool {
    let mut before = text[..start].chars().rev();
    let a = before.next();
    !a.is_some_and(char::is_alphanumeric) && !glued(a, before.next(), seps)
}

/// Mirror of [`start_bounded`] for the end of a number.
pub(crate) fn end_bounded(text: &str, end: usize, seps: &[char]) -> bool {
    let mut after = text[end..].chars();
    let a = after.next();
    !a.is_some_and(char::is_alphanumeric) && !glued(a, after.next(), seps)
}

/// [`start_bounded`] and [`end_bounded`] together.
pub(crate) fn number_bounded(text: &str, start: usize, end: usize, seps: &[char]) -> bool {
    start_bounded(text, start, seps) && end_bounded(text, end, seps)
}

/// Advance `i` by one character.
fn step(text: &str, i: usize) -> usize {
    i + next_char(text, i).map_or(1, char::len_utf8)
}

/// Run `re` over `text`, letting `accept` validate or shrink each match. A rejected match is
/// retried one character later, so that a bad left context does not hide a valid match inside it.
pub(crate) fn scan(re: &Regex, text: &str, out: &mut Vec<Found>, mut accept: impl FnMut(Match<'_>) -> Option<Found>) {
    let mut pos = 0;
    while pos < text.len() {
        let Some(m) = re.find_at(text, pos) else { break };
        match accept(m) {
            Some(f) if !f.span.is_empty() => {
                pos = f.span.end.max(step(text, m.start()));
                out.push(f);
            }
            _ => pos = step(text, m.start()),
        }
    }
}

/// For each match of `cue`, find the first `value` match starting within `max_gap` characters
/// after the cue that `accept` keeps. Only the value is reported, never the cue.
pub(crate) fn after_cue(
    cue: &Regex,
    value: &Regex,
    text: &str,
    max_gap: usize,
    out: &mut Vec<Found>,
    mut accept: impl FnMut(Match<'_>) -> Option<Found>,
) {
    for c in cue.find_iter(text) {
        let mut pos = c.end();
        while pos < text.len() {
            let Some(m) = value.find_at(text, pos) else { break };
            if text[c.end()..m.start()].chars().count() > max_gap {
                break;
            }
            if let Some(f) = accept(m) {
                out.push(f);
                break;
            }
            pos = step(text, m.start());
        }
    }
}

/// Byte positions of the ASCII alphanumeric characters in `s`, with `s` starting at `base`.
pub(crate) fn alnum_positions(s: &str, base: usize) -> Vec<(usize, u8)> {
    s.char_indices()
        .filter(|(_, c)| c.is_ascii_alphanumeric())
        .map(|(i, c)| (base + i, c as u8))
        .collect()
}

/// Only the ASCII digits of `s`.
pub(crate) fn digits_of(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}
