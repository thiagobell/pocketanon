//! The Model Detector: GLiNER (span mode) via `gline-rs` on ONNX Runtime, plus the model
//! [`Registry`] and the [`ModelStore`] that downloads Models into the app's data directory.
//!
//! Only model files are ever written to disk; user content is never persisted (ADR-0003).

mod detector;
mod error;
mod registry;
mod store;

pub use detector::{
    DEFAULT_MAX_WORDS, DEFAULT_THRESHOLD, GlinerDetector, GlinerOptions, MODEL_MAX_WORDS, batch_labels,
    max_words_for_types, spans_to_candidates,
};
pub use error::GlinerError;
pub use registry::{ModelFile, ModelSpec, Quantization, Registry, VariantSpec};
pub use store::{DownloadProgress, ModelStatus, ModelStore, find_onnx_file};
