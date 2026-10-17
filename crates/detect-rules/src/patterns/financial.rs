//! Identifiers group, financial part: iban, credit card number.

use std::sync::LazyLock;

use pocketanon_core::Span;
use regex::Regex;

use super::Ctx;
use crate::scan::{Found, alnum_positions, digits_of, is_space_like, next_char, prev_char, scan, start_bounded};
use crate::validators::{luhn, mod_97};

/// Upper-case country code, two check digits, then 11 to 30 alphanumerics with optional single
/// spaces (or NBSP) between them.
static IBAN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Z]{2}[0-9]{2}(?:[ \x{00A0}\x{202F}]?[A-Z0-9]){11,30}").unwrap());

/// IBAN lengths for SEPA and other common countries.
fn iban_length(country: &str) -> Option<usize> {
    Some(match country {
        "NO" => 15,
        "BE" => 16,
        "DK" | "FI" | "FO" | "GL" | "NL" => 18,
        "MK" | "SI" => 19,
        "AT" | "BA" | "EE" | "KZ" | "LT" | "LU" | "XK" => 20,
        "CH" | "CR" | "HR" | "LI" | "LV" => 21,
        "BG" | "BH" | "DE" | "GB" | "GE" | "IE" | "ME" | "RS" | "VA" => 22,
        "AE" | "GI" | "IL" | "TL" => 23,
        "AD" | "CZ" | "ES" | "MD" | "PK" | "RO" | "SA" | "SE" | "SK" | "TN" | "VG" => 24,
        "PT" | "ST" => 25,
        "IS" | "TR" => 26,
        "FR" | "GR" | "IT" | "MC" | "MR" | "SM" => 27,
        "AL" | "AZ" | "CY" | "DO" | "GT" | "HU" | "LB" | "PL" => 28,
        "BR" | "EG" | "PS" | "QA" | "UA" => 29,
        "JO" | "KW" | "MU" => 30,
        "MT" | "SC" => 31,
        "LC" => 32,
        _ => return None,
    })
}

/// One IBAN-shaped match: its span (cut to the country length when known) and, if the checksum
/// passes, the validated span.
struct IbanShape {
    shape: Span,
    valid: Option<Span>,
}

fn iban_shapes(text: &str) -> Vec<IbanShape> {
    let mut shapes = Vec::new();
    let mut pos = 0;
    while let Some(m) = IBAN_RE.find_at(text, pos) {
        let start = m.start();
        if prev_char(text, start).is_some_and(char::is_alphanumeric) {
            pos = start + 1;
            continue;
        }
        let chars = alnum_positions(m.as_str(), start);
        let end_after = |n: usize| chars[n - 1].0 + 1;
        let compact = |n: usize| chars[..n].iter().map(|&(_, c)| c as char).collect::<String>();
        // Lengths at which the match ends a token (next character is a space or not alphanumeric).
        let token_end = |n: usize| {
            let e = end_after(n);
            !next_char(text, e).is_some_and(char::is_alphanumeric)
        };

        let (shape, valid) = match iban_length(&m.as_str()[..2]) {
            Some(len) if chars.len() >= len => {
                let e = end_after(len);
                let ok = token_end(len) && mod_97(&compact(len));
                (Span::new(start, e), ok.then(|| Span::new(start, e)))
            }
            Some(_) => (Span::new(start, m.end()), None),
            None => {
                let valid = (15..=chars.len())
                    .rev()
                    .filter(|&n| token_end(n))
                    .find(|&n| mod_97(&compact(n)))
                    .map(|n| Span::new(start, end_after(n)));
                (valid.unwrap_or(Span::new(start, m.end())), valid)
            }
        };
        pos = shape.end;
        shapes.push(IbanShape { shape, valid });
    }
    shapes
}

/// Spans of everything that looks like an IBAN, used to keep other rules off IBAN digits.
pub(crate) fn iban_like_spans(text: &str) -> Vec<Span> {
    iban_shapes(text).into_iter().map(|s| s.shape).collect()
}

pub(crate) fn iban(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    out.extend(iban_shapes(ctx.text).into_iter().filter_map(|s| s.valid).map(|s| Found::new(s.start, s.end, 1.0)));
}

/// 13 to 19 digits with optional single separators.
static CARD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9](?:[ \x{00A0}\x{202F}\-]?[0-9]){12,18}").unwrap());

pub(crate) fn credit_card(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&CARD_RE, text, out, |m| {
        let start = m.start();
        if !start_bounded(text, start, &['-', '.', '/', ',']) || prev_char(text, start) == Some('+') {
            return None;
        }
        // Major networks start with 2 (Mastercard 2-series), 3 (Amex, Diners, JCB), 4, 5, 6.
        if !matches!(m.as_str().as_bytes()[0], b'2'..=b'6') {
            return None;
        }
        // Digit groups as (end byte relative to the match, digit count); one separator kind only.
        let mut groups: Vec<(usize, usize)> = Vec::new();
        let mut sep: Option<char> = None;
        let (mut len, mut end) = (0, 0);
        for (i, c) in m.as_str().char_indices() {
            if c.is_ascii_digit() {
                len += 1;
                end = i + 1;
                continue;
            }
            let kind = if is_space_like(c) { ' ' } else { c };
            if sep.is_some_and(|s| s != kind) {
                break;
            }
            sep = Some(kind);
            groups.push((end, len));
            len = 0;
        }
        if len > 0 {
            groups.push((end, len));
        }

        // The longest prefix of whole groups that is a plausible, Luhn-valid card number.
        let all = digits_of(m.as_str());
        let mut n = 0;
        let prefixes: Vec<(usize, usize, usize)> = groups
            .iter()
            .enumerate()
            .map(|(k, &(e, l))| {
                n += l;
                (k, e, n)
            })
            .collect();
        prefixes.into_iter().rev().find_map(|(k, e, n)| {
            let end = start + e;
            let grouped_ok = k == 0 || groups[..=k].iter().all(|&(_, l)| (3..=6).contains(&l));
            let ok = (13..=19).contains(&n)
                && grouped_ok
                && !next_char(text, end).is_some_and(char::is_alphanumeric)
                && luhn(&all[..n])
                && !ctx.in_iban(start, end);
            ok.then(|| Found::new(start, end, 1.0))
        })
    });
}
