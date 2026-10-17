//! [`GlinerDetector`]: the Model Detector.

use std::path::{Path, PathBuf};

use gliner::model::GLiNER;
use gliner::model::input::text::TextInput;
use gliner::model::params::Parameters;
use gliner::model::pipeline::span::SpanMode;
use orp::params::RuntimeParameters;
use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorError, DetectorKind, EntityType, Span};

use crate::error::GlinerError;
use crate::registry::Quantization;
use crate::store::find_onnx_file;

/// Word limit of the GLiNER models we ship (`max_len` in `gliner_config.json`), prompt included.
pub const MODEL_MAX_WORDS: usize = 384;
/// Maximum number of labels per model call (`max_types` in `gliner_config.json`).
pub const MAX_TYPES_PER_CALL: usize = 25;
/// Span width in words (`max_width`).
pub const MAX_SPAN_WIDTH: usize = 12;
/// Words per chunk when the active type count is unknown. Leaves room for a full label prompt
/// and for GLiNER counting punctuation as separate words (the pipeline counts whitespace words).
pub const DEFAULT_MAX_WORDS: usize = 250;
/// Low on purpose: the Review Threshold does the filtering.
pub const DEFAULT_THRESHOLD: f32 = 0.2;

/// How to load a Model.
#[derive(Clone, Debug)]
pub struct GlinerOptions {
    /// Which ONNX variant to load. `None` takes the first present of fp16, int8, fp32, then any
    /// `*.onnx` file.
    pub quantization: Option<Quantization>,
    /// Explicit ONNX file (absolute, or relative to the model directory); overrides `quantization`.
    pub onnx_file: Option<PathBuf>,
    /// Minimum span probability the model reports.
    pub threshold: f32,
    /// ONNX Runtime intra-op threads.
    pub threads: usize,
    /// Detector name; defaults to the model directory name (the Model id in a `ModelStore`).
    pub name: Option<String>,
}

impl Default for GlinerOptions {
    fn default() -> Self {
        GlinerOptions {
            quantization: None,
            onnx_file: None,
            threshold: DEFAULT_THRESHOLD,
            threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
            name: None,
        }
    }
}

/// The Model Detector: zero-shot GLiNER (span mode) over the active Entity Types.
pub struct GlinerDetector {
    name: String,
    onnx_path: PathBuf,
    model: GLiNER<SpanMode>,
}

// `GLiNER<SpanMode>` holds an `ort::Session` (thread-safe `run(&self)`), a `tokenizers::Tokenizer`
// and a `regex::Regex`, all `Send + Sync`; no mutex needed. Checked at compile time:
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GlinerDetector>();
};

impl GlinerDetector {
    /// Loads `tokenizer.json` and an ONNX graph from `model_dir`.
    pub fn load(model_dir: &Path, options: GlinerOptions) -> Result<Self, GlinerError> {
        let tokenizer = model_dir.join("tokenizer.json");
        if !tokenizer.is_file() {
            return Err(GlinerError::MissingFile(model_dir.to_path_buf(), "tokenizer.json".into()));
        }
        let onnx_path = match &options.onnx_file {
            Some(f) => {
                let p = if f.is_absolute() { f.clone() } else { model_dir.join(f) };
                if !p.is_file() {
                    return Err(GlinerError::MissingFile(model_dir.to_path_buf(), f.display().to_string()));
                }
                p
            }
            None => match options.quantization {
                Some(q) => {
                    let p = model_dir.join(q.onnx_file());
                    if !p.is_file() {
                        return Err(GlinerError::MissingFile(model_dir.to_path_buf(), q.onnx_file().into()));
                    }
                    p
                }
                None => find_onnx_file(model_dir, None)
                    .ok_or_else(|| GlinerError::MissingFile(model_dir.to_path_buf(), "an .onnx model file".into()))?,
            },
        };
        let name = options.name.clone().unwrap_or_else(|| {
            model_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "gliner".into())
        });

        // max_length: None so text is never silently truncated (a truncated tail would leak
        // personal data unnoticed). The pipeline keeps chunks below `max_words()`.
        let params = Parameters::default()
            .with_threshold(options.threshold)
            .with_max_width(MAX_SPAN_WIDTH)
            .with_max_length(None)
            .with_flat_ner(true)
            .with_dup_label(false)
            .with_multi_label(false);
        let runtime = RuntimeParameters::default()
            .with_threads(options.threads.max(1))
            .with_execution_providers(execution_providers());

        let model = GLiNER::<SpanMode>::new(params, runtime, tokenizer, onnx_path.clone())
            .map_err(|e| GlinerError::Load(format!("{}: {e}", onnx_path.display())))?;
        log::info!("loaded GLiNER model {name} from {}", onnx_path.display());
        Ok(GlinerDetector { name, onnx_path, model })
    }

    /// The ONNX file actually loaded.
    pub fn onnx_path(&self) -> &Path {
        &self.onnx_path
    }

    /// Word limit for a chunk when `n_types` Entity Types are active.
    pub fn max_words_for(&self, n_types: usize) -> usize {
        max_words_for_types(n_types)
    }

    /// Runs the model for `labels` (already normalised), batching them by
    /// [`MAX_TYPES_PER_CALL`].
    pub fn detect_labels(&self, text: &str, labels: &[String]) -> Result<Vec<Candidate>, GlinerError> {
        Ok(self.detect_labels_batch(&[text], labels)?.pop().unwrap_or_default())
    }

    /// One inference over all `texts` (per label batch). Empty texts are skipped and come back
    /// as empty results, so the output always has one entry per input.
    pub fn detect_labels_batch(&self, texts: &[&str], labels: &[String]) -> Result<Vec<Vec<Candidate>>, GlinerError> {
        let mut out: Vec<Vec<Candidate>> = vec![Vec::new(); texts.len()];
        let live: Vec<usize> = (0..texts.len()).filter(|&i| !texts[i].trim().is_empty()).collect();
        if live.is_empty() || labels.is_empty() {
            return Ok(out);
        }
        let batches = batch_labels(labels, MAX_TYPES_PER_CALL);
        let multi = batches.len() > 1;
        for batch in batches {
            let input = TextInput::new(live.iter().map(|&i| texts[i].to_string()).collect(), batch)
                .map_err(|e| GlinerError::Inference(e.to_string()))?;
            let output = self.model.inference(input).map_err(|e| GlinerError::Inference(e.to_string()))?;
            for (k, spans) in output.spans.iter().enumerate() {
                let Some(&i) = live.get(k) else { break };
                out[i].extend(spans_to_candidates(texts[i], spans.iter()));
            }
        }
        for cands in out.iter_mut() {
            if multi {
                *cands = flatten_overlaps(std::mem::take(cands));
            }
            cands.sort_by_key(|c| c.span);
        }
        Ok(out)
    }

    fn labels_for(&self, req: &DetectRequest<'_>) -> Vec<String> {
        let mut labels: Vec<String> = Vec::with_capacity(req.types.len());
        for t in req.types {
            let l = t.as_str();
            if !l.is_empty() && !labels.iter().any(|x| x == l) {
                labels.push(l.to_string());
            }
        }
        labels
    }
}

impl Detector for GlinerDetector {
    fn kind(&self) -> DetectorKind {
        DetectorKind::Model
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    /// The trait has no access to the active types, so this uses [`DEFAULT_MAX_WORDS`]; use
    /// [`GlinerDetector::max_words_for`] when the type count is known.
    fn max_words(&self) -> Option<usize> {
        Some(DEFAULT_MAX_WORDS)
    }

    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        Ok(self.detect_batch(&[text], req)?.pop().unwrap_or_default())
    }

    fn detect_batch(&self, texts: &[&str], req: &DetectRequest<'_>) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        let labels = self.labels_for(req);
        let limit = max_words_for_types(labels.len().min(MAX_TYPES_PER_CALL));
        for text in texts {
            let words = text.split_whitespace().count();
            if words > limit {
                log::warn!("{}: chunk of {words} words exceeds the recommended limit", self.name);
            }
        }
        self.detect_labels_batch(texts, &labels).map_err(|e| DetectorError::failed(self.name.clone(), e))
    }
}

/// `MODEL_MAX_WORDS - (2 * n + 2)` clamped to `[64, 300]`, with `n` capped at
/// [`MAX_TYPES_PER_CALL`] because larger type sets are split into batches.
pub fn max_words_for_types(n_types: usize) -> usize {
    let n = n_types.min(MAX_TYPES_PER_CALL);
    MODEL_MAX_WORDS.saturating_sub(2 * n + 2).clamp(64, 300)
}

/// Splits `labels` into the fewest batches of at most `max` labels, sizes balanced.
pub fn batch_labels(labels: &[String], max: usize) -> Vec<Vec<String>> {
    if labels.is_empty() {
        return Vec::new();
    }
    let max = max.max(1);
    let n_batches = labels.len().div_ceil(max);
    let size = labels.len().div_ceil(n_batches);
    labels.chunks(size).map(<[String]>::to_vec).collect()
}

/// Maps gline-rs spans (byte offsets into `text`) to Candidates, dropping any whose offsets are
/// out of bounds, not on char boundaries, or whose text does not match.
pub fn spans_to_candidates<'s>(
    text: &str,
    spans: impl IntoIterator<Item = &'s gliner::text::span::Span>,
) -> Vec<Candidate> {
    spans
        .into_iter()
        .filter_map(|s| {
            let (start, end) = s.offsets();
            if start >= end {
                return None;
            }
            let span = Span::new(start, end);
            if !span.is_valid_in(text) || &text[span.range()] != s.text() {
                log::debug!("dropping GLiNER span {start}..{end} with invalid offsets");
                return None;
            }
            Some(Candidate::new(span, EntityType::new(s.class()), s.probability(), DetectorKind::Model))
        })
        .collect()
}

/// Keeps the highest-confidence candidate among overlapping ones (what `flat_ner` does within
/// one call), so separate label batches yield a flat result too.
fn flatten_overlaps(mut cands: Vec<Candidate>) -> Vec<Candidate> {
    cands.sort_by(|a, b| b.confidence.total_cmp(&a.confidence).then(a.span.cmp(&b.span)));
    let mut kept: Vec<Candidate> = Vec::with_capacity(cands.len());
    for c in cands {
        if !kept.iter().any(|k| k.span.overlaps(&c.span)) {
            kept.push(c);
        }
    }
    kept
}

#[cfg(feature = "coreml")]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    vec![ort::execution_providers::CoreMLExecutionProvider::default().build()]
}

#[cfg(not(feature = "coreml"))]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gliner::text::span::Span as GSpan;

    fn labels(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("type {i}")).collect()
    }

    #[test]
    fn batching() {
        assert!(batch_labels(&[], 25).is_empty());
        let sizes = |n, max| batch_labels(&labels(n), max).iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(sizes(1, 25), [1]);
        assert_eq!(sizes(25, 25), [25]);
        assert_eq!(sizes(26, 25), [13, 13]);
        assert_eq!(sizes(51, 25), [17, 17, 17]);
        assert_eq!(sizes(3, 0), [1, 1, 1]);
        // order preserved, nothing lost
        let all = labels(60);
        assert_eq!(batch_labels(&all, 25).concat(), all);
        assert!(batch_labels(&all, 25).iter().all(|b| b.len() <= 25));
    }

    #[test]
    fn word_limits() {
        assert_eq!(max_words_for_types(0), 300);
        // 384 - 52 = 332, clamped to 300
        assert_eq!(max_words_for_types(25), 300);
        assert_eq!(max_words_for_types(100), max_words_for_types(25));
        assert!(DEFAULT_MAX_WORDS <= max_words_for_types(25));
        assert!((64..=300).contains(&max_words_for_types(1000)));
    }

    #[test]
    fn maps_multibyte_spans() {
        let text = "Jürgen Müßig wohnt in Köln — erreichbar unter jü@example.com.";
        let find = |s: &str| {
            let i = text.find(s).unwrap();
            (i, i + s.len())
        };
        let (a, b) = find("Jürgen Müßig");
        let (c, d) = find("Köln");
        let (e, f) = find("jü@example.com");
        let spans = vec![
            GSpan::new(0, a, b, "Jürgen Müßig".into(), "Person".into(), 0.93),
            GSpan::new(0, c, d, "Köln".into(), "location".into(), 0.81),
            GSpan::new(0, e, f, "jü@example.com".into(), "email".into(), 1.2),
            // inside 'ü' (2-byte char): not a char boundary
            GSpan::new(0, 1, 3, "x".into(), "person".into(), 0.9),
            // out of bounds
            GSpan::new(0, text.len() - 1, text.len() + 5, "x".into(), "person".into(), 0.9),
            // offsets valid but text mismatch
            GSpan::new(0, c, d, "Bonn".into(), "location".into(), 0.9),
        ];
        let cands = spans_to_candidates(text, &spans);
        assert_eq!(cands.len(), 3);
        assert_eq!(cands[0].text(text), "Jürgen Müßig");
        assert_eq!(cands[0].entity_type, EntityType::new("person"));
        assert_eq!(cands[0].detector, DetectorKind::Model);
        assert!((cands[0].confidence - 0.93).abs() < 1e-6);
        assert_eq!(cands[1].text(text), "Köln");
        assert_eq!(cands[2].text(text), "jü@example.com");
        assert_eq!(cands[2].confidence, 1.0, "confidence clamped");
        assert!(cands.iter().all(|c| c.span.is_valid_in(text)));
    }

    #[test]
    fn flattening_across_batches() {
        let c = |s, e, t: &str, p| Candidate::new(Span::new(s, e), t, p, DetectorKind::Model);
        let out = flatten_overlaps(vec![c(0, 5, "person", 0.6), c(0, 10, "organization", 0.8), c(12, 15, "location", 0.3)]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].entity_type, EntityType::new("organization"));
        assert_eq!(out[1].span, Span::new(12, 15));
    }
}
