//! The [`Detector`] trait implemented by the model detector and the rule detector.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::types::{Candidate, EntityType, Language};

/// Which kind of detector produced a candidate. Drives merging priority and UI labelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    /// GLiNER or another learned model.
    Model,
    /// Patterns and checksums.
    Rule,
    /// Added by the user during review.
    Manual,
}

impl fmt::Display for DetectorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DetectorKind::Model => "model",
            DetectorKind::Rule => "rule",
            DetectorKind::Manual => "manual",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DetectorError {
    #[error("detector {detector} failed: {message}")]
    Failed { detector: String, message: String },
    #[error("detector {detector} is not ready: {message}")]
    NotReady { detector: String, message: String },
}

impl DetectorError {
    pub fn failed(detector: impl Into<String>, message: impl fmt::Display) -> Self {
        DetectorError::Failed { detector: detector.into(), message: message.to_string() }
    }
    pub fn not_ready(detector: impl Into<String>, message: impl fmt::Display) -> Self {
        DetectorError::NotReady { detector: detector.into(), message: message.to_string() }
    }
}

/// What a detector is asked to look for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectRequest<'a> {
    /// Active entity types. A detector may ignore types it cannot produce.
    pub types: &'a [EntityType],
    /// Languages the user expects. Rule detectors pick patterns by language; model detectors may
    /// ignore this.
    pub languages: &'a [Language],
}

/// A source of [`Candidate`]s.
///
/// Implementations receive one chunk of text at a time (see [`crate::chunk`]) and return
/// candidates whose spans are byte offsets **into that chunk**. The pipeline shifts them onto the
/// whole input. Implementations must be `Send + Sync` so the app can run them off the UI thread.
pub trait Detector: Send + Sync {
    fn kind(&self) -> DetectorKind;

    /// Human-readable name for errors and the UI, e.g. `"gliner_multi_pii-v1"` or `"rules"`.
    fn name(&self) -> String;

    /// Upper bound on the number of *words* (whitespace-separated) one call may receive, if any.
    /// The pipeline chunks inputs so that no chunk exceeds the smallest limit over all detectors.
    fn max_words(&self) -> Option<usize> {
        None
    }

    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError>;

    /// Detect over several texts at once; the result has one entry per input, in order.
    /// Model detectors override this to run one inference over the whole batch, which is much
    /// faster than one call per paragraph. The default loops over [`Detector::detect`].
    fn detect_batch(&self, texts: &[&str], req: &DetectRequest<'_>) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        texts.iter().map(|t| self.detect(t, req)).collect()
    }
}

/// A detector backed by a closure; handy in tests and for simple rule sets.
pub struct FnDetector<F> {
    kind: DetectorKind,
    name: String,
    max_words: Option<usize>,
    f: F,
}

impl<F> FnDetector<F>
where
    F: Fn(&str, &DetectRequest<'_>) -> Vec<Candidate> + Send + Sync,
{
    pub fn new(kind: DetectorKind, name: impl Into<String>, f: F) -> Self {
        FnDetector { kind, name: name.into(), max_words: None, f }
    }

    pub fn with_max_words(mut self, n: usize) -> Self {
        self.max_words = Some(n);
        self
    }
}

impl<F> Detector for FnDetector<F>
where
    F: Fn(&str, &DetectRequest<'_>) -> Vec<Candidate> + Send + Sync,
{
    fn kind(&self) -> DetectorKind {
        self.kind
    }
    fn name(&self) -> String {
        self.name.clone()
    }
    fn max_words(&self) -> Option<usize> {
        self.max_words
    }
    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        Ok((self.f)(text, req))
    }
}
