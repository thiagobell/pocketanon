//! The Legend: a permanent right-side panel in both workspaces with Languages, every Type Group
//! and its colour, Custom Types and Term Rules. Every change here re-runs detection.

use egui::{CornerRadius, Margin, RichText, ScrollArea, Stroke, Ui};
use pocketanon_core::EntityType;

use crate::app::PocketAnon;
use crate::settings::LANGUAGES;
use crate::theme::{Tokens, group_color, type_color};
use crate::widgets::{caption, ghost_button, toggle_pill};

pub const WIDTH: f32 = 270.0;

pub fn panel(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let mut changed = false;
    egui::Frame::new().inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("What to detect").size(15.0).strong().color(tk.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗙").on_hover_text("Hide this panel  ⌘L").clicked() {
                    app.legend_open = false;
                }
            });
        });
        ui.add_space(6.0);
        ScrollArea::vertical().id_salt("legend_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, tk, "Languages");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (code, name) in LANGUAGES {
                    let mut on = app.settings.language_enabled(code);
                    if toggle_pill(ui, tk, &mut on, code.to_uppercase().as_str()).on_hover_text(name).clicked() {
                        app.settings.set_language(code, on);
                        changed = true;
                    }
                }
            });
            ui.add_space(10.0);
            ui.separator();
            caption(ui, tk, "Entity Types");
            changed |= groups(app, ui, tk);
            ui.add_space(6.0);
            ui.separator();
            caption(ui, tk, "Custom Types");
            changed |= custom_types(app, ui, tk);
            ui.add_space(6.0);
            ui.separator();
            caption(ui, tk, "Term Rules");
            changed |= term_rules(app, ui, tk);
        });
    });
    if changed {
        app.types_changed();
    }
}

fn dot(ui: &mut Ui, c: egui::Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 4.5, c);
}

fn groups(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) -> bool {
    let mut changed = false;
    let groups = app.catalog.groups.clone();
    for g in &groups {
        let c = group_color(Some(&g.name));
        let mut on = app.settings.group_enabled(&g.name);
        ui.horizontal(|ui| {
            dot(ui, c);
            let name_color = if on { tk.text } else { tk.text_faint };
            if ui.checkbox(&mut on, RichText::new(&g.name).strong().color(name_color)).changed() {
                app.settings.set_group_enabled(&g.name, on);
                changed = true;
            }
        });
        ui.indent(("legend_types", &g.name), |ui| {
            ui.add_enabled_ui(on, |ui| {
                for t in &g.types {
                    let mut ton = app.settings.type_enabled(t);
                    let color = if on && ton { c } else { tk.text_faint };
                    if ui.checkbox(&mut ton, RichText::new(t.as_str()).size(12.0).color(color)).changed() {
                        app.settings.set_type_enabled(t, ton);
                        changed = true;
                    }
                }
            });
        });
        ui.add_space(2.0);
    }
    changed
}

fn custom_types(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) -> bool {
    let mut changed = false;
    let c = group_color(None);
    let mut remove: Option<EntityType> = None;
    for t in &app.settings.custom_types {
        ui.horizontal(|ui| {
            dot(ui, c);
            ui.label(RichText::new(t.as_str()).size(12.0).color(c));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗙").on_hover_text("Remove this Custom Type").clicked() {
                    remove = Some(t.clone());
                }
            });
        });
    }
    if let Some(t) = remove {
        app.settings.custom_types.retain(|c| *c != t);
        app.settings.term_rules.retain(|r| r.entity_type != t);
        changed = true;
    }
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut app.custom_type_input).hint_text("e.g. project code name").desired_width(WIDTH - 90.0));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ghost_button(ui, tk, "Add", !app.custom_type_input.trim().is_empty()).clicked() || enter)
            && app.settings.add_custom_type(&app.custom_type_input)
        {
            app.custom_type_input.clear();
            changed = true;
        }
    });
    changed
}

fn term_rules(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) -> bool {
    let mut changed = false;
    ui.label(RichText::new("Terms always redacted, whole word, any case.").size(11.0).color(tk.text_faint));
    let types = app.settings.all_types(&app.catalog);
    let mut remove = None;
    let mut retype: Option<(usize, EntityType)> = None;
    for (i, r) in app.settings.term_rules.iter().enumerate() {
        let c = type_color(&app.catalog, &r.entity_type);
        egui::Frame::new()
            .fill(tk.input)
            .stroke(Stroke::new(1.0, tk.border))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(&r.term).strong().color(tk.text)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("🗙").on_hover_text("Remove this Term Rule").clicked() {
                            remove = Some(r.term.clone());
                        }
                        egui::ComboBox::from_id_salt(("term_type", i))
                            .selected_text(RichText::new(r.entity_type.as_str()).size(11.5).color(c))
                            .width(96.0)
                            .height(300.0)
                            .show_ui(ui, |ui| {
                                for t in &types {
                                    if ui.selectable_label(*t == r.entity_type, RichText::new(t.as_str()).color(type_color(&app.catalog, t))).clicked() {
                                        retype = Some((i, t.clone()));
                                    }
                                }
                            });
                    });
                });
            });
        ui.add_space(2.0);
    }
    if let Some(term) = remove {
        app.settings.remove_term_rule(&term);
        changed = true;
    }
    if let Some((i, t)) = retype {
        app.settings.term_rules[i].entity_type = t;
        changed = true;
    }
    ui.add_space(4.0);
    let r = ui.add(egui::TextEdit::singleline(&mut app.term_input).hint_text("Add term…").desired_width(f32::INFINITY));
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.horizontal(|ui| {
        ui.label(RichText::new("as").size(12.0).color(tk.text_dim));
        let c = type_color(&app.catalog, &app.term_type);
        egui::ComboBox::from_id_salt("term_new_type")
            .selected_text(RichText::new(app.term_type.as_str()).size(12.0).color(c))
            .width(120.0)
            .height(300.0)
            .show_ui(ui, |ui| {
                for t in &types {
                    ui.selectable_value(&mut app.term_type, t.clone(), RichText::new(t.as_str()).color(type_color(&app.catalog, t)));
                }
            });
        let add = ghost_button(ui, tk, "Add", !app.term_input.trim().is_empty()).clicked();
        if (add || enter) && app.settings.add_term_rule(&app.catalog, &app.term_input, &app.term_type) {
            app.term_input.clear();
            changed = true;
        }
    });
    changed
}
