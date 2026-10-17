//! The application state, chrome (rail, toolbar, drawer, palette) and message handling.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText, Stroke, Ui};
use pocketanon_core::{
    DetectorError, EntityType, Format, PipelineConfig, Placement, Review, Segment, SegmentEdits, SegmentId, SegmentKind,
    Span, Strategy, Summary, TypeCatalog,
};

use crate::palette::{Action, Palette};
use crate::review_ui::{Hovered, SegmentReview, plan_all, set_threshold};
use crate::services::{DocumentLoader, DownloadProgress, ModelManager, ModelStatus, Services};
use crate::settings::{ModelChoice, Settings, SettingsStore};
use crate::theme::{self, Theme, Tokens};
use crate::toast::Toasts;
use crate::worker::{Jobs, Msg, SharedDetector, SharedDoc, Target};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workspace {
    Text,
    File,
}

impl Workspace {
    pub fn title(self) -> &'static str {
        match self {
            Workspace::Text => "Text Anonymization",
            Workspace::File => "File Anonymization",
        }
    }
}

/// The friendly "no Model" notice shown above the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Banner {
    /// Detection ran without a Model Detector (rules only, or nothing at all).
    NoModel { ran_rules: bool },
    /// A detector reported `NotReady`.
    NotReady(String),
}

pub struct AppServices {
    /// Always-on detectors (Rule Detector).
    pub base: Vec<SharedDetector>,
    pub documents: Arc<dyn DocumentLoader>,
    pub models: Arc<dyn ModelManager>,
}

#[derive(Default)]
pub struct ModelState {
    pub detector: Option<SharedDetector>,
    pub label: Option<String>,
    pub loading: bool,
    pub downloads: HashMap<String, DownloadProgress>,
}

pub struct TextWs {
    pub input: String,
    pub reviews: Option<Vec<SegmentReview>>,
    pub output: String,
    pub summary: Option<Summary>,
    pub detecting: Option<u64>,
    /// Last non-empty selection in the input, as a byte span.
    pub selection: Option<Span>,
    pub manual_type: EntityType,
}

impl Default for TextWs {
    fn default() -> Self {
        TextWs {
            input: String::new(),
            reviews: None,
            output: String::new(),
            summary: None,
            detecting: None,
            selection: None,
            manual_type: EntityType::new("person"),
        }
    }
}

pub struct PageTex {
    pub scale: f32,
    pub texture: egui::TextureHandle,
    /// Page size at scale 1.0.
    pub size: egui::Vec2,
}

pub struct FileWs {
    pub path: Option<PathBuf>,
    pub name: String,
    pub format: Option<Format>,
    pub doc: Option<SharedDoc>,
    pub segments: Vec<Segment>,
    pub page_count: usize,
    pub reviews: Option<Vec<SegmentReview>>,
    pub summary: Option<Summary>,
    pub saved_to: Option<PathBuf>,
    /// Generation of the loaded document; stale worker results are ignored.
    pub ticket: u64,
    pub loading: bool,
    pub detecting: Option<u64>,
    pub saving: bool,
    pub zoom: f32,
    pub pages: HashMap<usize, PageTex>,
    pub pending_pages: HashSet<usize>,
    pub placements: HashMap<(SegmentId, Span), Vec<Placement>>,
}

impl Default for FileWs {
    fn default() -> Self {
        FileWs {
            path: None,
            name: String::new(),
            format: None,
            doc: None,
            segments: Vec::new(),
            page_count: 0,
            reviews: None,
            summary: None,
            saved_to: None,
            ticket: 0,
            loading: false,
            detecting: None,
            saving: false,
            zoom: 1.0,
            pages: HashMap::new(),
            pending_pages: HashSet::new(),
            placements: HashMap::new(),
        }
    }
}

pub struct PocketAnon {
    pub services: AppServices,
    pub catalog: TypeCatalog,
    pub settings: Settings,
    store: SettingsStore,
    settings_dirty: Option<Instant>,
    pub workspace: Workspace,
    pub settings_open: bool,
    pub palette: Palette,
    pub toasts: Toasts,
    pub jobs: Jobs,
    pub model: ModelState,
    pub banner: Option<Banner>,
    pub text: TextWs,
    pub file: FileWs,
    pub hovered: Hovered,
    next_ticket: u64,
    theme_applied: Option<Theme>,
    pub custom_type_input: String,
    ctx: Option<egui::Context>,
}

impl PocketAnon {
    pub fn new(ctx: Option<egui::Context>, services: Services, store: SettingsStore) -> Self {
        let settings = store.load();
        let services = AppServices {
            base: services.detectors.into_iter().map(SharedDetector::from).collect(),
            documents: Arc::from(services.documents),
            models: Arc::from(services.models),
        };
        let mut app = PocketAnon {
            services,
            catalog: TypeCatalog::builtin(),
            settings,
            store,
            settings_dirty: None,
            workspace: Workspace::Text,
            settings_open: false,
            palette: Palette::default(),
            toasts: Toasts::default(),
            jobs: Jobs::new(ctx.clone()),
            model: ModelState::default(),
            banner: None,
            text: TextWs::default(),
            file: FileWs::default(),
            hovered: None,
            next_ticket: 1,
            theme_applied: None,
            custom_type_input: String::new(),
            ctx,
        };
        app.start_model();
        app
    }

    fn ticket(&mut self) -> u64 {
        self.next_ticket += 1;
        self.next_ticket
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::for_theme(self.settings.theme)
    }

    pub fn settings_changed(&mut self) {
        self.settings_dirty = Some(Instant::now());
    }

    fn flush_settings(&mut self, force: bool) {
        if let Some(t) = self.settings_dirty
            && (force || t.elapsed() > Duration::from_millis(400))
        {
            self.settings_dirty = None;
            if let Err(e) = self.store.save(&self.settings) {
                self.toasts.error(format!("Could not save settings: {e}"));
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Models

    /// Load the chosen Model at start-up; pick the first installed Registry entry when none is chosen.
    fn start_model(&mut self) {
        let choice = self.settings.model.clone().or_else(|| {
            self.services
                .models
                .models()
                .into_iter()
                .find(|m| self.services.models.status(&m.id) == ModelStatus::Installed)
                .map(|m| ModelChoice::Registry(m.id))
        });
        match choice {
            Some(ModelChoice::Registry(id)) if self.services.models.status(&id) == ModelStatus::Installed => self.use_model(&id),
            Some(ModelChoice::Folder(p)) => self.use_model_folder(p),
            _ => {}
        }
    }

    pub fn use_model(&mut self, id: &str) {
        let label = self
            .services
            .models
            .models()
            .into_iter()
            .find(|m| m.id == id)
            .map_or_else(|| id.to_string(), |m| m.display_name);
        self.model.loading = true;
        self.jobs.load_model(self.services.models.clone(), id.to_string(), label);
        if self.settings.model.as_ref() != Some(&ModelChoice::Registry(id.to_string())) {
            self.settings.model = Some(ModelChoice::Registry(id.to_string()));
            self.settings_changed();
        }
    }

    pub fn use_model_folder(&mut self, folder: PathBuf) {
        self.model.loading = true;
        self.jobs.load_model_folder(self.services.models.clone(), folder.clone());
        if self.settings.model.as_ref() != Some(&ModelChoice::Folder(folder.clone())) {
            self.settings.model = Some(ModelChoice::Folder(folder));
            self.settings_changed();
        }
    }

    pub fn download_model(&mut self, id: &str) {
        self.model.downloads.insert(id.to_string(), DownloadProgress { received: 0, total: None });
        self.jobs.download(self.services.models.clone(), id.to_string());
    }

    fn detectors(&self) -> Vec<SharedDetector> {
        let mut d = self.services.base.clone();
        if let Some(m) = &self.model.detector {
            d.push(m.clone());
        }
        d
    }

    fn pipeline_config(&self) -> PipelineConfig {
        PipelineConfig {
            types: self.settings.active_types(&self.catalog),
            languages: self.settings.languages.clone(),
            ..PipelineConfig::default()
        }
    }

    /// Detectors to run, or `None` (with the banner set) when there are none at all.
    fn ready_detectors(&mut self) -> Option<Vec<SharedDetector>> {
        if self.model.detector.is_none() {
            self.banner = Some(Banner::NoModel { ran_rules: !self.services.base.is_empty() });
        } else {
            self.banner = None;
        }
        let d = self.detectors();
        if d.is_empty() {
            return None;
        }
        Some(d)
    }

    // -----------------------------------------------------------------------------------------
    // Actions

    pub fn run_action(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Run => self.detect(),
            Action::Apply => match self.workspace {
                Workspace::Text => self.apply_text(),
                Workspace::File => self.apply_and_save_file(),
            },
            Action::CopyResult => self.copy_result(ctx),
            Action::OpenFile => {
                self.workspace = Workspace::File;
                self.open_file_dialog();
            }
            Action::SaveResult => match self.workspace {
                Workspace::Text => self.save_text_result(),
                Workspace::File => self.apply_and_save_file(),
            },
            Action::ToggleStrategy => {
                self.settings.strategy = match self.settings.strategy {
                    Strategy::IndexedRedaction => Strategy::Redaction,
                    Strategy::Redaction => Strategy::IndexedRedaction,
                };
                self.settings_changed();
                self.toasts.info(format!("Replacement Strategy: {}", strategy_label(self.settings.strategy)));
            }
            Action::OpenSettings => self.settings_open = !self.settings_open,
            Action::SwitchWorkspace => {
                self.workspace = match self.workspace {
                    Workspace::Text => Workspace::File,
                    Workspace::File => Workspace::Text,
                }
            }
            Action::Clear => self.clear(),
        }
    }

    pub fn detect(&mut self) {
        match self.workspace {
            Workspace::Text => self.detect_text(),
            Workspace::File => self.detect_file(),
        }
    }

    pub fn detect_text(&mut self) {
        if self.text.input.trim().is_empty() {
            self.toasts.info("Type or paste some text first.");
            return;
        }
        let Some(detectors) = self.ready_detectors() else { return };
        let ticket = self.ticket();
        self.text.detecting = Some(ticket);
        self.text.summary = None;
        self.text.output.clear();
        let config = self.pipeline_config();
        self.jobs.detect(Target::Text, ticket, detectors, config, vec![(SegmentId(0), self.text.input.clone())]);
    }

    pub fn detect_file(&mut self) {
        if self.file.doc.is_none() {
            self.toasts.info("Open a document first.");
            return;
        }
        let Some(detectors) = self.ready_detectors() else { return };
        let ticket = self.ticket();
        self.file.detecting = Some(ticket);
        self.file.summary = None;
        self.file.saved_to = None;
        let config = self.pipeline_config();
        let segs = self.file.segments.iter().map(|s| (s.id, s.text.clone())).collect();
        self.jobs.detect(Target::File, ticket, detectors, config, segs);
    }

    /// Text Anonymization: apply the Replacement Strategy to the accepted Entities.
    pub fn apply_text(&mut self) {
        let Some(reviews) = &self.text.reviews else {
            self.toasts.info("Run detection first.");
            return;
        };
        let (plan, summary) = plan_all(reviews, self.settings.strategy);
        let seg = &reviews[0];
        let reps = plan.into_iter().find(|(id, _)| *id == seg.id).map(|(_, r)| r).unwrap_or_default();
        self.text.output = pocketanon_core::apply_replacements(&seg.text, &reps);
        self.text.summary = Some(summary);
    }

    pub fn copy_result(&mut self, ctx: &egui::Context) {
        let text = match self.workspace {
            Workspace::Text => self.text.output.clone(),
            Workspace::File => String::new(),
        };
        if text.is_empty() {
            self.toasts.info("Nothing to copy yet. Apply first.");
            return;
        }
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())) {
            Ok(()) => {}
            Err(_) => ctx.copy_text(text),
        }
        self.toasts.success("Result copied to the clipboard.");
    }

    fn save_text_result(&mut self) {
        if self.text.output.is_empty() {
            self.toasts.info("Apply first, then save the result.");
            return;
        }
        if let Some(path) = rfd::FileDialog::new().set_file_name("anonymized.txt").add_filter("Text", &["txt"]).save_file() {
            match std::fs::write(&path, self.text.output.as_bytes()) {
                Ok(()) => self.toasts.success(format!("Saved {}", path.display())),
                Err(e) => self.toasts.error(format!("Could not save: {e}")),
            }
        }
    }

    pub fn clear(&mut self) {
        match self.workspace {
            Workspace::Text => self.text = TextWs::default(),
            Workspace::File => self.file = FileWs { zoom: self.file.zoom, ..FileWs::default() },
        }
        self.banner = None;
        self.hovered = None;
    }

    pub fn open_file_dialog(&mut self) {
        let exts: Vec<&str> = Format::all().iter().map(|f| f.extension()).collect();
        if let Some(path) = rfd::FileDialog::new().add_filter("Documents", &exts).pick_file() {
            self.open_path(path);
        }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        if self.services.documents.detect_format(&path).is_none() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("?").to_uppercase();
            self.toasts.error(format!("{ext} is not supported by this build. Supported: TXT, DOCX, RTF."));
            return;
        }
        let ticket = self.ticket();
        self.file = FileWs { zoom: self.file.zoom, ticket, loading: true, ..FileWs::default() };
        self.workspace = Workspace::File;
        self.jobs.load_document(self.services.documents.clone(), ticket, path);
    }

    /// File Anonymization: plan with one shared Indexer, confirm the output path, then apply and
    /// save on a worker.
    pub fn apply_and_save_file(&mut self) {
        let (Some(doc), Some(reviews), Some(input)) = (self.file.doc.clone(), &self.file.reviews, self.file.path.clone()) else {
            self.toasts.info("Open a document and run detection first.");
            return;
        };
        let (plan, summary) = plan_all(reviews, self.settings.strategy);
        let default = self.services.documents.output_path(&input);
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = default.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = default.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        if let Some(f) = self.file.format {
            dialog = dialog.add_filter(f.to_string(), &[f.extension()]);
        }
        let Some(out) = dialog.save_file() else { return };
        let same = match (std::fs::canonicalize(&input), std::fs::canonicalize(&out)) {
            (Ok(a), Ok(b)) => a == b,
            _ => input == out,
        };
        if same {
            self.toasts.error("PocketAnon never overwrites the input. Choose a different name.");
            return;
        }
        let edits = plan.into_iter().map(|(segment, replacements)| SegmentEdits { segment, replacements }).collect();
        self.file.saving = true;
        let ticket = self.file.ticket;
        self.jobs.apply_and_save(doc, ticket, edits, summary, out);
    }

    // -----------------------------------------------------------------------------------------
    // Messages

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Detected { target, ticket, result } => {
                let pending = match target {
                    Target::Text => &mut self.text.detecting,
                    Target::File => &mut self.file.detecting,
                };
                if *pending != Some(ticket) {
                    return; // stale
                }
                *pending = None;
                let segments: Vec<(SegmentId, SegmentKind, String)> = match target {
                    Target::Text => vec![(SegmentId(0), SegmentKind::Body, self.text.input.clone())],
                    Target::File => self.file.segments.iter().map(|s| (s.id, s.kind, s.text.clone())).collect(),
                };
                match result {
                    Ok(found) => {
                        let mut by_id: HashMap<SegmentId, Vec<_>> = found.into_iter().collect();
                        let threshold = self.settings.threshold;
                        let reviews: Vec<SegmentReview> = segments
                            .into_iter()
                            .map(|(id, kind, text)| SegmentReview {
                                id,
                                kind,
                                text,
                                review: Review::new(by_id.remove(&id).unwrap_or_default(), threshold),
                            })
                            .collect();
                        let n: usize = reviews.iter().map(|r| r.review.items().len()).sum();
                        match target {
                            Target::Text => self.text.reviews = Some(reviews),
                            Target::File => self.file.reviews = Some(reviews),
                        }
                        if n == 0 {
                            self.toasts.info("No Candidates found.");
                        }
                        if target == Target::File {
                            self.request_placements();
                        }
                    }
                    Err(DetectorError::NotReady { detector, message }) => {
                        self.banner = Some(Banner::NotReady(format!("{detector}: {message}")));
                    }
                    Err(e) => self.toasts.error(format!("Detection failed: {e}")),
                }
            }
            Msg::ModelLoaded { label, result } => {
                self.model.loading = false;
                match result {
                    Ok(d) => {
                        self.model.detector = Some(d);
                        self.model.label = Some(label.clone());
                        if matches!(self.banner, Some(Banner::NoModel { .. }) | Some(Banner::NotReady(_))) {
                            self.banner = None;
                        }
                        self.toasts.success(format!("Model ready: {label}"));
                    }
                    Err(e) => self.toasts.error(format!("Could not load the Model: {e}")),
                }
            }
            Msg::Download { id, progress } => {
                self.model.downloads.insert(id, progress);
            }
            Msg::Downloaded { id, result } => {
                self.model.downloads.remove(&id);
                match result {
                    Ok(()) => {
                        self.toasts.success("Model downloaded.");
                        self.use_model(&id);
                    }
                    Err(e) => self.toasts.error(format!("Download failed: {e}")),
                }
            }
            Msg::DocLoaded { ticket, name, path, result } => {
                if ticket != self.file.ticket {
                    return;
                }
                self.file.loading = false;
                match result {
                    Ok((doc, format, segments, pages)) => {
                        self.file.doc = Some(doc);
                        self.file.format = Some(format);
                        self.file.segments = segments;
                        self.file.page_count = pages;
                        self.file.name = name;
                        self.file.path = Some(path);
                    }
                    Err(e) => self.toasts.error(e),
                }
            }
            Msg::PageRendered { ticket, page, scale, image } => {
                if ticket != self.file.ticket {
                    return;
                }
                self.file.pending_pages.remove(&page);
                if let Some(img) = image {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
                    if let Some(ctx) = self.jobs_ctx() {
                        let texture = ctx.load_texture(format!("page-{ticket}-{page}"), ci, egui::TextureOptions::LINEAR);
                        let size = egui::vec2(img.width as f32 / scale, img.height as f32 / scale);
                        self.file.pages.insert(page, PageTex { scale, texture, size });
                    }
                }
            }
            Msg::Located { ticket, placements } => {
                if ticket == self.file.ticket {
                    self.file.placements = placements;
                }
            }
            Msg::Saved { ticket, result } => {
                if ticket != self.file.ticket {
                    return;
                }
                self.file.saving = false;
                match result {
                    Ok((out, summary, segments)) => {
                        self.toasts.success(format!("Saved {}", out.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
                        self.file.summary = Some(summary);
                        self.file.saved_to = Some(out);
                        self.file.segments = segments;
                        self.file.reviews = None;
                        // The document now holds the Result: re-render the preview.
                        let t = self.ticket();
                        self.file.ticket = t;
                        self.file.pages.clear();
                        self.file.pending_pages.clear();
                        self.file.placements.clear();
                    }
                    Err(e) => self.toasts.error(e),
                }
            }
        }
    }

    fn jobs_ctx(&self) -> Option<egui::Context> {
        self.ctx.clone()
    }

    pub fn request_placements(&mut self) {
        if self.file.page_count == 0 {
            return;
        }
        if let (Some(doc), Some(reviews)) = (self.file.doc.clone(), &self.file.reviews) {
            let spans = crate::review_ui::all_spans(reviews);
            self.jobs.locate(doc, self.file.ticket, spans);
        }
    }

    pub fn set_threshold(&mut self, t: f32) {
        self.settings.threshold = t;
        if let Some(r) = &mut self.text.reviews {
            set_threshold(r, t);
        }
        if let Some(r) = &mut self.file.reviews {
            set_threshold(r, t);
        }
        self.settings_changed();
    }

    // -----------------------------------------------------------------------------------------
    // Frame

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        let mut actions = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_shortcut(&cmd(Key::K)) {
                self.palette.toggle();
            }
            if i.consume_shortcut(&cmd_shift(Key::Enter)) {
                actions.push(Action::Apply);
            }
            if i.consume_shortcut(&cmd(Key::Enter)) {
                actions.push(Action::Run);
            }
            if i.consume_shortcut(&cmd(Key::O)) {
                actions.push(Action::OpenFile);
            }
            if i.consume_shortcut(&cmd(Key::S)) {
                actions.push(Action::SaveResult);
            }
            if i.consume_shortcut(&cmd(Key::Comma)) {
                actions.push(Action::OpenSettings);
            }
            if i.consume_shortcut(&cmd(Key::Num1)) {
                self.workspace = Workspace::Text;
            }
            if i.consume_shortcut(&cmd(Key::Num2)) {
                self.workspace = Workspace::File;
            }
        });
        for a in actions {
            self.run_action(ctx, a);
        }
    }

    /// The whole UI; separate from `eframe::App` so headless tests can drive it.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
        }
        if self.theme_applied != Some(self.settings.theme) {
            theme::apply(&ctx, self.settings.theme);
            self.theme_applied = Some(self.settings.theme);
        }
        for m in self.jobs.poll() {
            self.handle(m);
        }
        if self.jobs.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.shortcuts(&ctx);

        // Files dropped anywhere go to File Anonymization.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.open_path(p);
        }

        let tk = self.tokens();
        self.rail(ui, &tk);
        self.toolbar(ui, &tk);
        if self.settings_open {
            egui::Panel::right("settings_drawer")
                .exact_size(348.0)
                .resizable(false)
                .frame(Frame::new().fill(tk.panel).inner_margin(Margin::same(0)).stroke(Stroke::new(1.0, tk.border)))
                .show(ui, |ui| crate::settings_ui::drawer(self, ui, &tk));
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(tk.canvas).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                if let Some(b) = self.banner.clone() {
                    self.banner_ui(ui, &tk, &b);
                    ui.add_space(10.0);
                }
                match self.workspace {
                    Workspace::Text => crate::text_ws::show(self, ui, &tk),
                    Workspace::File => crate::file_ws::show(self, ui, &tk),
                }
            });

        if let Some(a) = self.palette.show(&ctx) {
            self.run_action(&ctx, a);
        }
        self.toasts.show(&ctx);
        self.flush_settings(false);
        if self.settings_dirty.is_some() {
            ctx.request_repaint_after(Duration::from_millis(450));
        }
    }

    fn rail(&mut self, ui: &mut Ui, tk: &Tokens) {
        egui::Panel::left("rail")
            .exact_size(72.0)
            .resizable(false)
            .frame(Frame::new().fill(tk.rail).inner_margin(Margin::symmetric(8, 12)).stroke(Stroke::new(1.0, tk.border)))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // Logo mark.
                    let (r, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(10), tk.accent);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "PA", egui::FontId::proportional(15.0), tk.on_accent);
                    ui.add_space(18.0);
                    for (ws, icon, label) in [(Workspace::Text, "📝", "Text"), (Workspace::File, "📄", "File")] {
                        if rail_button(ui, tk, icon, label, self.workspace == ws).on_hover_text(ws.title()).clicked() {
                            self.workspace = ws;
                        }
                        ui.add_space(4.0);
                    }
                    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        if rail_button(ui, tk, "⚙", "Settings", self.settings_open).on_hover_text("Settings  ⌘,").clicked() {
                            self.settings_open = !self.settings_open;
                        }
                    });
                });
            });
    }

    fn toolbar(&mut self, ui: &mut Ui, tk: &Tokens) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("toolbar")
            .exact_size(52.0)
            .resizable(false)
            .frame(Frame::new().fill(tk.panel).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, tk.border)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("PocketAnon").size(15.0).strong().color(tk.text));
                    ui.label(RichText::new("/").size(15.0).color(tk.text_faint));
                    ui.label(RichText::new(self.workspace.title()).size(14.0).color(tk.text_dim));
                    ui.add_space(18.0);

                    let mut s = self.settings.strategy;
                    if crate::widgets::segmented(
                        ui,
                        tk,
                        &mut s,
                        &[(Strategy::IndexedRedaction, "[PERSON_1]  Indexed"), (Strategy::Redaction, "[PERSON]  Redaction")],
                    ) {
                        self.settings.strategy = s;
                        self.settings_changed();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let k = egui::Button::new(RichText::new("⌘K  Commands").size(12.0).color(tk.text_dim))
                            .fill(tk.input)
                            .stroke(Stroke::new(1.0, tk.border))
                            .corner_radius(CornerRadius::same(6));
                        if ui.add(k).on_hover_text("Command palette").clicked() {
                            self.palette.toggle();
                        }
                        ui.add_space(6.0);
                        let (apply_label, can_apply) = match self.workspace {
                            Workspace::Text => ("Apply", self.text.reviews.is_some()),
                            Workspace::File => ("Apply & Save…", self.file.reviews.is_some() && !self.file.saving),
                        };
                        if crate::widgets::primary_button(ui, tk, apply_label, can_apply).on_hover_text("⌘ Shift ↩").clicked() {
                            self.run_action(&ctx, Action::Apply);
                        }
                        let detecting = match self.workspace {
                            Workspace::Text => self.text.detecting.is_some(),
                            Workspace::File => self.file.detecting.is_some(),
                        };
                        let can_detect = !detecting
                            && match self.workspace {
                                Workspace::Text => !self.text.input.trim().is_empty(),
                                Workspace::File => self.file.doc.is_some(),
                            };
                        if crate::widgets::ghost_button(ui, tk, "🔍  Detect", can_detect).on_hover_text("⌘↩").clicked() {
                            self.detect();
                        }
                        ui.add_space(8.0);
                        let status = if detecting {
                            Some("Detecting…")
                        } else if self.model.loading {
                            Some("Loading Model…")
                        } else if self.file.loading {
                            Some("Opening…")
                        } else if self.file.saving {
                            Some("Saving…")
                        } else if !self.model.downloads.is_empty() {
                            Some("Downloading…")
                        } else {
                            None
                        };
                        if let Some(s) = status {
                            ui.label(RichText::new(s).size(12.0).color(tk.text_dim));
                            ui.add(egui::Spinner::new().size(14.0).color(tk.accent));
                        } else {
                            let (dot, text) = match &self.model.label {
                                Some(l) => (tk.ok, l.clone()),
                                None => (tk.warn, "No Model".to_string()),
                            };
                            ui.label(RichText::new(text).size(12.0).color(tk.text_dim));
                            let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 3.5, dot);
                        }
                    });
                });
            });
    }

    fn banner_ui(&mut self, ui: &mut Ui, tk: &Tokens, b: &Banner) {
        let text = match b {
            Banner::NoModel { ran_rules: true } => {
                "No Model loaded: only the Rule Detector ran. Pick a Model to find names, places and other free-text Entities."
                    .to_string()
            }
            Banner::NoModel { ran_rules: false } => "No Model loaded, so there is nothing to detect with yet. Pick or download a Model.".to_string(),
            Banner::NotReady(m) => format!("The Model is not ready ({m}). Pick or download a Model."),
        };
        Frame::new()
            .fill(Color32::from_rgba_unmultiplied(tk.warn.r(), tk.warn.g(), tk.warn.b(), 0x22))
            .stroke(Stroke::new(1.0, tk.warn.gamma_multiply(0.6)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠").color(tk.warn));
                    ui.label(RichText::new(text).color(tk.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("🗙").clicked() {
                            self.banner = None;
                        }
                        if ui.button("Open model picker").clicked() {
                            self.settings_open = true;
                        }
                    });
                });
            });
    }
}

fn rail_button(ui: &mut Ui, tk: &Tokens, icon: &str, label: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(56.0, 52.0), egui::Sense::click());
    let bg = if on {
        tk.accent_soft
    } else if resp.hovered() {
        tk.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
    if on {
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left() - 8.0, rect.top() + 12.0), egui::vec2(3.0, rect.height() - 24.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), tk.accent);
    }
    let color = if on { tk.text } else { tk.text_dim };
    ui.painter().text(rect.center() - egui::vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(18.0), color);
    ui.painter().text(rect.center() + egui::vec2(0.0, 13.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(10.5), color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

pub fn strategy_label(s: Strategy) -> &'static str {
    match s {
        Strategy::IndexedRedaction => "Indexed Redaction",
        Strategy::Redaction => "Redaction",
    }
}

impl eframe::App for PocketAnon {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn on_exit(&mut self) {
        self.flush_settings(true);
    }
}
