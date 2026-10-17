//! Times one inference on a ~150-word paragraph after warm-up.
//!
//! `POCKETANON_MODEL_DIR=<dir> cargo run --release -p pocketanon-detect-gliner --example bench -- [onnx file]`

use std::path::PathBuf;
use std::time::Instant;

use pocketanon_core::{DetectRequest, Detector, Language, TypeCatalog};
use pocketanon_detect_gliner::{GlinerDetector, GlinerOptions};

const PARAGRAPH: &str = "Sehr geehrte Frau Musterfrau, vielen Dank für Ihr Schreiben vom 12. März 2024. \
Wie besprochen bestätigen wir hiermit, dass Herr Max Mustermann, geboren am 01.01.1980, seit dem \
1. Juli 2019 bei der Beispiel GmbH in Bonn als Projektleiter beschäftigt ist. Herr Mustermann wohnt \
in der Musterstraße 12, 53111 Bonn, und ist telefonisch unter +49 228 123456 sowie per E-Mail unter \
max@example.com erreichbar. Die Gehaltszahlungen erfolgen auf das Konto mit der IBAN \
DE89 3704 0044 0532 0130 00 bei der Beispielbank. Für Rückfragen steht Ihnen unsere \
Personalabteilung, Frau Erika Beispiel, gerne zur Verfügung. Bitte geben Sie bei jeder \
Korrespondenz die Personalnummer 4711-0815 sowie das Aktenzeichen AZ 2024/123 an. \
Our partner office in London, managed by John Smith, will forward the documents to the \
tax office in Berlin. Mit freundlichen Grüßen, Ihre Beispiel GmbH, Abteilung Personal, \
Hauptstraße 1, 53113 Bonn.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::var("POCKETANON_MODEL_DIR")?);
    let mut opts = GlinerOptions::default();
    if let Some(f) = std::env::args().nth(1) {
        opts.onnx_file = Some(f.into());
    }
    let t = Instant::now();
    let det = GlinerDetector::load(&dir, opts)?;
    println!("model {} ({}) loaded in {:.2?}", det.name(), det.onnx_path().display(), t.elapsed());

    let types = TypeCatalog::builtin().default_types();
    let langs = [Language::german(), Language::english()];
    let req = DetectRequest { types: &types, languages: &langs };
    println!("{} words, {} types", PARAGRAPH.split_whitespace().count(), types.len());

    for _ in 0..2 {
        det.detect(PARAGRAPH, &req)?;
    }
    let runs = 5;
    let mut times = Vec::new();
    let mut cands = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        cands = det.detect(PARAGRAPH, &req)?;
        times.push(t.elapsed());
    }
    times.sort();
    println!("inference: min {:.2?}, median {:.2?}, max {:.2?} over {runs} runs", times[0], times[runs / 2], times[runs - 1]);
    for c in &cands {
        println!("  {:28} {:18} {:.3}", c.text(PARAGRAPH), c.entity_type, c.confidence);
    }
    Ok(())
}
