//! The seams between the app and the crates that do the real work.
//!
//! The app depends only on `pocketanon-core` and on the two traits below. `main.rs` builds a
//! [`Services`] value; today it uses the clearly marked stubs in [`stubs`], later the real
//! document adapter (`crates/documents`) and model registry (`crates/detect-gliner`).

use std::path::{Path, PathBuf};

use pocketanon_core::{AnonymizableDocument, Detector, DocumentError, Format, Language};

pub mod real;
#[cfg_attr(not(test), allow(dead_code))]
pub mod stubs;

/// Everything the UI needs from the outside world.
pub struct Services {
    /// Detectors that are always active (e.g. the Rule Detector). The Model Detector is loaded at
    /// runtime through [`ModelManager::load_detector`] and comes on top of these.
    pub detectors: Vec<Box<dyn Detector>>,
    pub documents: Box<dyn DocumentLoader>,
    pub models: Box<dyn ModelManager>,
}

/// Loads [`AnonymizableDocument`]s for File Anonymization.
pub trait DocumentLoader: Send + Sync {
    /// The Format of a file, from its path (extension), or `None` when unsupported.
    fn detect_format(&self, path: &Path) -> Option<Format>;
    /// Parse a document from its bytes.
    fn load(&self, bytes: &[u8], format: Format) -> Result<Box<dyn AnonymizableDocument>, DocumentError>;
    /// Where the Result is written by default: beside the input, under a new name. Never `input`.
    fn output_path(&self, input: &Path) -> PathBuf;
}

/// One Registry entry, as the model picker shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub languages: Vec<Language>,
    pub size_bytes: u64,
}

// `Missing`/`Downloading` are produced by the real ModelManager, not by the stub.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelStatus {
    Installed,
    Missing,
    Downloading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: Option<u64>,
}

/// The Model Registry plus downloader. Every method may be called from a worker thread;
/// `download` and `load_detector` may block for a long time and are never called on the UI thread.
pub trait ModelManager: Send + Sync {
    fn models(&self) -> Vec<ModelInfo>;
    fn status(&self, id: &str) -> ModelStatus;
    fn download(&self, id: &str, progress: Box<dyn Fn(DownloadProgress) + Send>) -> Result<(), String>;
    fn load_detector(&self, id: &str) -> Result<Box<dyn Detector>, String>;
    /// Load a Model from a user-supplied folder, bypassing the Registry.
    fn load_from_folder(&self, folder: &Path) -> Result<Box<dyn Detector>, String> {
        Err(format!("Loading a model from {} is not supported by this build", folder.display()))
    }
}
