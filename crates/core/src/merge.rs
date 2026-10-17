//! Resolving overlapping candidates from several detectors into one list.
//!
//! Rules (see `CONTEXT.md`, *Merging*):
//!
//! 1. A **rule** candidate wins the span boundaries over a **model** candidate it overlaps.
//! 2. Between two candidates of the same detector kind, the one with higher confidence wins;
//!    on a tie, the longer span wins (GLiNER tends to emit a nested shorter variant).
//! 3. A **manual** candidate always wins and is never altered.
//! 4. The entity type of the surviving candidate is the type of the overlapping candidate with
//!    the highest confidence, so a rule span tagged `iban` at 1.0 keeps `iban` even when the
//!    model called the same bytes `customer number` at 0.6.
//!
//! The result is sorted by start offset and contains no overlapping spans.

use crate::detector::DetectorKind;
use crate::types::Candidate;

fn priority(k: DetectorKind) -> u8 {
    match k {
        DetectorKind::Manual => 2,
        DetectorKind::Rule => 1,
        DetectorKind::Model => 0,
    }
}

/// Ordering used to decide which of two overlapping candidates keeps its span.
fn beats(a: &Candidate, b: &Candidate) -> bool {
    let (pa, pb) = (priority(a.detector), priority(b.detector));
    if pa != pb {
        return pa > pb;
    }
    if a.confidence != b.confidence {
        return a.confidence > b.confidence;
    }
    if a.span.len() != b.span.len() {
        return a.span.len() > b.span.len();
    }
    a.span.start <= b.span.start
}

/// Merge candidates from any number of detectors into a non-overlapping list.
pub fn merge(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    // Strongest first, so each candidate only has to be checked against already-accepted ones.
    candidates.sort_by(|a, b| {
        if beats(a, b) && !beats(b, a) {
            std::cmp::Ordering::Less
        } else if beats(b, a) && !beats(a, b) {
            std::cmp::Ordering::Greater
        } else {
            a.span.cmp(&b.span)
        }
    });

    let mut accepted: Vec<Candidate> = Vec::new();
    for c in candidates {
        if c.span.is_empty() {
            continue;
        }
        if let Some(winner) = accepted.iter_mut().find(|a| a.span.overlaps(&c.span)) {
            // The accepted one keeps its span; the type follows the higher confidence, unless the
            // winner is manual (the user's word is final).
            if winner.detector != DetectorKind::Manual && c.confidence > winner.confidence {
                winner.entity_type = c.entity_type.clone();
                winner.confidence = c.confidence;
            }
            continue;
        }
        accepted.push(c);
    }
    accepted.sort_by_key(|c| c.span);
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Span;

    fn c(s: usize, e: usize, t: &str, conf: f32, k: DetectorKind) -> Candidate {
        Candidate::new(Span::new(s, e), t, conf, k)
    }

    #[test]
    fn disjoint_candidates_all_survive_sorted() {
        let out = merge(vec![
            c(10, 15, "person", 0.9, DetectorKind::Model),
            c(0, 5, "email", 0.8, DetectorKind::Rule),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].span, Span::new(0, 5));
        assert_eq!(out[1].span, Span::new(10, 15));
    }

    #[test]
    fn rule_wins_boundaries_higher_confidence_wins_type() {
        // model says "DE89 3704" (partial) is a customer number at 0.95; rule says the full IBAN
        // at 0.9. Rule keeps its span; type goes to the higher confidence (customer number).
        let out = merge(vec![
            c(0, 9, "customer number", 0.95, DetectorKind::Model),
            c(0, 22, "iban", 0.9, DetectorKind::Rule),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].span, Span::new(0, 22));
        assert_eq!(out[0].entity_type.as_str(), "customer number");
        assert_eq!(out[0].detector, DetectorKind::Rule);
    }

    #[test]
    fn rule_with_checksum_confidence_keeps_type() {
        let out = merge(vec![
            c(0, 9, "customer number", 0.6, DetectorKind::Model),
            c(0, 22, "iban", 1.0, DetectorKind::Rule),
        ]);
        assert_eq!(out[0].entity_type.as_str(), "iban");
        assert_eq!(out[0].confidence, 1.0);
    }

    #[test]
    fn same_kind_higher_confidence_wins_then_longer() {
        let out = merge(vec![
            c(0, 4, "person", 0.7, DetectorKind::Model),
            c(0, 10, "person", 0.7, DetectorKind::Model),
            c(2, 6, "organization", 0.9, DetectorKind::Model),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].span, Span::new(2, 6));
        assert_eq!(out[0].entity_type.as_str(), "organization");

        let out = merge(vec![
            c(0, 4, "person", 0.7, DetectorKind::Model),
            c(0, 10, "person", 0.7, DetectorKind::Model),
        ]);
        assert_eq!(out[0].span, Span::new(0, 10));
    }

    #[test]
    fn manual_is_untouchable() {
        let out = merge(vec![
            c(0, 5, "project code name", 1.0, DetectorKind::Manual),
            c(0, 5, "person", 1.0, DetectorKind::Model),
            c(3, 12, "iban", 1.0, DetectorKind::Rule),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].span, Span::new(0, 5));
        assert_eq!(out[0].entity_type.as_str(), "project code name");
    }

    #[test]
    fn chain_overlaps_resolve_without_overlap_in_output() {
        let out = merge(vec![
            c(0, 6, "a", 0.5, DetectorKind::Model),
            c(4, 10, "b", 0.6, DetectorKind::Model),
            c(8, 14, "c", 0.5, DetectorKind::Model),
        ]);
        for w in out.windows(2) {
            assert!(!w[0].span.overlaps(&w[1].span));
        }
        assert_eq!(out.len(), 1); // b beats both neighbours
        assert_eq!(out[0].entity_type.as_str(), "b");
    }

    #[test]
    fn empty_spans_are_dropped() {
        let out = merge(vec![c(3, 3, "person", 1.0, DetectorKind::Rule)]);
        assert!(out.is_empty());
    }
}
