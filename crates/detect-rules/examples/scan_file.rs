//! `cargo run -p pocketanon-detect-rules --example scan_file -- fixtures/letter_de.txt de`
use pocketanon_core::{DetectRequest, Detector, Language, TypeCatalog};
use pocketanon_detect_rules::RuleDetector;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("path");
    let langs: Vec<Language> = args.map(Language::new).collect();
    let text = std::fs::read_to_string(&path).expect("read");
    let types = TypeCatalog::builtin().all_types();
    let req = DetectRequest { types: &types, languages: &langs };
    for c in RuleDetector::new().detect(&text, &req).unwrap() {
        println!("{:<22} {:.2}  {:?}", c.entity_type.as_str(), c.confidence, c.text(&text));
    }
}
