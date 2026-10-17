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
        let plan = ChunkPlan::new(text, self.max_words());
        let req = DetectRequest { types: &self.config.types, languages: &self.config.languages };
        let mut all = Vec::new();
        for d in &self.detectors {
            for chunk in &plan.chunks {
                let local = d.detect(chunk.text(text), &req)?;
                for mut c in local {
                    if !c.span.is_valid_in(chunk.text(text)) {
                        continue; // defensive: a detector returned garbage offsets
                    }
                    c.span = c.span.shifted(chunk.span.start);
                    all.push(c);
                }
            }
        }
        Ok(merge(all))
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
    fn garbage_offsets_are_dropped() {
        let p = Pipeline::new(PipelineConfig::default()).with_detector(FnDetector::new(
            DetectorKind::Model,
            "bad",
            |_, _| vec![Candidate::new(Span::new(0, 999), "t", 1.0, DetectorKind::Model)],
        ));
        assert!(p.detect("short").unwrap().is_empty());
    }
}
