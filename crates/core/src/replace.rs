//! Replacement strategies and their application.
//!
//! [`Indexer`] is the stateful heart of **Indexed Redaction**: it hands out `[PERSON_1]`,
//! `[PERSON_2]` and guarantees that the same entity text (case- and whitespace-insensitive)
//! of the same type gets the same index for as long as the indexer lives. One indexer per
//! input: a document's paragraphs, headers and footnotes all share it, so chunk and paragraph
//! boundaries never reset numbering. The mapping it holds is never exported (ADR-0003).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::summary::Summary;
use crate::types::{Entity, EntityType, Span};

/// How an accepted entity is turned into replacement text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    /// `[PERSON]`
    Redaction,
    /// `[PERSON_1]`, stable per distinct entity text within one input.
    #[default]
    IndexedRedaction,
}

/// Issues placeholders and remembers which original text got which index.
#[derive(Debug, Default)]
pub struct Indexer {
    strategy: Strategy,
    next: HashMap<EntityType, usize>,
    seen: HashMap<(EntityType, String), usize>,
}

impl Indexer {
    pub fn new(strategy: Strategy) -> Self {
        Indexer { strategy, ..Default::default() }
    }

    pub fn strategy(&self) -> Strategy {
        self.strategy
    }

    /// The placeholder for `original` of type `t`. Allocates a new index on first sight.
    pub fn placeholder(&mut self, t: &EntityType, original: &str) -> String {
        let token = t.placeholder_token();
        match self.strategy {
            Strategy::Redaction => format!("[{token}]"),
            Strategy::IndexedRedaction => {
                let key = (t.clone(), normalise(original));
                let n = match self.seen.get(&key) {
                    Some(n) => *n,
                    None => {
                        let counter = self.next.entry(t.clone()).or_insert(0);
                        *counter += 1;
                        let n = *counter;
                        self.seen.insert(key, n);
                        n
                    }
                };
                format!("[{token}_{n}]")
            }
        }
    }

    /// Number of distinct originals seen so far (indexed strategy only).
    pub fn distinct(&self) -> usize {
        self.seen.len()
    }
}

/// Case-, whitespace- and trailing-punctuation-insensitive key for "same entity".
fn normalise(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ',', ';', ':'])
        .to_lowercase()
}

/// One concrete edit: replace the bytes in `span` of the original text with `with`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub span: Span,
    pub entity_type: EntityType,
    pub with: String,
}

/// Compute the edits for `entities` in `text` without applying them.
///
/// Entities are sorted by start; overlapping entities are an input error that is tolerated by
/// keeping the earlier one and dropping any that start before the previous one ends. Entities
/// with spans that are out of bounds or off a char boundary are dropped too. The returned
/// replacements are in document order, which lets callers (such as the document adapter) apply
/// them back to front on a mutable structure.
pub fn plan_replacements(text: &str, entities: &[Entity], indexer: &mut Indexer) -> (Vec<Replacement>, Summary) {
    let mut sorted: Vec<&Entity> = entities.iter().filter(|e| e.span.is_valid_in(text) && !e.span.is_empty()).collect();
    sorted.sort_by_key(|e| (e.span.start, std::cmp::Reverse(e.span.end)));

    let mut out = Vec::with_capacity(sorted.len());
    let mut summary = Summary::default();
    let mut cursor = 0usize;
    for e in sorted {
        if e.span.start < cursor {
            continue; // overlaps the previous replacement
        }
        let with = indexer.placeholder(&e.entity_type, e.text(text));
        summary.record(&e.entity_type);
        out.push(Replacement { span: e.span, entity_type: e.entity_type.clone(), with });
        cursor = e.span.end;
    }
    (out, summary)
}

/// Apply replacements to a string, producing the anonymized text.
pub fn apply_replacements(text: &str, replacements: &[Replacement]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for r in replacements {
        out.push_str(&text[cursor..r.span.start]);
        out.push_str(&r.with);
        cursor = r.span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Convenience: plan and apply in one go.
pub fn apply(text: &str, entities: &[Entity], indexer: &mut Indexer) -> (String, Summary) {
    let (reps, summary) = plan_replacements(text, entities, indexer);
    (apply_replacements(text, &reps), summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::{prop, prop_assert_eq, proptest};

    fn e(s: usize, en: usize, t: &str) -> Entity {
        Entity::new(Span::new(s, en), t)
    }

    #[test]
    fn plain_redaction_uses_type_only() {
        let mut ix = Indexer::new(Strategy::Redaction);
        let t = "Max Mustermann und Erika Musterfrau";
        let (out, s) = apply(t, &[e(0, 14, "person"), e(19, 35, "person")], &mut ix);
        assert_eq!(out, "[PERSON] und [PERSON]");
        assert_eq!(s.total(), 2);
        assert_eq!(s.counts[&EntityType::new("person")], 2);
    }

    #[test]
    fn indexed_redaction_numbers_distinct_and_repeats_same() {
        let mut ix = Indexer::new(Strategy::IndexedRedaction);
        let t = "Max Mustermann traf Erika. Später rief max mustermann an.";
        let m = t.find("max mustermann").unwrap();
        let ents = [e(0, 14, "person"), e(20, 25, "person"), e(m, m + 14, "person")];
        let (out, _) = apply(t, &ents, &mut ix);
        assert_eq!(out, "[PERSON_1] traf [PERSON_2]. Später rief [PERSON_1] an.");
        assert_eq!(ix.distinct(), 2);
    }

    #[test]
    fn indices_are_per_type_and_survive_across_calls() {
        let mut ix = Indexer::new(Strategy::IndexedRedaction);
        let (a, _) = apply("Anna, anna@example.com", &[e(0, 4, "person"), e(6, 22, "email")], &mut ix);
        assert_eq!(a, "[PERSON_1], [EMAIL_1]");
        // second paragraph of the same document: Anna keeps 1, a new person gets 2
        let (b, _) = apply("Bob und Anna", &[e(0, 3, "person"), e(8, 12, "person")], &mut ix);
        assert_eq!(b, "[PERSON_2] und [PERSON_1]");
    }

    #[test]
    fn multiword_type_token() {
        let mut ix = Indexer::new(Strategy::IndexedRedaction);
        let (out, _) = apply("born 01.02.1980", &[e(5, 15, "date of birth")], &mut ix);
        assert_eq!(out, "born [DATE_OF_BIRTH_1]");
    }

    #[test]
    fn overlapping_and_invalid_entities_are_tolerated() {
        let mut ix = Indexer::new(Strategy::Redaction);
        let t = "aé bcd";
        let ents = [
            e(0, 3, "x"),   // "aé"
            e(2, 4, "y"),   // overlaps previous AND starts inside 'é' → dropped
            e(4, 7, "z"),   // "bcd"
            e(5, 99, "w"),  // out of bounds → dropped
        ];
        let (out, s) = apply(t, &ents, &mut ix);
        assert_eq!(out, "[X] [Z]");
        assert_eq!(s.total(), 2);
    }

    #[test]
    fn adjacent_entities_do_not_swallow_each_other() {
        let mut ix = Indexer::new(Strategy::Redaction);
        let (out, _) = apply("ABCD", &[e(0, 2, "a"), e(2, 4, "b")], &mut ix);
        assert_eq!(out, "[A][B]");
    }

    #[test]
    fn replacements_are_in_document_order() {
        let mut ix = Indexer::new(Strategy::Redaction);
        let (reps, _) = plan_replacements("0123456789", &[e(6, 8, "b"), e(1, 3, "a")], &mut ix);
        assert_eq!(reps.iter().map(|r| r.span.start).collect::<Vec<_>>(), vec![1, 6]);
    }

    proptest! {
        /// Everything outside entity spans is preserved verbatim, in order.
        #[test]
        fn untouched_text_is_preserved(
            text in "[a-zA-Zäöü0-9 .,]{0,80}",
            cuts in prop::collection::vec((0usize..80, 1usize..12), 0..8),
        ) {
            let mut ents = Vec::new();
            let mut cursor = 0;
            for (gap, len) in cuts {
                let start = (cursor + gap).min(text.len());
                let end = (start + len).min(text.len());
                let (start, end) = (snap(&text, start), snap(&text, end));
                if start < end { ents.push(e(start, end, "t")); cursor = end; }
            }
            let mut ix = Indexer::new(Strategy::IndexedRedaction);
            let (reps, _) = plan_replacements(&text, &ents, &mut ix);
            let out = apply_replacements(&text, &reps);
            // Reconstruct expected by stitching gaps.
            let mut expected = String::new();
            let mut c = 0;
            for (i, r) in reps.iter().enumerate() {
                expected.push_str(&text[c..r.span.start]);
                expected.push_str(&format!("[T_{}]", i + 1).replace(&format!("[T_{}]", i+1), &r.with));
                c = r.span.end;
            }
            expected.push_str(&text[c..]);
            prop_assert_eq!(out, expected);
        }
    }

    fn snap(s: &str, mut i: usize) -> usize {
        while i < s.len() && !s.is_char_boundary(i) { i += 1; }
        i.min(s.len())
    }
}
