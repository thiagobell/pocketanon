//! Value types shared by every other module.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::detector::DetectorKind;

/// A category assigned to a detected span, e.g. `person`, `email`, `iban`.
///
/// Stored lowercase and trimmed so that `"Person"`, `" person "` and `"person"` compare equal.
/// GLiNER labels are free text, so this is deliberately not an enum; see [`crate::catalog`]
/// for the built-in set.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityType(String);

impl EntityType {
    pub fn new(label: impl AsRef<str>) -> Self {
        let norm = label
            .as_ref()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        EntityType(norm)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The token used inside a redaction placeholder: upper-case, spaces to underscores.
    /// `"date of birth"` becomes `DATE_OF_BIRTH`.
    pub fn placeholder_token(&self) -> String {
        self.0
            .chars()
            .map(|c| if c.is_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_")
    }
}

impl fmt::Debug for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityType({:?})", self.0)
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EntityType {
    fn from(s: &str) -> Self {
        EntityType::new(s)
    }
}

impl From<String> for EntityType {
    fn from(s: String) -> Self {
        EntityType::new(s)
    }
}

/// A natural language the user expects in the input, as a lowercase ISO 639-1 code (`de`, `en`).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Language(String);

impl Language {
    pub fn new(code: impl AsRef<str>) -> Self {
        Language(code.as_ref().trim().to_lowercase())
    }
    pub fn code(&self) -> &str {
        &self.0
    }
    pub fn german() -> Self {
        Language::new("de")
    }
    pub fn english() -> Self {
        Language::new("en")
    }
}

impl fmt::Debug for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Language({:?})", self.0)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A half-open byte range `start..end` into the analysed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start {start} after end {end}");
        Span { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }

    /// `true` when the two spans share at least one byte.
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// `true` when `self` lies entirely inside `other` (equality counts).
    pub fn is_within(&self, other: &Span) -> bool {
        self.start >= other.start && self.end <= other.end
    }

    /// The same span expressed relative to a base offset (used when mapping chunk-local
    /// offsets back onto the whole input).
    pub fn shifted(&self, by: usize) -> Span {
        Span::new(self.start + by, self.end + by)
    }

    /// Validates that the span is in bounds of `text` and on `char` boundaries.
    pub fn is_valid_in(&self, text: &str) -> bool {
        self.start <= self.end
            && self.end <= text.len()
            && text.is_char_boundary(self.start)
            && text.is_char_boundary(self.end)
    }
}

/// A span proposed by one detector. Becomes an [`Entity`] only after merging and review.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub span: Span,
    pub entity_type: EntityType,
    /// 0.0 ..= 1.0
    pub confidence: f32,
    pub detector: DetectorKind,
}

impl Candidate {
    pub fn new(span: Span, entity_type: impl Into<EntityType>, confidence: f32, detector: DetectorKind) -> Self {
        Candidate { span, entity_type: entity_type.into(), confidence: confidence.clamp(0.0, 1.0), detector }
    }

    pub fn text<'t>(&self, text: &'t str) -> &'t str {
        &text[self.span.range()]
    }
}

/// An accepted span that will be replaced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub span: Span,
    pub entity_type: EntityType,
}

impl Entity {
    pub fn new(span: Span, entity_type: impl Into<EntityType>) -> Self {
        Entity { span, entity_type: entity_type.into() }
    }

    pub fn text<'t>(&self, text: &'t str) -> &'t str {
        &text[self.span.range()]
    }
}

impl From<&Candidate> for Entity {
    fn from(c: &Candidate) -> Self {
        Entity { span: c.span, entity_type: c.entity_type.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_normalises() {
        assert_eq!(EntityType::new(" Date  Of Birth "), EntityType::new("date of birth"));
        assert_eq!(EntityType::new("Date Of Birth").placeholder_token(), "DATE_OF_BIRTH");
        assert_eq!(EntityType::new("e-mail").placeholder_token(), "E_MAIL");
        assert_eq!(EntityType::new("iban").placeholder_token(), "IBAN");
    }

    #[test]
    fn span_relations() {
        let a = Span::new(0, 5);
        let b = Span::new(3, 8);
        let c = Span::new(5, 9);
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
        assert!(Span::new(1, 2).is_within(&a));
        assert!(a.is_within(&a));
        assert!(!b.is_within(&a));
        assert_eq!(Span::new(1, 2).shifted(10), Span::new(11, 12));
    }

    #[test]
    fn span_validity_respects_char_boundaries() {
        let s = "aé b";
        assert!(Span::new(0, 1).is_valid_in(s));
        assert!(!Span::new(1, 2).is_valid_in(s)); // inside 'é'
        assert!(Span::new(1, 3).is_valid_in(s));
        assert!(!Span::new(0, 99).is_valid_in(s));
    }
}
