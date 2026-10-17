//! The right-side settings drawer.

use egui::{CornerRadius, Margin, RichText, ScrollArea, Stroke, Ui};
use pocketanon_core::{EntityType, Strategy};

use crate::app::PocketAnon;
use crate::services::ModelStatus;
use crate::settings::{LANGUAGES, ModelChoice};
use crate::theme::{Theme, Tokens, group_color};
use crate::widgets::{caption, card, chip, ghost_button, segmented, toggle_pill};

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
            section(ui, tk, "Languages", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (code, name) in LANGUAGES {
                        let mut on = app.settings.language_enabled(code);
                        if toggle_pill(ui, tk, &mut on, code.to_uppercase().as_str()).on_hover_text(name).clicked() {
                            app.settings.set_language(code, on);
                            changed = true;
                        }
                    }
                });
            });
            section(ui, tk, "Replacement Strategy", |ui| {
                let mut s = app.settings.strategy;
                if segmented(ui, tk, &mut s, &[(Strategy::IndexedRedaction, "Indexed Redaction"), (Strategy::Redaction, "Redaction")]) {
                    app.settings.strategy = s;
                    changed = true;
                }
                let example = match app.settings.strategy {
                    Strategy::IndexedRedaction => "Max Mustermann becomes [PERSON_1], the same person keeps the same index.",
                    Strategy::Redaction => "Max Mustermann becomes [PERSON], every person looks the same.",
                };
                ui.label(RichText::new(example).size(11.5).color(tk.text_faint));
            });
            section(ui, tk, "Entity Types", |ui| changed |= types_section(app, ui, tk));
            section(ui, tk, "Custom Types", |ui| changed |= custom_section(app, ui, tk));
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

fn types_section(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) -> bool {
    let mut changed = false;
    let groups = app.catalog.groups.clone();
    for g in &groups {
        let c = group_color(Some(&g.name));
        let mut on = app.settings.group_enabled(&g.name);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, c);
            if ui.checkbox(&mut on, RichText::new(&g.name).strong().color(tk.text)).changed() {
                app.settings.set_group_enabled(&g.name, on);
                changed = true;
            }
        });
        if on {
            ui.indent(("types", &g.name), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for t in &g.types {
                        let mut ton = app.settings.type_enabled(t);
                        if ui.checkbox(&mut ton, RichText::new(t.as_str()).size(12.0).color(tk.text_dim)).changed() {
                            app.settings.set_type_enabled(t, ton);
                            changed = true;
                        }
                    }
                });
            });
        }
        ui.add_space(4.0);
    }
    changed
}

fn custom_section(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) -> bool {
    let mut changed = false;
    ui.label(RichText::new("Free-text Entity Types the Model should look for, e.g. “project code name”.").size(11.5).color(tk.text_faint));
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut app.custom_type_input).hint_text("New Custom Type").desired_width(200.0));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ghost_button(ui, tk, "Add", !app.custom_type_input.trim().is_empty()).clicked() || enter)
            && app.settings.add_custom_type(&app.custom_type_input)
        {
            app.custom_type_input.clear();
            changed = true;
        }
    });
    let mut remove: Option<EntityType> = None;
    ui.horizontal_wrapped(|ui| {
        for t in &app.settings.custom_types {
            chip(ui, t.as_str(), group_color(None), tk.dark);
            if ui.small_button("🗙").on_hover_text("Remove").clicked() {
                remove = Some(t.clone());
            }
        }
    });
    if let Some(t) = remove {
        app.settings.custom_types.retain(|c| *c != t);
        changed = true;
    }
    changed
}
