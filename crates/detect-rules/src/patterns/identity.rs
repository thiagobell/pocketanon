//! Identifiers group: national id number (German Personalausweis), passport number.

use std::sync::LazyLock;

use regex::Regex;

use super::Ctx;
use crate::scan::{Found, after_cue, alnum_bounded};
use crate::validators::icao_check_digit;

static AUSWEIS_CUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:personalausweis(?:nummer|[\s\-]?nr\.?)?|ausweis(?:nummer|[\s\-]?nr\.?)?|perso[\s\-]?nr\.?|PA[\s\-]?Nr\.?)",
    )
    .unwrap()
});

/// Document number of a German identity card: 9 characters from the ICAO alphabet the
/// Bundesdruckerei uses (digits and `CFGHJKLMNPRTVWXYZ`), optionally followed by the check digit.
static AUSWEIS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[CFGHJKLMNPRTVWXYZ0-9]{9}[0-9]?").unwrap());

/// German Personalausweis number, only after a cue word. Ten characters must carry a valid ICAO
/// check digit (0.9); nine characters are accepted at 0.8.
pub(crate) fn personalausweis(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    after_cue(&AUSWEIS_CUE_RE, &AUSWEIS_RE, text, 30, out, |m| {
        let s = m.as_str();
        if !alnum_bounded(text, m.start(), m.end()) || !s.bytes().any(|c| c.is_ascii_digit()) {
            return None;
        }
        match s.len() {
            10 => (icao_check_digit(&s[..9]) == s[9..].parse().ok()).then(|| Found::new(m.start(), m.end(), 0.9)),
            _ => Some(Found::new(m.start(), m.end(), 0.8)),
        }
    });
}

static PASSPORT_CUE_DE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:reisepass(?:nummer|[\s\-]?nr\.?)?|passnummer|pass[\s\-]?nr\.?)").unwrap()
});
static PASSPORT_CUE_EN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bpassport(?:\s*(?:no\.?|number|nr\.?|#))?").unwrap()
});
static PASSPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z0-9]{8,9}").unwrap());

fn passport(cue: &Regex, ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    after_cue(cue, &PASSPORT_RE, text, 20, out, |m| {
        let ok = alnum_bounded(text, m.start(), m.end()) && m.as_str().bytes().any(|c| c.is_ascii_digit());
        ok.then(|| Found::new(m.start(), m.end(), 0.8))
    });
}

pub(crate) fn passport_de(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    passport(&PASSPORT_CUE_DE_RE, ctx, out);
}

pub(crate) fn passport_en(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    passport(&PASSPORT_CUE_EN_RE, ctx, out);
}
