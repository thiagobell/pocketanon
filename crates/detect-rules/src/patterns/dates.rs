//! Identity group: date of birth. A date only counts after a cue word; plain dates never fire.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::Ctx;
use crate::scan::{Found, after_cue, alnum_bounded};

static CUE_DE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:geb\.(?:[\s\-]?datum)?|geboren(?:\s+am)?|geburtsdatum|geburtstag)").unwrap()
});
static CUE_EN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:born(?:\s+on)?|date\s+of\s+birth|birth\s?date|birthday|d\.o\.b\.|dob\b)").unwrap()
});

const MONTHS_DE: &str = r"januar|jan\.?|jänner|februar|feb\.?|märz|maerz|mär\.?|april|apr\.?|mai|juni|jun\.?|juli|jul\.?|august|aug\.?|september|sept?\.?|oktober|okt\.?|november|nov\.?|dezember|dez\.?";
const MONTHS_EN: &str = r"january|jan\.?|february|feb\.?|march|mar\.?|april|apr\.?|may|june|jun\.?|july|jul\.?|august|aug\.?|september|sept?\.?|october|oct\.?|november|nov\.?|december|dec\.?";

/// All supported date shapes. Named groups `d`, `m`/`mn`, `y` are read by [`plausible`].
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let months = format!("{MONTHS_DE}|{MONTHS_EN}");
    Regex::new(&format!(
        r"(?xi)
          (?P<d1>[0-9]{{1,2}})\.\s?(?P<m1>[0-9]{{1,2}})\.\s?(?P<y1>[0-9]{{4}}|[0-9]{{2}})   # 12.03.1980
        | (?P<d2>[0-9]{{1,2}})/(?P<m2>[0-9]{{1,2}})/(?P<y2>[0-9]{{4}})                       # 12/03/1980
        | (?P<y3>[0-9]{{4}})-(?P<m3>[0-9]{{2}})-(?P<d3>[0-9]{{2}})                           # 1980-03-12
        | (?P<d4>[0-9]{{1,2}})(?:\.|st|nd|rd|th)?\s+(?:of\s+)?(?P<mn4>{months})\s+(?P<y4>[0-9]{{4}})  # 12. März 1980
        | (?P<mn5>{months})\s+(?P<d5>[0-9]{{1,2}})(?:st|nd|rd|th)?,?\s+(?P<y5>[0-9]{{4}})    # March 12, 1980
        "
    ))
    .unwrap()
});

fn num(c: &Captures<'_>, names: &[&str]) -> Option<u32> {
    names.iter().find_map(|n| c.name(n)).and_then(|m| m.as_str().parse().ok())
}

fn plausible(c: &Captures<'_>) -> bool {
    let Some(day) = num(c, &["d1", "d2", "d3", "d4", "d5"]) else { return false };
    let Some(year) = num(c, &["y1", "y2", "y3", "y4", "y5"]) else { return false };
    let numeric_month = num(c, &["m1", "m2", "m3"]);
    let named_month = c.name("mn4").or(c.name("mn5")).is_some();
    let month_ok = named_month || numeric_month.is_some_and(|m| (1..=12).contains(&m));
    let year_ok = year < 100 || (1850..=2100).contains(&year);
    (1..=31).contains(&day) && month_ok && year_ok
}

fn date_of_birth(cue: &Regex, ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    let text = ctx.text;
    after_cue(cue, &DATE_RE, text, 30, out, |m| {
        if !alnum_bounded(text, m.start(), m.end()) {
            return None;
        }
        // Re-run anchored on the match to read the groups.
        let c = DATE_RE.captures_at(text, m.start()).filter(|c| c.get(0) == Some(m))?;
        plausible(&c).then(|| Found::new(m.start(), m.end(), 0.9))
    });
}

pub(crate) fn date_of_birth_de(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    date_of_birth(&CUE_DE_RE, ctx, out);
}

pub(crate) fn date_of_birth_en(ctx: &Ctx<'_>, out: &mut Vec<Found>) {
    date_of_birth(&CUE_EN_RE, ctx, out);
}
