//! Try the document adapter on a real file:
//!
//! ```text
//! cargo run -p pocketanon-documents --example roundtrip -- <in> <out> [term ...]
//! ```
//!
//! Prints every segment with its kind, replaces each occurrence of the given terms (a stand-in
//! for the detectors) with an indexed placeholder, saves to `<out>` in the input's format, then
//! reloads `<out>` and reports any term that survived. Also renders page 1 as `<out>.page1.ppm`.

use std::process::ExitCode;

use pocketanon_core::replace::{Indexer, Strategy, plan_replacements};
use pocketanon_core::types::{Entity, Span};
use pocketanon_core::{SegmentEdits, Summary};
use pocketanon_documents::{detect_format, load};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, terms @ ..] = args.as_slice() else {
        eprintln!("usage: roundtrip <in> <out> [term ...]");
        return ExitCode::from(2);
    };
    match run(input, output, terms) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &str, output: &str, terms: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let format = detect_format(input.as_ref()).ok_or("unsupported extension (txt, docx, rtf)")?;
    println!("{input}: {format} — {}", format.fidelity());
    let t0 = std::time::Instant::now();
    let mut doc = load(&std::fs::read(input)?, format)?;
    let segments = doc.segments();
    println!("loaded in {:?}, {} segments", t0.elapsed(), segments.len());

    let mut indexer = Indexer::new(Strategy::IndexedRedaction);
    let mut summary = Summary::default();
    let mut edits = Vec::new();
    for s in &segments {
        println!("  #{:<4} {:<9} {:?}", s.id.0, s.kind.to_string(), s.text);
        let mut ents = Vec::new();
        for term in terms.iter().filter(|t| !t.is_empty()) {
            ents.extend(s.text.match_indices(term.as_str()).map(|(i, _)| Entity::new(Span::new(i, i + term.len()), "term")));
        }
        let (replacements, sum) = plan_replacements(&s.text, &ents, &mut indexer);
        summary.merge_from(&sum);
        if !replacements.is_empty() {
            edits.push(SegmentEdits { segment: s.id, replacements });
        }
    }
    doc.apply(&edits)?;
    std::fs::write(output, doc.save()?)?;
    println!("wrote {output}: {} replacements", summary.total());

    let again = load(&std::fs::read(output)?, format)?;
    let survivors: Vec<_> = again.segments().into_iter().filter(|s| terms.iter().any(|t| !t.is_empty() && s.text.contains(t.as_str()))).collect();
    for s in &survivors {
        println!("  SURVIVED in #{} {}: {:?}", s.id.0, s.kind, s.text);
    }

    let t0 = std::time::Instant::now();
    let pages = again.page_count();
    if let Some(img) = again.render_page(0, 1.5) {
        let ppm = format!("{output}.page1.ppm");
        let mut bytes = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
        bytes.extend(img.rgba.chunks_exact(4).flat_map(|px| [px[0], px[1], px[2]]));
        std::fs::write(&ppm, bytes)?;
        println!("{pages} page(s); page 1 rendered to {ppm} in {:?}", t0.elapsed());
    }
    if survivors.is_empty() { Ok(()) } else { Err("some terms survived".into()) }
}
