//! Contact group: email, url, phone number.

use std::sync::LazyLock;

use regex::Regex;

use super::Ctx;
use crate::scan::{Found, alnum_bounded, digits_of, is_space_like, next_char, prev_char, scan};

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        [\p{L}\p{N}_%+\-]+(?:\.[\p{L}\p{N}_%+\-]+)*      # local part, no leading/trailing dot
        @
        (?:[\p{L}\p{N}](?:[\p{L}\p{N}\-]*[\p{L}\p{N}])?\.)+
        \p{L}{2,}                                       # top-level domain
        ",
    )
    .unwrap()
});

pub(crate) fn email(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&EMAIL_RE, text, out, |m| {
        // Glued to more address characters on either side means we only saw part of it.
        let bad_prev = prev_char(text, m.start()).is_some_and(|c| c.is_alphanumeric() || c == '.' || c == '@');
        let bad_next = next_char(text, m.end()).is_some_and(|c| c.is_alphanumeric() || c == '@' || c == '-');
        (!bad_prev && !bad_next).then(|| Found::new(m.start(), m.end(), 0.95))
    });
}

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)(?:https?://|www\.)[^\s<>"'`«»“”„‚‘’|\\^{}]+"#).unwrap());

/// Drop trailing punctuation and closing brackets that do not belong to the URL.
fn trim_url(s: &str) -> usize {
    let mut end = s.len();
    loop {
        let Some(c) = s[..end].chars().next_back() else { return end };
        let drop = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '*' => true,
            ')' | ']' => {
                let (open, close) = if c == ')' { ('(', ')') } else { ('[', ']') };
                let body = &s[..end];
                body.matches(close).count() > body.matches(open).count()
            }
            _ => false,
        };
        if !drop {
            return end;
        }
        end -= c.len_utf8();
    }
}

pub(crate) fn url(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&URL_RE, text, out, |m| {
        if prev_char(text, m.start()).is_some_and(|c| c.is_alphanumeric() || matches!(c, '@' | '.' | '/' | '-')) {
            return None;
        }
        let end = m.start() + trim_url(m.as_str());
        let s = &text[m.start()..end];
        // Something after the scheme or "www." must look like a host.
        let rest = s.split_once("://").map_or(&s[4..], |(_, r)| r);
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        (host.contains('.') || host.starts_with("localhost")).then(|| Found::new(m.start(), end, 0.9))
    });
}

// Phone numbers: a start (`+`, `00`, `0`, `(0`) followed by digits, spaces, `/`, `-`, `–` and
// parentheses, ending in a digit. The structure is then checked in code.
static PHONE_INTL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\+|00)[1-9][0-9() \x{00A0}\x{202F}/\-–]*[0-9]").unwrap());
static PHONE_DE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(?0[1-9][0-9() \x{00A0}\x{202F}/\-–]*[0-9]").unwrap());
static PHONE_US_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\([2-9][0-9]{2}\)[ \x{00A0}]?|[2-9][0-9]{2}[ \x{00A0}.\-])[2-9][0-9]{2}[ \x{00A0}.\-][0-9]{4}")
        .unwrap()
});
static DATE_SHAPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[0-9]{1,2}[/\-][0-9]{1,2}[/\-][0-9]{2,4}|[0-9]{4}[/\-][0-9]{1,2}[/\-][0-9]{1,2})$").unwrap()
});
static AMOUNT_AFTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[.,][0-9]|[ \x{00A0}]?(?:€|\$|%|EUR\b|Euro\b|USD\b))").unwrap());

/// Checks digit groups and separators of a phone candidate.
fn phone_shape_ok(s: &str, min_digits: usize, max_digits: usize) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    if !(min_digits..=max_digits).contains(&digits) {
        return false;
    }
    if DATE_SHAPE_RE.is_match(s) {
        return false;
    }
    // Parentheses: balanced, not nested, digits only inside.
    let mut open = false;
    for c in s.chars() {
        match c {
            '(' if open => return false,
            '(' => open = true,
            ')' if !open => return false,
            ')' => open = false,
            c if open && !c.is_ascii_digit() => return false,
            _ => {}
        }
    }
    if open {
        return false;
    }
    // Separator runs: at most one of `/ - –`, at most one space on each side of it.
    let mut run = String::new();
    for c in s.chars().chain(std::iter::once('0')) {
        if c.is_ascii_digit() || c == '(' || c == ')' {
            if !run.is_empty() {
                let spaces = run.chars().filter(|&c| is_space_like(c)).count();
                let marks = run.chars().count() - spaces;
                let ok = match marks {
                    0 => spaces == 1,
                    1 => {
                        let cs: Vec<char> = run.chars().collect();
                        spaces <= 2 && !cs.windows(2).any(|w| is_space_like(w[0]) && is_space_like(w[1]))
                    }
                    _ => false,
                };
                if !ok {
                    return false;
                }
                run.clear();
            }
        } else {
            run.push(c);
        }
    }
    true
}

fn phone_context_ok(text: &str, start: usize, end: usize) -> bool {
    if prev_char(text, start).is_some_and(|c| c.is_alphanumeric() || matches!(c, '+' | '.' | ',' | '/' | '-' | '€' | '$')) {
        return false;
    }
    if next_char(text, end).is_some_and(|c| c.is_alphanumeric() || c == '/') {
        return false;
    }
    // "1 000 000 €", "0,50" and the like.
    let before = text[..start].trim_end_matches(is_space_like);
    if before.ends_with('€') || before.ends_with('$') || before.ends_with("EUR") {
        return false;
    }
    !AMOUNT_AFTER_RE.is_match(&text[end..])
}

pub(crate) fn phone_international(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&PHONE_INTL_RE, text, out, |m| {
        let ok = phone_context_ok(text, m.start(), m.end())
            && phone_shape_ok(m.as_str(), 7, 16)
            && !ctx.in_iban(m.start(), m.end());
        ok.then(|| Found::new(m.start(), m.end(), 0.9))
    });
}

pub(crate) fn phone_de(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&PHONE_DE_RE, text, out, |m| {
        // "00…" is international and handled there.
        let s = m.as_str().trim_start_matches('(');
        if s.starts_with("00") {
            return None;
        }
        let ok = phone_context_ok(text, m.start(), m.end())
            && phone_shape_ok(m.as_str(), 7, 14)
            && !ctx.in_iban(m.start(), m.end());
        ok.then(|| Found::new(m.start(), m.end(), 0.8))
    });
}

pub(crate) fn phone_us(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&PHONE_US_RE, text, out, |m| {
        let s = m.as_str();
        // Mixed separators such as "555.123-4567" are unlikely to be phone numbers.
        let seps: Vec<char> = s.chars().filter(|c| !c.is_ascii_digit() && !matches!(c, '(' | ')')).collect();
        let mixed = !s.starts_with('(') && seps.windows(2).any(|w| w[0] != w[1]);
        let ok = !mixed
            && digits_of(s).len() == 10
            && alnum_bounded(text, m.start(), m.end())
            && phone_context_ok(text, m.start(), m.end())
            && !ctx.in_iban(m.start(), m.end());
        ok.then(|| Found::new(m.start(), m.end(), 0.8))
    });
}
