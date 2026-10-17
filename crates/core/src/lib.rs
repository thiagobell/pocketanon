//! PocketAnon domain core.
//!
//! Vocabulary follows `CONTEXT.md` at the repository root:
//!
//! - a [`Detector`] proposes [`Candidate`]s for a text;
//! - [`merge`] resolves overlapping candidates from several detectors;
//! - [`chunk`] splits long inputs for detectors with an input ceiling and maps offsets back;
//! - [`Review`](review::Review) turns candidates into accepted [`Entity`]s;
//! - a [`Strategy`](replace::Strategy) decides the replacement text, [`replace`] applies it and
//!   produces a [`Summary`].
//!
//! All offsets in this crate are **byte offsets** into the exact `&str` that was analysed, and
//! always fall on `char` boundaries. Nothing here performs I/O or knows about files or models.

pub mod catalog;
pub mod chunk;
pub mod detector;
pub mod document;
pub mod merge;
pub mod pipeline;
pub mod replace;
pub mod review;
pub mod summary;
pub mod types;

pub use catalog::{TypeCatalog, TypeGroup};
pub use chunk::{Chunk, ChunkPlan};
pub use detector::{DetectRequest, Detector, DetectorError, DetectorKind, FnDetector};
pub use document::{AnonymizableDocument, DocumentError, Format, PageImage, Placement, Rect, Segment, SegmentEdits, SegmentId, SegmentKind};
pub use merge::merge;
pub use pipeline::{Pipeline, PipelineConfig};
pub use replace::{Indexer, Replacement, Strategy, apply, apply_replacements, plan_replacements};
pub use review::{CandidateId, ReviewItem};
pub use review::Review;
pub use summary::Summary;
pub use types::{Candidate, Entity, EntityType, Language, Span};
