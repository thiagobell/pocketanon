//! Identifiers group: tax number (German Steuer-ID and Steuernummer, US SSN).

use std::sync::LazyLock;

use regex::Regex;

use super::Ctx;
use crate::scan::{Found, after_cue, digits_of, number_bounded, scan};
use crate::validators;

const NUM_SEPS: &[char] = &['-', '.', '/', ','];

/// Steuer-ID: 11 digits, plain or as `12 345 678 901`.
static STEUER_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[1-9][0-9](?:[ \x{00A0}\x{202F}][0-9]{3}[ \x{00A0}\x{202F}][0-9]{3}[ \x{00A0}\x{202F}][0-9]{3}|[0-9]{9})")
        .unwrap()
});

pub(crate) fn steuer_id(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    scan(&STEUER_ID_RE, text, out, |m| {
        let ok = number_bounded(text, m.start(), m.end(), NUM_SEPS) && validators::steuer_id(&digits_of(m.as_str()));
        ok.then(|| Found::new(m.start(), m.end(), 1.0))
    });
}

static STEUERNUMMER_CUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:steuer[\s\-]?(?:nummer|nr\.?)|st\.?[\s\-]*nr\.?|stnr\.?)").unwrap()
});

/// Steuernummer in the per-state forms (`12/345/67890`, `123/456/78901`, `12345/67890`,
/// `013 815 08153`) and the unified 13-digit federal form.
static STEUERNUMMER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        [0-9]{2,5}/[0-9]{3,4}/[0-9]{4,5}
        | [0-9]{5}/[0-9]{5}
        | [0-9]{3}[\x20\x{00A0}][0-9]{3,4}[\x20\x{00A0}][0-9]{4,5}
        | [0-9]{13}
        | [0-9]{10,11}
        ",
    )
    .unwrap()
});

pub(crate) fn steuernummer(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    after_cue(&STEUERNUMMER_CUE_RE, &STEUERNUMMER_RE, text, 20, out, |m| {
        number_bounded(text, m.start(), m.end(), NUM_SEPS).then(|| Found::new(m.start(), m.end(), 0.85))
    });
}

/// US Social Security Number, `123-45-6789`.
static SSN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([0-9]{3})-([0-9]{2})-([0-9]{4})").unwrap());

pub(crate) fn ssn(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    let mut pos = 0;
    while let Some(c) = SSN_RE.captures_at(text, pos) {
        let m = c.get(0).unwrap();
        pos = m.start() + 1;
        let (area, group, serial) = (&c[1], &c[2], &c[3]);
        let valid = area != "000"
            && area != "666"
            && !area.starts_with('9')
            && group != "00"
            && serial != "0000"
            && m.as_str() != "078-05-1120"; // the famous wallet-insert number
        if valid && number_bounded(text, m.start(), m.end(), NUM_SEPS) {
            out.push(Found::new(m.start(), m.end(), 0.9));
            pos = m.end();
        }
    }
}
