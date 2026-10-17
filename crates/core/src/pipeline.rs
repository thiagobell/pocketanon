//! Runs every detector over every chunk and merges the results.

use crate::chunk::ChunkPlan;
use crate::detector::{DetectRequest, Detector, DetectorError};
use crate::merge::merge;
use crate::types::{Candidate, EntityType, Language};

#[derive(Clone, Debug, PartialEq)]
pub struct PipelineConfig {
    pub types: Vec<EntityType>,
    pub languages: Vec<Language>,
    /// Fallback chunk size when no detector declares a limit.
    pub default_max_words: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        PipelineConfig {
            types: crate::catalog::TypeCatalog::builtin().default_types(),
            languages: vec![Language::german(), Language::english()],
            default_max_words: 300,
        }
    }
}

/// A set of detectors plus the configuration they run with.
pub struct Pipeline {
    detectors: Vec<Box<dyn Detector>>,
    pub config: PipelineConfig,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Pipeline { detectors: Vec::new(), config }
    }

    pub fn with_detector(mut self, d: impl Detector + 'static) -> Self {
        self.detectors.push(Box::new(d));
        self
    }

    pub fn add_detector(&mut self, d: Box<dyn Detector>) {
        self.detectors.push(d);
    }

    pub fn detectors(&self) -> &[Box<dyn Detector>] {
        &self.detectors
    }

    /// The chunk size every detector can accept.
    pub fn max_words(&self) -> usize {
        self.detectors
            .iter()
            .filter_map(|d| d.max_words())
            .min()
            .unwrap_or(self.config.default_max_words)
            .max(1)
    }

    /// Detect over the whole input. Returns merged, non-overlapping candidates with offsets into
    /// `text`. A failing detector aborts the run; the UI decides how to present that.
    pub fn detect(&self, text: &str) -> Result<Vec<Candidate>, DetectorError> {
        Ok(self.detect_many(&[text])?.pop().unwrap_or_default())
    }

    /// Detect over several independent texts (e.g. the segments of a document). Every text is
    /// chunked, all chunks go to each detector in one batch, and the results are merged per
    /// text. Offsets are relative to each input text.
    pub fn detect_many(&self, texts: &[&str]) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        let max_words = self.max_words();
        let req = DetectRequest { types: &self.config.types, languages: &self.config.languages };
        // (text index, chunk span) for every chunk of every text, in order.
        let mut owners: Vec<(usize, crate::types::Span)> = Vec::new();
        let mut chunk_texts: Vec<&str> = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            for chunk in ChunkPlan::new(text, max_words).chunks {
                owners.push((i, chunk.span));
                chunk_texts.push(chunk.text(text));
            }
        }
        let mut per_text: Vec<Vec<Candidate>> = vec![Vec::new(); texts.len()];
        if chunk_texts.is_empty() {
            return Ok(per_text);
        }
        for d in &self.detectors {
            let results = d.detect_batch(&chunk_texts, &req)?;
            if results.len() != chunk_texts.len() {
                return Err(DetectorError::failed(
                    d.name(),
                    format!("returned {} results for {} chunks", results.len(), chunk_texts.len()),
                ));
            }
            for ((owner, span), (chunk_text, local)) in owners.iter().zip(chunk_texts.iter().zip(results)) {
                for mut c in local {
                    if !c.span.is_valid_in(chunk_text) {
                        continue; // defensive: a detector returned garbage offsets
                    }
                    c.span = c.span.shifted(span.start);
                    per_text[*owner].push(c);
                }
            }
        }
        Ok(per_text.into_iter().map(merge).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::{DetectorKind, FnDetector};
    use crate::types::Span;

    /// Finds every occurrence of `needle` in the chunk.
    fn finder(
        kind: DetectorKind,
        needle: &'static str,
        t: &'static str,
        conf: f32,
    ) -> FnDetector<impl Fn(&str, &DetectRequest<'_>) -> Vec<Candidate> + Send + Sync> {
        FnDetector::new(kind, format!("find-{needle}"), move |text, _| {
            text.match_indices(needle)
                .map(|(i, m)| Candidate::new(Span::new(i, i + m.len()), t, conf, kind))
                .collect()
        })
    }

    #[test]
    fn offsets_are_global_across_chunks() {
        let text = "Anna hier.\n\nBob dort.\n\nAnna wieder.";
        let p = Pipeline::new(PipelineConfig { default_max_words: 2, ..Default::default() })
            .with_detector(finder(DetectorKind::Model, "Anna", "person", 0.9))
            .with_detector(finder(DetectorKind::Model, "Bob", "person", 0.9));
        assert!(ChunkPlan::new(text, 2).len() >= 3);
        let out = p.detect(text).unwrap();
        let found: Vec<&str> = out.iter().map(|c| c.text(text)).collect();
        assert_eq!(found, vec!["Anna", "Bob", "Anna"]);
        assert_eq!(out[2].span, Span::new(23, 27));
    }

    #[test]
    fn smallest_detector_limit_wins() {
        let p = Pipeline::new(PipelineConfig::default())
            .with_detector(finder(DetectorKind::Model, "x", "t", 0.5).with_max_words(50))
            .with_detector(finder(DetectorKind::Rule, "y", "t", 0.5));
        assert_eq!(p.max_words(), 50);
    }

    #[test]
    fn rule_and_model_merge_across_pipeline() {
        let text = "IBAN DE89370400440532013000 end";
        let p = Pipeline::new(PipelineConfig::default())
            .with_detector(finder(DetectorKind::Model, "DE8937040044", "customer number", 0.7))
            .with_detector(finder(DetectorKind::Rule, "DE89370400440532013000", "iban", 1.0));
        let out = p.detect(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text(text), "DE89370400440532013000");
        assert_eq!(out[0].entity_type.as_str(), "iban");
    }

    #[test]
    fn detect_many_keeps_results_per_text_and_batches_chunks() {
        use std::sync::Mutex;
        let calls = std::sync::Arc::new(Mutex::new(Vec::<usize>::new()));
        let c2 = calls.clone();
        struct Batchy(std::sync::Arc<Mutex<Vec<usize>>>);
        impl Detector for Batchy {
            fn kind(&self) -> DetectorKind {
                DetectorKind::Model
            }
            fn name(&self) -> String {
                "batchy".into()
            }
            fn max_words(&self) -> Option<usize> {
                Some(2)
            }
            fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
                Ok(self.detect_batch(&[text], req)?.pop().unwrap())
            }
            fn detect_batch(&self, texts: &[&str], _: &DetectRequest<'_>) -> Result<Vec<Vec<Candidate>>, DetectorError> {
                self.0.lock().unwrap().push(texts.len());
                Ok(texts
                    .iter()
                    .map(|t| {
                        t.match_indices("Anna")
                            .map(|(i, m)| Candidate::new(Span::new(i, i + m.len()), "person", 0.9, DetectorKind::Model))
                            .collect()
                    })
                    .collect())
            }
        }
        let p = Pipeline::new(PipelineConfig::default()).with_detector(Batchy(c2));
        let texts = ["Anna hier.\n\nBob dort.", "", "Und Anna nochmal da"];
        let out = p.detect_many(&texts).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].len(), 1);
        assert!(out[1].is_empty());
        assert_eq!(out[2][0].span, Span::new(4, 8));
        // one batch call for all chunks of all texts
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(calls.lock().unwrap()[0] >= 3);
    }

    #[test]
    fn garbage_offsets_are_dropped() {
        let p = Pipeline::new(PipelineConfig::default()).with_detector(FnDetector::new(
            DetectorKind::Model,
            "bad",
            |_, _| vec![Candidate::new(Span::new(0, 999), "t", 1.0, DetectorKind::Model)],
        ));
        assert!(p.detect("short").unwrap().is_empty());
    }
}
