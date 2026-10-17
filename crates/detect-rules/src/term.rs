//! User-defined Term Rules: literal terms mapped to an Entity Type.
//!
//! Matching is case-insensitive (Unicode simple case folding per character), whole-word (no
//! alphanumeric character may touch the match on either side) and whitespace-tolerant (a run of
//! whitespace in the term matches any run of whitespace in the text, so a term still matches
//! across a line break). Overlapping matches resolve to the longest one; on a tie the term that
//! comes first in the list wins.

use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorError, DetectorKind, EntityType, Span};

/// One literal term and the Entity Type it is redacted as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermRule {
    pub term: String,
    pub entity_type: EntityType,
}

/// Detects user-defined terms. Produces [`DetectorKind::Rule`] candidates at confidence 1.0
/// regardless of the active Entity Types: a term rule is an explicit user decision.
#[derive(Clone, Debug, Default)]
pub struct TermDetector {
    /// `(normalised term chars, type)`; empty terms are dropped.
    terms: Vec<(Vec<Tok>, EntityType)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Char(char),
    Space,
}

impl TermDetector {
    pub fn new(rules: Vec<(String, EntityType)>) -> Self {
        let terms = rules
            .into_iter()
            .filter_map(|(term, t)| {
                let toks = tokenize(&term);
                (!toks.is_empty()).then_some((toks, t))
            })
            .collect();
        TermDetector { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Every match of every term, overlaps resolved, in document order.
    pub fn find(&self, text: &str) -> Vec<Candidate> {
        let mut found: Vec<(Span, usize)> = Vec::new();
        for (ti, (toks, _)) in self.terms.iter().enumerate() {
            let mut prev: Option<char> = None;
            for (i, c) in text.char_indices() {
                let at_word_start = !prev.is_some_and(char::is_alphanumeric);
                prev = Some(c);
                if !at_word_start {
                    continue;
                }
                if let Some(end) = match_at(text, i, toks)
                    && !text[end..].chars().next().is_some_and(char::is_alphanumeric)
                {
                    found.push((Span::new(i, end), ti));
                }
            }
        }
        // Longest first at each start; ties keep list order.
        found.sort_by(|a, b| a.0.start.cmp(&b.0.start).then(b.0.len().cmp(&a.0.len())).then(a.1.cmp(&b.1)));
        let mut kept: Vec<(Span, usize)> = Vec::new();
        for (span, ti) in found {
            if let Some(last) = kept.last_mut()
                && last.0.overlaps(&span)
            {
                if span.len() > last.0.len() {
                    *last = (span, ti);
                }
                continue;
            }
            kept.push((span, ti));
        }
        kept.into_iter()
            .map(|(span, ti)| Candidate::new(span, self.terms[ti].1.clone(), 1.0, DetectorKind::Rule))
            .collect()
    }
}

fn tokenize(term: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    for c in term.trim().chars() {
        if c.is_whitespace() {
            if out.last() != Some(&Tok::Space) {
                out.push(Tok::Space);
            }
        } else {
            out.push(Tok::Char(c));
        }
    }
    out
}

fn same_char(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// End byte offset of `toks` matched at byte `start` of `text`, if it matches.
fn match_at(text: &str, start: usize, toks: &[Tok]) -> Option<usize> {
    let mut it = text[start..].char_indices().peekable();
    let mut end = start;
    for tok in toks {
        match tok {
            Tok::Char(want) => {
                let (i, c) = it.next()?;
                if !same_char(*want, c) {
                    return None;
                }
                end = start + i + c.len_utf8();
            }
            Tok::Space => {
                let mut n = 0;
                while let Some(&(i, c)) = it.peek() {
                    if !c.is_whitespace() {
                        break;
                    }
                    end = start + i + c.len_utf8();
                    it.next();
                    n += 1;
                }
                if n == 0 {
                    return None;
                }
            }
        }
    }
    Some(end)
}

impl Detector for TermDetector {
    fn kind(&self) -> DetectorKind {
        DetectorKind::Rule
    }

    fn name(&self) -> String {
        "term rules".to_string()
    }

    fn detect(&self, text: &str, _req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        Ok(self.find(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(rules: &[(&str, &str)]) -> TermDetector {
        TermDetector::new(rules.iter().map(|(a, b)| (a.to_string(), EntityType::new(b))).collect())
    }

    fn texts<'t>(d: &TermDetector, text: &'t str) -> Vec<(&'t str, String)> {
        d.find(text).iter().map(|c| (c.text(text), c.entity_type.as_str().to_string())).collect()
    }

    #[test]
    fn case_insensitive_whole_word() {
        let d = det(&[("Falke", "project code name")]);
        let t = "Projekt FALKE startet; falke ist geheim, Falkenberg nicht.";
        assert_eq!(texts(&d, t), vec![("FALKE", "project code name".into()), ("falke", "project code name".into())]);
        let c = &d.find(t)[0];
        assert_eq!(c.detector, DetectorKind::Rule);
        assert_eq!(c.confidence, 1.0);
    }

    #[test]
    fn umlauts_and_unicode_case() {
        let d = det(&[("Jürgen Müller", "person"), ("straße", "location")]);
        let t = "Herr JÜRGEN MÜLLER wohnt in der Straße. Müllerstraße zählt nicht; ÄJürgen Müller auch nicht.";
        assert_eq!(texts(&d, t), vec![("JÜRGEN MÜLLER", "person".into()), ("Straße", "location".into())]);
        for c in d.find(t) {
            assert!(c.span.is_valid_in(t));
        }
    }

    #[test]
    fn whitespace_in_term_matches_line_breaks() {
        let d = det(&[("Max  Mustermann", "person")]);
        assert_eq!(texts(&d, "Max\nMustermann."), vec![("Max\nMustermann", "person".into())]);
    }

    #[test]
    fn overlapping_terms_longest_wins() {
        let d = det(&[("Bank", "organization"), ("Deutsche Bank", "organization"), ("Bank AG", "customer number")]);
        let t = "Die Deutsche Bank AG und die Bank.";
        // "Deutsche Bank" (13) and "Bank AG" (7) overlap: the longer one wins.
        assert_eq!(texts(&d, t), vec![("Deutsche Bank", "organization".into()), ("Bank", "organization".into())]);
        // Equal length tie: first rule in the list wins.
        let d = det(&[("abc", "a"), ("ABC", "b")]);
        assert_eq!(texts(&d, "abc"), vec![("abc", "a".into())]);
    }

    #[test]
    fn empty_terms_are_ignored() {
        let d = det(&[("  ", "x"), ("", "y")]);
        assert!(d.is_empty());
        assert!(d.find("anything").is_empty());
    }
}
