//! Headless end-to-end run of the real services: rules + GLiNER (if the default model is
//! installed) over a TXT, DOCX or RTF file, one shared Indexer, saved beside the input.
//!
//! `cargo run -p pocketanon --example anonymize -- fixtures/letter_de.docx [threshold]`
use std::path::Path;

use pocketanon_core::{Indexer, Pipeline, PipelineConfig, Review, SegmentEdits, Strategy, Summary, plan_replacements};
use pocketanon_detect_gliner::{GlinerDetector, GlinerOptions, ModelStatus, ModelStore, Quantization, Registry};
use pocketanon_detect_rules::RuleDetector;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("usage: anonymize <file> [threshold]");
    let threshold: f32 = args.next().and_then(|t| t.parse().ok()).unwrap_or(0.3);
    let input = Path::new(&input);

    let mut pipeline = Pipeline::new(PipelineConfig::default()).with_detector(RuleDetector::new());
    let registry = Registry::builtin();
    let store = ModelStore::with_default_dir().expect("data dir");
    match store.status(registry.default_model()) {
        ModelStatus::Installed(dir) => {
            let t = std::time::Instant::now();
            // POCKETANON_QUANT=int8|fp16|fp32 picks the ONNX variant (default: whatever is installed).
            let quantization = match std::env::var("POCKETANON_QUANT").ok().as_deref() {
                Some("int8") => Some(Quantization::Int8),
                Some("fp16") => Some(Quantization::Fp16),
                Some("fp32") => Some(Quantization::Fp32),
                _ => None,
            };
            // POCKETANON_COREML=1 enables the CoreML provider; POCKETANON_BATCH=n sets texts per run.
            let coreml = std::env::var("POCKETANON_COREML").is_ok_and(|v| v == "1");
            let batch_size = std::env::var("POCKETANON_BATCH").ok().and_then(|v| v.parse().ok()).unwrap_or(pocketanon_detect_gliner::DEFAULT_BATCH_SIZE);
            let mut opts = GlinerOptions { quantization, coreml, batch_size, ..GlinerOptions::default() };
            if let Some(t) = std::env::var("POCKETANON_THREADS").ok().and_then(|v| v.parse().ok()) {
                opts.threads = t;
            }
            if let Some(w) = std::env::var("POCKETANON_WORDS").ok().and_then(|v| v.parse().ok()) {
                opts.max_words = w;
            }
            let det = GlinerDetector::load(&dir, opts).expect("load model");
            eprintln!("onnx: {}", det.onnx_path().display());
            eprintln!("model loaded in {:?}", t.elapsed());
            pipeline.add_detector(Box::new(det));
        }
        other => eprintln!("model not installed ({other:?}); rules only"),
    }

    let format = pocketanon_documents::detect_format(input).expect("supported format");
    let bytes = std::fs::read(input).expect("read");
    let mut doc = pocketanon_documents::load(&bytes, format).expect("load");
    let segments = doc.segments();

    let t = std::time::Instant::now();
    let mut indexer = Indexer::new(Strategy::IndexedRedaction);
    let mut summary = Summary::default();
    let mut edits = Vec::new();
    let texts: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
    // POCKETANON_PER_SEGMENT=1 runs every segment on its own (no packing), for comparisons.
    let all = if std::env::var("POCKETANON_PER_SEGMENT").is_ok() {
        texts.iter().map(|t| pipeline.detect(t).expect("detect")).collect()
    } else {
        pipeline.detect_many(&texts).expect("detect")
    };
    for (seg, candidates) in segments.iter().zip(all) {
        let review = Review::new(candidates, threshold);
        for item in review.visible() {
            println!(
                "{:<9} {:<20} {:.2} {:<6} {:?}",
                seg.kind.to_string(),
                item.candidate.entity_type.as_str(),
                item.candidate.confidence,
                item.candidate.detector.to_string(),
                item.candidate.text(&seg.text)
            );
        }
        let (reps, s) = plan_replacements(&seg.text, &review.entities(), &mut indexer);
        summary.merge_from(&s);
        if !reps.is_empty() {
            edits.push(SegmentEdits { segment: seg.id, replacements: reps });
        }
    }
    eprintln!("detection over {} segments took {:?}", segments.len(), t.elapsed());
    doc.apply(&edits).expect("apply");
    let out = pocketanon_documents::output_path(input);
    std::fs::write(&out, doc.save().expect("save")).expect("write");
    println!("\nSummary: {} entities", summary.total());
    for (t, n) in &summary.counts {
        println!("  {t}: {n}");
    }
    println!("wrote {}", out.display());
}
