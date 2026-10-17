#![allow(dead_code)]

use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorKind, EntityType, Language, TypeCatalog};
use pocketanon_detect_rules::RuleDetector;

pub fn all_types() -> Vec<EntityType> {
    TypeCatalog::builtin().all_types()
}

pub fn langs(codes: &[&str]) -> Vec<Language> {
    codes.iter().map(Language::new).collect()
}

/// Detect with every built-in type and the given languages; checks span invariants.
pub fn detect_with(text: &str, types: &[EntityType], codes: &[&str]) -> Vec<Candidate> {
    let languages = langs(codes);
    let req = DetectRequest { types, languages: &languages };
    let out = RuleDetector::new().detect(text, &req).expect("rule detector is infallible");
    for c in &out {
        assert!(c.span.is_valid_in(text), "invalid span {:?} in {text:?}", c.span);
        assert!(!c.span.is_empty());
        assert_eq!(c.detector, DetectorKind::Rule);
        let t = c.text(text);
        assert_eq!(t, t.trim(), "span has surrounding whitespace: {t:?}");
        assert!(!t.ends_with(['.', ',', ';', ':', '!', '?']), "trailing punctuation in {t:?}");
    }
    out
}

pub fn detect(text: &str) -> Vec<Candidate> {
    detect_with(text, &all_types(), &["de", "en"])
}

/// (text, type) pairs, in order.
pub fn pairs(text: &str, cs: &[Candidate]) -> Vec<(String, String)> {
    cs.iter().map(|c| (c.text(text).to_string(), c.entity_type.as_str().to_string())).collect()
}

/// Matched texts of one type.
pub fn of_type(text: &str, ty: &str) -> Vec<String> {
    detect(text).iter().filter(|c| c.entity_type == EntityType::new(ty)).map(|c| c.text(text).to_string()).collect()
}

pub fn confidence_of(text: &str, needle: &str, ty: &str) -> f32 {
    detect(text)
        .into_iter()
        .find(|c| c.text(text) == needle && c.entity_type == EntityType::new(ty))
        .unwrap_or_else(|| panic!("{needle:?} as {ty} not found in {text:?}"))
        .confidence
}

/// Assert that `ty` finds exactly `expected` in `text`.
#[track_caller]
pub fn assert_finds(text: &str, ty: &str, expected: &[&str]) {
    assert_eq!(of_type(text, ty), expected, "type {ty} in {text:?}");
}
