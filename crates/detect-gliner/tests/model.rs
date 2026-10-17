//! Runs the real Model. Needs `--features model-tests` and `POCKETANON_MODEL_DIR` pointing at a
//! model directory (e.g. the one `examples/download.rs` prints).
#![cfg(feature = "model-tests")]

use std::path::PathBuf;
use std::sync::OnceLock;

use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorKind, EntityType, Language, TypeCatalog};
use pocketanon_detect_gliner::{GlinerDetector, GlinerOptions};

fn detector() -> &'static GlinerDetector {
    static D: OnceLock<GlinerDetector> = OnceLock::new();
    D.get_or_init(|| {
        let dir = PathBuf::from(std::env::var("POCKETANON_MODEL_DIR").expect("set POCKETANON_MODEL_DIR"));
        GlinerDetector::load(&dir, GlinerOptions::default()).expect("load model")
    })
}

fn run(text: &str) -> Vec<Candidate> {
    let types = TypeCatalog::builtin().default_types();
    let langs = [Language::german(), Language::english()];
    let req = DetectRequest { types: &types, languages: &langs };
    let cands = detector().detect(text, &req).expect("detect");
    for c in &cands {
        eprintln!("{:>5}..{:<5} {:20} {:16} {:.3}", c.span.start, c.span.end, c.text(text), c.entity_type, c.confidence);
        assert!(c.span.is_valid_in(text));
        assert_eq!(c.detector, DetectorKind::Model);
    }
    cands
}

fn has(text: &str, cands: &[Candidate], ty: &str, s: &str) -> bool {
    cands.iter().any(|c| c.entity_type == EntityType::new(ty) && c.text(text) == s && c.confidence > 0.5)
}

#[test]
fn german_sentence() {
    let text = "Max Mustermann wohnt in Bonn und erreicht Sie unter max@example.com.";
    let cands = run(text);
    assert!(has(text, &cands, "person", "Max Mustermann"), "{cands:?}");
    assert!(has(text, &cands, "location", "Bonn"), "{cands:?}");
}

#[test]
fn english_sentence() {
    let text = "Erika Musterfrau works at Example GmbH in Berlin.";
    let cands = run(text);
    assert!(has(text, &cands, "person", "Erika Musterfrau"), "{cands:?}");
    assert!(has(text, &cands, "location", "Berlin"), "{cands:?}");
}

#[test]
fn multibyte_offsets() {
    let text = "Sehr geehrte Frau Jürgensen, Herr Müßig aus Köln lässt grüßen.";
    let cands = run(text);
    assert!(cands.iter().any(|c| c.entity_type == EntityType::new("person")), "{cands:?}");
}

#[test]
fn more_than_25_types_are_batched() {
    let mut types = TypeCatalog::builtin().all_types();
    for i in 0..15 {
        types.push(EntityType::new(format!("dummy type {i}")));
    }
    assert!(types.len() > 25);
    let text = "Max Mustermann wohnt in Bonn.";
    let req = DetectRequest { types: &types, languages: &[] };
    let cands = detector().detect(text, &req).unwrap();
    assert!(has(text, &cands, "person", "Max Mustermann"), "{cands:?}");
}

#[test]
fn empty_input() {
    let types = [EntityType::new("person")];
    let req = DetectRequest { types: &types, languages: &[] };
    assert!(detector().detect("   ", &req).unwrap().is_empty());
    assert!(detector().detect("Max", &DetectRequest { types: &[], languages: &[] }).unwrap().is_empty());
}
