//! Runs every detector over every chunk and merges the results.

use crate::chunk::ChunkPlan;
use crate::detector::{DetectRequest, Detector, DetectorError};
use crate::merge::merge;
use crate::types::{Candidate, EntityType, Language, Span};

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

    /// Detect over several independent texts (e.g. the segments of a document). Offsets are
    /// relative to each input text. See [`Pipeline::detect_many_with`].
    pub fn detect_many(&self, texts: &[&str]) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        self.detect_many_with(texts, &mut |_| {})
    }

    /// Like [`Pipeline::detect_many`], reporting [`Progress`] after every slice of chunks a
    /// detector finishes.
    ///
    /// Detectors with a word limit (models, whose cost per sequence is high) see the texts
    /// **packed**: all texts are joined and re-chunked, so many short paragraphs share one
    /// sequence instead of each paying the per-sequence price. Candidates that would cross a
    /// text boundary are dropped. Detectors without a limit get each text on its own.
    pub fn detect_many_with(
        &self,
        texts: &[&str],
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        let max_words = self.max_words();
        let req = DetectRequest { types: &self.config.types, languages: &self.config.languages };
        let mut per_text: Vec<Vec<Candidate>> = vec![Vec::new(); texts.len()];
        if texts.iter().all(|t| t.trim().is_empty()) {
            return Ok(per_text);
        }

        // Packed view for model detectors: runs of short texts (headings, address lines, table
        // cells) are joined into one string, separated by newlines (a paragraph boundary for
        // the chunker); longer texts stay alone. Measured on gliner_multi_pii-v1: merging whole
        // paragraphs into 120+ word sequences lost names and addresses, merging only short
        // lines did not, and short lines are where the per-sequence overhead dominates.
        struct Group {
            text: String,
            owners: Vec<(usize, Span)>, // (text index, span within `text`)
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut open: Option<(Group, usize)> = None; // group being filled, its word count
        for (i, t) in texts.iter().enumerate() {
            if t.trim().is_empty() {
                continue;
            }
            let words = crate::chunk::word_count(t);
            if words <= PACK_SHORT_WORDS {
                if open.as_ref().is_some_and(|(_, n)| n + words > PACK_MAX_WORDS)
                    && let Some((full, _)) = open.take()
                {
                    groups.push(full);
                }
                let (g, n) = open.get_or_insert_with(|| (Group { text: String::new(), owners: Vec::new() }, 0));
                let start = g.text.len();
                g.text.push_str(t);
                g.owners.push((i, Span::new(start, g.text.len())));
                g.text.push('\n');
                *n += words;
            } else {
                if let Some((full, _)) = open.take() {
                    groups.push(full);
                }
                groups.push(Group { text: t.to_string(), owners: vec![(i, Span::new(0, t.len()))] });
            }
        }
        if let Some((full, _)) = open.take() {
            groups.push(full);
        }

        // Work items per detector: chunk spans and the texts they index into.
        struct Job<'a> {
            packed: bool,
            spans: Vec<(usize, Span)>, // (text index, or group index when packed; chunk span)
            source: Vec<&'a str>,
        }
        let mut jobs: Vec<Job<'_>> = Vec::with_capacity(self.detectors.len());
        for d in &self.detectors {
            let mut spans = Vec::new();
            let mut source = Vec::new();
            if d.max_words().is_some() {
                for (gi, g) in groups.iter().enumerate() {
                    for chunk in ChunkPlan::new(&g.text, max_words).chunks {
                        spans.push((gi, chunk.span));
                        source.push(chunk.text(&g.text));
                    }
                }
                jobs.push(Job { packed: true, spans, source });
            } else {
                for (i, text) in texts.iter().enumerate() {
                    for chunk in ChunkPlan::new(text, max_words).chunks {
                        spans.push((i, chunk.span));
                        source.push(chunk.text(text));
                    }
                }
                jobs.push(Job { packed: false, spans, source });
            }
        }
        let total: usize = jobs.iter().map(|j| j.spans.len()).sum();
        let mut done = 0usize;
        on_progress(Progress { done, total });

        for (d, job) in self.detectors.iter().zip(jobs) {
            let slice_len = if job.packed { PROGRESS_SLICE } else { job.spans.len().max(1) };
            for (spans, chunk_texts) in job.spans.chunks(slice_len).zip(job.source.chunks(slice_len)) {
                let results = d.detect_batch(chunk_texts, &req)?;
                if results.len() != chunk_texts.len() {
                    return Err(DetectorError::failed(
                        d.name(),
                        format!("returned {} results for {} chunks", results.len(), chunk_texts.len()),
                    ));
                }
                for (((owner, span), chunk_text), local) in spans.iter().zip(chunk_texts).zip(results) {
                    for mut c in local {
                        if !c.span.is_valid_in(chunk_text) {
                            continue; // defensive: a detector returned garbage offsets
                        }
                        c.span = c.span.shifted(span.start);
                        if job.packed {
                            for (i, local_span) in unpack(&groups[*owner].owners, texts, c.span) {
                                per_text[i].push(Candidate { span: local_span, ..c.clone() });
                            }
                        } else {
                            per_text[*owner].push(c);
                        }
                    }
                }
                done += spans.len();
                on_progress(Progress { done, total });
            }
        }
        Ok(per_text.into_iter().map(merge).collect())
    }
}

/// Texts of at most this many words are packed together for model detectors.
pub const PACK_SHORT_WORDS: usize = 12;
/// Word limit of one pack of short texts.
pub const PACK_MAX_WORDS: usize = 48;

/// Chunks per `detect_batch` call for packed (model) detectors: small enough for progress to
/// move visibly, large enough to amortise a call.
pub const PROGRESS_SLICE: usize = 8;

/// How far a [`Pipeline::detect_many_with`] run has come, in chunks over all detectors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

impl Progress {
    /// `0.0..=1.0`; `1.0` when there is nothing to do.
    pub fn fraction(self) -> f32 {
        if self.total == 0 { 1.0 } else { self.done as f32 / self.total as f32 }
    }
}

/// Maps a span in a pack's coordinates to `(text index, span local to that text)` pieces. A span
/// that crosses text boundaries (an address block over several lines, say) is cut at them, one
/// piece per text, each trimmed of surrounding whitespace; empty pieces are dropped. `owners`
/// are `(text index, span of that text within the pack)` in ascending order.
fn unpack(owners: &[(usize, Span)], texts: &[&str], span: Span) -> Vec<(usize, Span)> {
    let mut out = Vec::new();
    let first = owners.partition_point(|(_, o)| o.end <= span.start);
    for &(i, o) in owners.iter().skip(first) {
        if o.start >= span.end {
            break;
        }
        let start = span.start.max(o.start) - o.start;
        let end = span.end.min(o.end) - o.start;
        let piece = &texts[i][start..end];
        let lead = piece.len() - piece.trim_start().len();
        let trail = piece.len() - piece.trim_end().len();
        if lead + trail < piece.len() {
            out.push((i, Span::new(start + lead, end - trail)));
        }
    }
    out
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
    fn packing_joins_short_texts_and_keeps_offsets_local() {
        use std::sync::{Arc, Mutex};
        struct Spy(Arc<Mutex<Vec<Vec<String>>>>);
        impl Detector for Spy {
            fn kind(&self) -> DetectorKind {
                DetectorKind::Model
            }
            fn name(&self) -> String {
                "spy".into()
            }
            fn max_words(&self) -> Option<usize> {
                Some(50)
            }
            fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
                Ok(self.detect_batch(&[text], req)?.pop().unwrap())
            }
            fn detect_batch(&self, texts: &[&str], _: &DetectRequest<'_>) -> Result<Vec<Vec<Candidate>>, DetectorError> {
                self.0.lock().unwrap().push(texts.iter().map(|t| t.to_string()).collect());
                Ok(texts
                    .iter()
                    .map(|t| {
                        let mut v: Vec<Candidate> = t
                            .match_indices("Anna")
                            .map(|(i, m)| Candidate::new(Span::new(i, i + m.len()), "person", 0.9, DetectorKind::Model))
                            .collect();
                        // a span across the newline between two texts: "x\ny Anna"
                        if let Some(i) = t.find("x\ny") {
                            v.push(Candidate::new(Span::new(i, i + 8), "address", 0.9, DetectorKind::Model));
                        }
                        v
                    })
                    .collect())
            }
        }
        let calls = Arc::new(Mutex::new(Vec::new()));
        let p = Pipeline::new(PipelineConfig::default()).with_detector(Spy(calls.clone()));
        let long = "Dies ist ein längerer Absatz mit deutlich mehr als zwölf Wörtern, der für sich allein bleibt.";
        let texts = ["Übersicht", "", "Anna x", "y Anna", long, "Ende"];
        let mut seen = Vec::new();
        let out = p.detect_many_with(&texts, &mut |pr| seen.push(pr)).unwrap();
        // short texts before the long one form one pack, the long one stays alone, "Ende" is a
        // pack of its own: one call with three chunks
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], vec!["Übersicht\nAnna x\ny Anna", long, "Ende"]);
        assert_eq!(out[0], vec![]);
        // "Anna" plus the "x" piece of the crossing address
        assert_eq!(out[2].len(), 2);
        assert_eq!(out[2][0].span, Span::new(0, 4));
        assert_eq!((out[2][1].span, out[2][1].entity_type.as_str()), (Span::new(5, 6), "address"));
        // the "y Anna" piece wins over the overlapping "Anna" by merge rules (same confidence,
        // longer span) or coexists; either way nothing points outside text 3
        assert!(out[3].iter().all(|c| c.span.end <= texts[3].len()));
        assert!(out[3].iter().any(|c| c.entity_type.as_str() == "address" && c.span == Span::new(0, 6)));
        assert!(out[4].is_empty() && out[5].is_empty());
        assert_eq!(seen.first(), Some(&Progress { done: 0, total: 3 }));
        assert_eq!(seen.last(), Some(&Progress { done: 3, total: 3 }));
    }

    #[test]
    fn unlimited_detectors_see_each_text_alone() {
        use std::sync::{Arc, Mutex};
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let c2 = calls.clone();
        let p = Pipeline::new(PipelineConfig::default()).with_detector(FnDetector::new(DetectorKind::Rule, "r", move |t, _| {
            c2.lock().unwrap().push(t.to_string());
            Vec::new()
        }));
        p.detect_many(&["a b", "c d"]).unwrap();
        assert_eq!(*calls.lock().unwrap(), vec!["a b", "c d"]);
    }

    #[test]
    fn unpack_splits_at_boundaries() {
        let texts = ["ab c", "d ef", ""];
        let owners = [(0, Span::new(0, 4)), (1, Span::new(5, 9)), (2, Span::new(10, 10))];
        assert_eq!(unpack(&owners, &texts, Span::new(5, 8)), vec![(1, Span::new(0, 3))]);
        assert_eq!(unpack(&owners, &texts, Span::new(0, 4)), vec![(0, Span::new(0, 4))]);
        // crossing: " c" + "d e" -> trimmed pieces
        assert_eq!(unpack(&owners, &texts, Span::new(2, 8)), vec![(0, Span::new(3, 4)), (1, Span::new(0, 3))]);
        // only the separator: nothing
        assert!(unpack(&owners, &texts, Span::new(4, 5)).is_empty());
        // reaching into the empty text yields nothing for it
        assert_eq!(unpack(&owners, &texts, Span::new(7, 10)), vec![(1, Span::new(2, 4))]);
    }

    #[test]
    fn packs_respect_the_word_limit() {
        use std::sync::{Arc, Mutex};
        let calls = Arc::new(Mutex::new(Vec::<usize>::new()));
        let c2 = calls.clone();
        let p = Pipeline::new(PipelineConfig::default()).with_detector(
            FnDetector::new(DetectorKind::Model, "m", move |t, _| {
                c2.lock().unwrap().push(crate::chunk::word_count(t));
                Vec::new()
            })
            .with_max_words(300),
        );
        let lines: Vec<String> = (0..20).map(|i| format!("Zeile {i} mit fünf Wörtern")).collect();
        let texts: Vec<&str> = lines.iter().map(String::as_str).collect();
        p.detect_many(&texts).unwrap();
        let sizes = calls.lock().unwrap().clone();
        assert!(sizes.iter().all(|&n| n <= PACK_MAX_WORDS), "{sizes:?}");
        assert_eq!(sizes.iter().sum::<usize>(), 100);
        assert!(sizes.len() <= 3, "{sizes:?}");
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
