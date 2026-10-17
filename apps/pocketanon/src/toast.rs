//! A tiny toast stack, bottom-right.

use std::time::{Duration, Instant};

use egui::{Align2, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// A button on a toast; the app performs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToastAction {
    /// Show this file in the Finder (macOS).
    Reveal(std::path::PathBuf),
}

impl ToastAction {
    fn label(&self) -> &'static str {
        match self {
            ToastAction::Reveal(_) => "Reveal in Finder",
        }
    }

    /// Perform the action.
    pub fn run(&self) {
        match self {
            ToastAction::Reveal(path) => reveal(path),
        }
    }
}

#[cfg(target_os = "macos")]
fn reveal(path: &std::path::Path) {
    if let Err(e) = std::process::Command::new("open").arg("-R").arg(path).spawn() {
        log::warn!("could not reveal {}: {e}", path.display());
    }
}

#[cfg(not(target_os = "macos"))]
fn reveal(_path: &std::path::Path) {}

struct Toast {
    action: Option<ToastAction>,
    kind: ToastKind,
    text: String,
    born: Instant,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

const LIFETIME: Duration = Duration::from_secs(5);

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text = text.into();
        match kind {
            ToastKind::Error => log::warn!("{text}"),
            _ => log::info!("{text}"),
        }
        self.items.push(Toast { action: None, kind, text, born: Instant::now() });
        if self.items.len() > 5 {
            self.items.remove(0);
        }
    }

    /// A success toast with a button; stays twice as long.
    pub fn success_with(&mut self, t: impl Into<String>, action: ToastAction) {
        self.push(ToastKind::Success, t);
        if let Some(last) = self.items.last_mut() {
            last.action = Some(action);
        }
    }

    pub fn info(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Info, t);
    }
    pub fn success(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Success, t);
    }
    pub fn error(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Error, t);
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.items.retain(|t| t.born.elapsed() < if t.action.is_some() { LIFETIME * 2 } else { LIFETIME });
        if self.items.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let tk = Tokens::current(ctx);
        let mut dismiss = None;
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                for (i, t) in self.items.iter().enumerate() {
                    let (dot, label) = match t.kind {
                        ToastKind::Info => (tk.accent, "Info"),
                        ToastKind::Success => (tk.ok, "Done"),
                        ToastKind::Error => (tk.danger, "Error"),
                    };
                    let r = Frame::new()
                        .fill(tk.menu)
                        .stroke(Stroke::new(1.0, tk.border_strong))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(12, 10))
                        .shadow(ctx.global_style().visuals.popup_shadow)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                                ui.painter().circle_filled(rect.center(), 4.0, dot);
                                ui.label(RichText::new(label).strong().color(tk.text));
                                ui.add(egui::Label::new(RichText::new(&t.text).color(tk.text_dim)).wrap());
                            });
                            if let Some(a) = &t.action
                                && ui.button(a.label()).clicked()
                            {
                                a.run();
                            }
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        dismiss = Some(i);
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some(i) = dismiss {
            self.items.remove(i);
        }
    }
}
