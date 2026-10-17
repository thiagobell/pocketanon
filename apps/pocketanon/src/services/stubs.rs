//! STUBS FOR UI DEVELOPMENT ONLY.
//!
//! [`PlainTextLoader`] and [`FakeModelManager`] stand in for `crates/documents` and
//! `crates/detect-gliner` until those are wired in by `main.rs`. Swap them by constructing a
//! different [`Services`](super::Services); nothing else in the app refers to these types.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pocketanon_core::{
    AnonymizableDocument, Candidate, Detector, DetectorKind, DocumentError, FnDetector, Format, Language, Segment,
    SegmentEdits, SegmentId, SegmentKind, Span, apply_replacements,
};

use super::{DocumentLoader, DownloadProgress, ModelInfo, ModelManager, ModelStatus};

// ---------------------------------------------------------------------------------------------
// Documents

/// STUB: handles only [`Format::Txt`].
#[derive(Debug, Default)]
pub struct PlainTextLoader;

impl DocumentLoader for PlainTextLoader {
    fn detect_format(&self, path: &Path) -> Option<Format> {
        path.extension().and_then(|e| e.to_str()).and_then(Format::from_extension).filter(|f| *f == Format::Txt)
    }

    fn load(&self, bytes: &[u8], format: Format) -> Result<Box<dyn AnonymizableDocument>, DocumentError> {
        if format != Format::Txt {
            return Err(DocumentError::Unsupported);
        }
        let text = String::from_utf8(bytes.to_vec()).map_err(|e| DocumentError::Read(e.to_string()))?;
        Ok(Box::new(PlainTextDocument::parse(&text)))
    }

    fn output_path(&self, input: &Path) -> PathBuf {
        let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
        let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("txt");
        input.with_file_name(format!("{stem}.anonymized.{ext}"))
    }
}

/// STUB: a TXT document split into paragraphs on blank lines. Saving is exact: the separators
/// between paragraphs are kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainTextDocument {
    /// `(paragraph, separator that follows it)`
    parts: Vec<(String, String)>,
}

impl PlainTextDocument {
    pub fn parse(text: &str) -> Self {
        let mut parts = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find("\n\n") {
            let para = &rest[..i];
            let after = &rest[i..];
            let sep_len = after.len() - after.trim_start_matches('\n').len();
            parts.push((para.to_string(), after[..sep_len].to_string()));
            rest = &after[sep_len..];
        }
        parts.push((rest.to_string(), String::new()));
        PlainTextDocument { parts }
    }

    pub fn text(&self) -> String {
        self.parts.iter().map(|(p, s)| format!("{p}{s}")).collect()
    }
}

impl AnonymizableDocument for PlainTextDocument {
    fn format(&self) -> Format {
        Format::Txt
    }

    fn segments(&self) -> Vec<Segment> {
        self.parts
            .iter()
            .enumerate()
            .map(|(i, (p, _))| Segment { id: SegmentId(i), kind: SegmentKind::Body, text: p.clone() })
            .collect()
    }

    fn apply(&mut self, edits: &[SegmentEdits]) -> Result<(), DocumentError> {
        for e in edits {
            let part = self.parts.get_mut(e.segment.0).ok_or(DocumentError::UnknownSegment(e.segment))?;
            part.0 = apply_replacements(&part.0, &e.replacements);
        }
        Ok(())
    }

    fn save(&self) -> Result<Vec<u8>, DocumentError> {
        Ok(self.text().into_bytes())
    }
}

// ---------------------------------------------------------------------------------------------
// Models

pub const DEMO_MODEL_ID: &str = "demo";

/// STUB: one "Demo (no model)" Registry entry whose detector is a toy heuristic.
#[derive(Debug, Default)]
pub struct FakeModelManager;

impl ModelManager for FakeModelManager {
    fn models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: DEMO_MODEL_ID.into(),
            display_name: "Demo (no model)".into(),
            languages: ["de", "en", "fr", "es", "it", "nl", "pl"].iter().map(Language::new).collect(),
            size_bytes: 0,
        }]
    }

    fn status(&self, _id: &str) -> ModelStatus {
        ModelStatus::Installed
    }

    fn download(&self, _id: &str, progress: Box<dyn Fn(DownloadProgress) + Send>) -> Result<(), String> {
        // Pretend: ten steps over one second, so the progress bar can be seen.
        let total = 10_000_000;
        for i in 0..=10 {
            progress(DownloadProgress { received: total / 10 * i, total: Some(total) });
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn load_detector(&self, id: &str) -> Result<Box<dyn Detector>, String> {
        if id != DEMO_MODEL_ID {
            return Err(format!("unknown model {id}"));
        }
        Ok(Box::new(demo_detector()))
    }
}

/// STUB heuristic: capitalised two-word sequences are `person` (0.6), whitespace-separated
/// words containing `@` are `email` (0.9).
pub fn demo_detector() -> impl Detector {
    FnDetector::new(DetectorKind::Model, "demo-heuristic", |text, _req| {
        let words: Vec<(usize, &str)> = text
            .split_whitespace()
            .map(|w| (w.as_ptr() as usize - text.as_ptr() as usize, w))
            .collect();
        let mut out = Vec::new();
        let trim = |w: &str| w.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
        let mut i = 0;
        while i < words.len() {
            let (start, w) = words[i];
            if w.contains('@') {
                let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '@');
                let s = start + words[i].1.find(w).unwrap_or(0);
                out.push(Candidate::new(Span::new(s, s + w.len()), "email", 0.9, DetectorKind::Model));
                i += 1;
                continue;
            }
            let cap = |w: &str| {
                let mut cs = w.chars();
                cs.next().is_some_and(char::is_uppercase) && cs.next().is_some_and(char::is_lowercase)
            };
            if i + 1 < words.len() && cap(w) && cap(words[i + 1].1) && trim(w) == w.len() {
                let (s2, w2) = words[i + 1];
                out.push(Candidate::new(Span::new(start, s2 + trim(w2)), "person", 0.6, DetectorKind::Model));
                i += 2;
                continue;
            }
            i += 1;
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketanon_core::DetectRequest;

    #[test]
    fn plain_text_round_trips_exactly() {
        let t = "Hallo Max Mustermann,\n\n\nzweiter Absatz.\n\nletzter\n";
        let d = PlainTextDocument::parse(t);
        assert_eq!(d.segments().len(), 3);
        assert_eq!(String::from_utf8(d.save().unwrap()).unwrap(), t);
    }

    #[test]
    fn demo_detector_finds_names_and_emails() {
        let d = demo_detector();
        let text = "Please call Max Mustermann, or max@example.com.";
        let req = DetectRequest { types: &[], languages: &[] };
        let c = d.detect(text, &req).unwrap();
        let found: Vec<_> = c.iter().map(|c| (c.text(text), c.entity_type.as_str())).collect();
        assert_eq!(found, vec![("Max Mustermann", "person"), ("max@example.com", "email")]);
    }
}
