//! PocketAnon Rule Detector: patterns and checksums for structured Entity Types.
//!
//! [`RuleDetector`] implements [`pocketanon_core::Detector`] with [`DetectorKind::Rule`]. It
//! proposes [`Candidate`]s for email, url, phone number, iban, credit card number, tax number,
//! national id number, passport number and date of birth. Candidates are only produced for
//! Entity Types in [`DetectRequest::types`]; language-specific patterns (German tax numbers, US
//! phone formats, cue words) only run when their Language is in [`DetectRequest::languages`].
//!
//! Spans are byte offsets into the chunk passed to [`Detector::detect`], on `char` boundaries,
//! without surrounding whitespace or trailing punctuation, because Merging lets rule candidates
//! decide the span boundaries.

use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorError, DetectorKind, EntityType};

mod patterns;
mod scan;
mod term;
pub mod validators;

pub use term::{TermDetector, TermRule};

use patterns::{Ctx, RULES, Rule};

/// The Rule Detector with all built-in patterns.
pub struct RuleDetector {
    rules: Vec<(&'static Rule, EntityType)>,
}

impl RuleDetector {
    /// A detector with every built-in pattern.
    pub fn new() -> Self {
        RuleDetector { rules: RULES.iter().map(|r| (r, EntityType::new(r.entity_type))).collect() }
    }

    /// The Entity Types this detector can produce, in catalog order, without duplicates.
    pub fn supported_types(&self) -> Vec<EntityType> {
        let mut out: Vec<EntityType> = Vec::new();
        for (_, t) in &self.rules {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
        out
    }

    /// Identifiers of the patterns that would run for `req`, e.g. `"iban"`, `"tax.de.steuer_id"`.
    pub fn active_rules(&self, req: &DetectRequest<'_>) -> Vec<&'static str> {
        self.active(req).map(|(r, _)| r.id).collect()
    }

    fn active<'s>(&'s self, req: &'s DetectRequest<'_>) -> impl Iterator<Item = &'s (&'static Rule, EntityType)> {
        self.rules.iter().filter(move |(r, t)| {
            req.types.contains(t)
                && r.language.is_none_or(|lang| {
                    req.languages.iter().any(|l| l.code().split(['-', '_']).next() == Some(lang))
                })
        })
    }

    /// Run every active rule over `text`. Infallible; [`Detector::detect`] wraps it.
    pub fn find(&self, text: &str, req: &DetectRequest<'_>) -> Vec<Candidate> {
        let ctx = Ctx { text, iban_like: patterns::financial::iban_like_spans(text) };
        let mut found = Vec::new();
        let mut candidates: Vec<Candidate> = Vec::new();
        for (rule, ty) in self.active(req) {
            found.clear();
            (rule.find)(&ctx, &mut found);
            for f in &found {
                debug_assert!(f.span.is_valid_in(text), "rule {} produced invalid span {:?}", rule.id, f.span);
                if !f.span.is_empty() && f.span.is_valid_in(text) {
                    candidates.push(Candidate::new(f.span, ty.clone(), f.confidence, DetectorKind::Rule));
                }
            }
        }
        dedup_nested(candidates)
    }
}

impl Default for RuleDetector {
    fn default() -> Self {
        RuleDetector::new()
    }
}

/// Drop candidates that lie inside another candidate of the same Entity Type (several patterns
/// for one type may find the same number). For identical spans the higher confidence is kept.
fn dedup_nested(mut cs: Vec<Candidate>) -> Vec<Candidate> {
    cs.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(b.span.end.cmp(&a.span.end)));
    let mut kept: Vec<Candidate> = Vec::with_capacity(cs.len());
    for c in cs {
        if let Some(k) = kept.iter_mut().find(|k| k.entity_type == c.entity_type && c.span.is_within(&k.span)) {
            if k.span == c.span && c.confidence > k.confidence {
                k.confidence = c.confidence;
            }
            continue;
        }
        kept.push(c);
    }
    kept
}

impl Detector for RuleDetector {
    fn kind(&self) -> DetectorKind {
        DetectorKind::Rule
    }

    fn name(&self) -> String {
        "rules".to_string()
    }

    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        Ok(self.find(text, req))
    }
}
