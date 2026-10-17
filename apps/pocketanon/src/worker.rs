//! Background work: every slow call runs on a `std::thread` and reports back over `mpsc`.
//! The UI polls [`Jobs::poll`] each frame and never blocks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use pocketanon_core::{
    AnonymizableDocument, Candidate, DetectRequest, Detector, DetectorError, DetectorKind, DocumentError, Format,
    PageImage, Pipeline, PipelineConfig, Placement, Segment, SegmentEdits, SegmentId, Span, Summary,
};

use crate::services::{DocumentLoader, DownloadProgress, ModelManager};

/// A detector shared between the UI state and worker pipelines.
pub type SharedDetector = Arc<dyn Detector>;

/// Adapter so an `Arc<dyn Detector>` can go into a [`Pipeline`] (which owns boxes).
struct ArcDetector(SharedDetector);

impl Detector for ArcDetector {
    fn kind(&self) -> DetectorKind {
        self.0.kind()
    }
    fn name(&self) -> String {
        self.0.name()
    }
    fn max_words(&self) -> Option<usize> {
        self.0.max_words()
    }
    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        self.0.detect(text, req)
    }
}

pub type SharedDoc = Arc<Mutex<Box<dyn AnonymizableDocument>>>;

/// Which workspace a detection run belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Text,
    File,
}

pub enum Msg {
    Detected { target: Target, ticket: u64, result: Result<Vec<(SegmentId, Vec<Candidate>)>, DetectorError> },
    ModelLoaded { label: String, result: Result<SharedDetector, String> },
    Download { id: String, progress: DownloadProgress },
    Downloaded { id: String, result: Result<(), String> },
    DocLoaded { ticket: u64, name: String, path: PathBuf, result: Result<(SharedDoc, Format, Vec<Segment>, usize), String> },
    PageRendered { ticket: u64, page: usize, scale: f32, image: Option<PageImage> },
    Located { ticket: u64, placements: HashMap<(SegmentId, Span), Vec<Placement>> },
    Saved { ticket: u64, result: Result<(PathBuf, Summary, Vec<Segment>), String> },
}

pub struct Jobs {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: Option<egui::Context>,
    /// Number of jobs in flight (for the busy indicator).
    pub running: usize,
}

impl Jobs {
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let (tx, rx) = channel();
        Jobs { tx, rx, ctx, running: 0 }
    }

    pub fn poll(&mut self) -> Vec<Msg> {
        let mut out = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            if !matches!(m, Msg::Download { .. }) {
                self.running = self.running.saturating_sub(1);
            }
            out.push(m);
        }
        out
    }

    pub fn busy(&self) -> bool {
        self.running > 0
    }

    fn spawn(&mut self, name: &str, f: impl FnOnce(&Sender<Msg>) + Send + 'static) {
        self.running += 1;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new().name(format!("pocketanon-{name}")).spawn(move || {
            f(&tx);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
        if let Err(e) = spawned {
            log::error!("could not start worker {name}: {e}");
            self.running -= 1;
        }
    }

    pub fn detect(
        &mut self,
        target: Target,
        ticket: u64,
        detectors: Vec<SharedDetector>,
        config: PipelineConfig,
        segments: Vec<(SegmentId, String)>,
    ) {
        self.spawn("detect", move |tx| {
            let mut p = Pipeline::new(config);
            for d in detectors {
                p.add_detector(Box::new(ArcDetector(d)));
            }
            let result = segments
                .into_iter()
                .filter(|(_, t)| !t.trim().is_empty())
                .map(|(id, t)| p.detect(&t).map(|c| (id, c)))
                .collect();
            let _ = tx.send(Msg::Detected { target, ticket, result });
        });
    }

    pub fn load_model(&mut self, models: Arc<dyn ModelManager>, id: String, label: String) {
        self.spawn("model", move |tx| {
            let result = models.load_detector(&id).map(SharedDetector::from);
            let _ = tx.send(Msg::ModelLoaded { label, result });
        });
    }

    pub fn load_model_folder(&mut self, models: Arc<dyn ModelManager>, folder: PathBuf) {
        self.spawn("model", move |tx| {
            let label = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let result = models.load_from_folder(&folder).map(SharedDetector::from);
            let _ = tx.send(Msg::ModelLoaded { label, result });
        });
    }

    pub fn download(&mut self, models: Arc<dyn ModelManager>, id: String) {
        let ctx = self.ctx.clone();
        self.spawn("download", move |tx| {
            let ptx = tx.clone();
            let pid = id.clone();
            let progress = Box::new(move |p: DownloadProgress| {
                let _ = ptx.send(Msg::Download { id: pid.clone(), progress: p });
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            });
            let result = models.download(&id, progress);
            let _ = tx.send(Msg::Downloaded { id, result });
        });
    }

    pub fn load_document(&mut self, loader: Arc<dyn DocumentLoader>, ticket: u64, path: PathBuf) {
        self.spawn("load", move |tx| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let result = (|| {
                let format = loader
                    .detect_format(&path)
                    .ok_or_else(|| "Unsupported Format. PocketAnon reads TXT, DOCX and RTF.".to_string())?;
                let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the file: {e}"))?;
                let doc = loader.load(&bytes, format).map_err(|e| e.to_string())?;
                let segments = doc.segments();
                let pages = doc.page_count();
                Ok((Arc::new(Mutex::new(doc)) as SharedDoc, format, segments, pages))
            })();
            let _ = tx.send(Msg::DocLoaded { ticket, name, path, result });
        });
    }

    pub fn render_page(&mut self, doc: SharedDoc, ticket: u64, page: usize, scale: f32) {
        self.spawn("render", move |tx| {
            let image = doc.lock().ok().and_then(|d| d.render_page(page, scale));
            let _ = tx.send(Msg::PageRendered { ticket, page, scale, image });
        });
    }

    /// Placements at scale 1.0 for every span; the UI multiplies by its zoom.
    pub fn locate(&mut self, doc: SharedDoc, ticket: u64, spans: Vec<(SegmentId, Span)>) {
        self.spawn("locate", move |tx| {
            let mut placements = HashMap::new();
            if let Ok(d) = doc.lock() {
                for (seg, span) in spans {
                    let p = d.locate(seg, span, 1.0);
                    if !p.is_empty() {
                        placements.insert((seg, span), p);
                    }
                }
            }
            let _ = tx.send(Msg::Located { ticket, placements });
        });
    }

    pub fn apply_and_save(&mut self, doc: SharedDoc, ticket: u64, edits: Vec<SegmentEdits>, summary: Summary, out: PathBuf) {
        self.spawn("save", move |tx| {
            let result = (|| {
                let mut d = doc.lock().map_err(|_| "document lock poisoned".to_string())?;
                d.apply(&edits).map_err(|e: DocumentError| e.to_string())?;
                let bytes = d.save().map_err(|e| e.to_string())?;
                std::fs::write(&out, bytes).map_err(|e| format!("Could not write {}: {e}", out.display()))?;
                Ok((out, summary, d.segments()))
            })();
            let _ = tx.send(Msg::Saved { ticket, result });
        });
    }
}
