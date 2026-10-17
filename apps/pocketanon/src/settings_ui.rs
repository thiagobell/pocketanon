//! The right-side settings drawer.

use egui::{CornerRadius, Margin, RichText, ScrollArea, Stroke, Ui};

use crate::app::PocketAnon;
use crate::services::ModelStatus;
use crate::settings::ModelChoice;
use crate::theme::{Theme, Tokens};
use crate::widgets::{caption, card, chip, ghost_button, segmented};

pub fn drawer(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    egui::Frame::new().inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings").size(16.0).strong().color(tk.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗙").on_hover_text("Close").clicked() {
                    app.settings_open = false;
                }
            });
        });
        ui.label(RichText::new("Settings are saved on this device. Content never is.").size(11.5).color(tk.text_faint));
        ui.add_space(10.0);
        ScrollArea::vertical().id_salt("settings_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let mut changed = false;
            section(ui, tk, "Model", |ui| model_section(app, ui, tk));
            section(ui, tk, "Threshold", |ui| {
                let mut t = app.settings.threshold;
                if ui.add(egui::Slider::new(&mut t, 0.0..=1.0).step_by(0.01).fixed_decimals(2)).changed() {
                    app.set_threshold(t);
                }
                ui.label(RichText::new("Candidates below this Confidence are hidden from Review. Entity Types, Languages and Term Rules live in the Detection panel.").size(11.5).color(tk.text_faint));
            });
            section(ui, tk, "Appearance", |ui| {
                let mut t = app.settings.theme;
                if segmented(ui, tk, &mut t, &[(Theme::Dark, "🌙  Dark"), (Theme::Light, "☀  Light")]) {
                    app.settings.theme = t;
                    changed = true;
                }
            });
            if changed {
                app.settings_changed();
            }
        });
    });
}

fn section(ui: &mut Ui, tk: &Tokens, title: &str, body: impl FnOnce(&mut Ui)) {
    card(tk).show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, tk, title);
        ui.add_space(4.0);
        body(ui);
    });
    ui.add_space(10.0);
}

fn model_section(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let models = app.services.models.models();
    let current = app.settings.model.clone();
    for m in &models {
        let status = app.services.models.status(&m.id);
        let progress = app.model.downloads.get(&m.id).copied();
        let selected = current == Some(ModelChoice::Registry(m.id.clone()));
        egui::Frame::new()
            .fill(if selected { tk.accent_soft } else { tk.input })
            .stroke(Stroke::new(1.0, if selected { tk.accent } else { tk.border }))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&m.display_name).strong().color(tk.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, c) = match (status, progress.is_some()) {
                            (_, true) | (ModelStatus::Downloading, _) => ("downloading", tk.warn),
                            (ModelStatus::Installed, _) => ("installed", tk.ok),
                            (ModelStatus::Missing, _) => ("not downloaded", tk.text_faint),
                        };
                        chip(ui, label, c, tk.dark);
                    });
                });
                let langs: Vec<String> = m.languages.iter().map(|l| l.code().to_uppercase()).collect();
                let size = if m.size_bytes > 0 { format!(" · {:.0} MB", m.size_bytes as f64 / 1e6) } else { String::new() };
                ui.label(RichText::new(format!("{}{size}", langs.join(" "))).size(11.0).color(tk.text_faint));
                if let Some(p) = progress {
                    let frac = p.total.map_or(0.0, |t| if t == 0 { 0.0 } else { p.received as f32 / t as f32 });
                    ui.add(egui::ProgressBar::new(frac).desired_height(6.0).fill(tk.accent).corner_radius(3.0));
                    let total = p.total.map_or_else(|| "?".to_string(), |t| format!("{:.1}", t as f64 / 1e6));
                    ui.label(RichText::new(format!("{:.1} / {total} MB", p.received as f64 / 1e6)).size(11.0).color(tk.text_dim));
                } else {
                    ui.horizontal(|ui| match status {
                        ModelStatus::Installed => {
                            let active = selected && app.model.detector.is_some();
                            let text = if active { "In use" } else { "Use" };
                            if ghost_button(ui, tk, text, !active && !app.model.loading).clicked() {
                                app.use_model(&m.id);
                            }
                        }
                        ModelStatus::Missing => {
                            if ghost_button(ui, tk, "⬇  Download", true).clicked() {
                                app.download_model(&m.id);
                            }
                        }
                        ModelStatus::Downloading => {
                            ui.add(egui::Spinner::new().size(12.0));
                        }
                    });
                }
            });
        ui.add_space(4.0);
    }
    if let Some(ModelChoice::Folder(p)) = &current {
        ui.label(RichText::new(format!("Local folder: {}", p.display())).size(11.5).color(tk.text_dim));
    }
    if ghost_button(ui, tk, "📁  Use local folder…", !app.model.loading).clicked()
        && let Some(folder) = rfd::FileDialog::new().pick_folder()
    {
        app.use_model_folder(folder);
    }
}
