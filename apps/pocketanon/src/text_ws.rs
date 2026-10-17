//! Text Anonymization: input, Review, Result.

use egui::{FontId, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};
use pocketanon_core::{Review, SegmentId, SegmentKind};

use crate::app::PocketAnon;
use crate::review_ui::{self, SegmentReview};
use crate::text_util::{selection_span, trim_span};
use crate::theme::{Tokens, type_color};
use crate::widgets::{caption, card, card_header, chip, ghost_button, primary_button};

const REVIEW_WIDTH: f32 = 360.0;

pub fn show(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let full = ui.available_size();
    let gap = 12.0;
    let review_w = REVIEW_WIDTH.min(full.x * 0.34);
    let side = ((full.x - review_w - 2.0 * gap) / 2.0).max(150.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(egui::vec2(side, full.y), Layout::top_down(egui::Align::Min), |ui| input_card(app, ui, tk));
        ui.allocate_ui_with_layout(egui::vec2(review_w, full.y), Layout::top_down(egui::Align::Min), |ui| review_card(app, ui, tk));
        ui.allocate_ui_with_layout(egui::vec2(side, full.y), Layout::top_down(egui::Align::Min), |ui| result_card(app, ui, tk));
    });
}

fn editor_font() -> FontId {
    FontId::proportional(14.0)
}

fn input_card(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        let reviewing = app.text.reviews.is_some();
        card_header(ui, tk, "Input", |ui| {
            if reviewing {
                if ghost_button(ui, tk, "✏  Edit text", true).on_hover_text("Back to editing; clears the Review").clicked() {
                    app.text.reviews = None;
                    app.text.output.clear();
                    app.text.summary = None;
                    app.text.selection = None;
                }
                chip(ui, "Review", tk.accent, tk.dark);
            } else {
                let n = app.text.input.chars().count();
                ui.label(RichText::new(format!("{n} characters")).size(11.5).color(tk.text_faint));
            }
        });
        ui.add_space(6.0);
        let avail = ui.available_size();
        ScrollArea::vertical().id_salt("input_scroll").max_height(avail.y - 44.0).auto_shrink([false, false]).show(ui, |ui| {
            let out = if let Some(reviews) = &app.text.reviews {
                // Review mode: the same text, read-only, with highlighted Candidates. Selection
                // still works, for adding Entities by hand.
                let seg = &reviews[0];
                let hovered = app.hovered;
                let catalog = &app.catalog;
                let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let job = review_ui::highlight_job(buf.as_str(), Some(&seg.review), seg.id, hovered, catalog, tk, editor_font(), wrap);
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let mut text: &str = &seg.text;
                TextEdit::multiline(&mut text)
                    .id_salt("input_review")
                    .frame(egui::Frame::NONE)
                    .margin(Margin::same(4))
                    .desired_width(f32::INFINITY)
                    .desired_rows(16)
                    .layouter(&mut layouter)
                    .show(ui)
            } else {
                TextEdit::multiline(&mut app.text.input)
                    .id_salt("input_editor")
                    .hint_text("Paste or type text with personal information…\n\nExample: Sehr geehrter Herr Max Mustermann, bitte senden Sie die Unterlagen an max.mustermann@example.com.")
                    .font(editor_font())
                    .frame(egui::Frame::NONE)
                    .margin(Margin::same(4))
                    .desired_width(f32::INFINITY)
                    .desired_rows(16)
                    .show(ui)
            };
            let text = app.text.reviews.as_ref().map_or(app.text.input.as_str(), |r| r[0].text.as_str());
            if let Some(cr) = out.cursor_range {
                let sel = selection_span(text, cr.primary.index.0, cr.secondary.index.0).and_then(|s| trim_span(text, s));
                if sel.is_some() || out.response.has_focus() {
                    app.text.selection = sel;
                }
            }
        });
        ui.add_space(4.0);
        manual_add_row(app, ui, tk);
    });
}

/// "Add selection as [type]": adds a manual Entity from the editor selection.
fn manual_add_row(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let types = app.settings.all_types(&app.catalog);
    ui.horizontal(|ui| {
        let sel = app.text.selection;
        let label = match sel {
            Some(s) => {
                let text = app.text.reviews.as_ref().map_or(app.text.input.as_str(), |r| r[0].text.as_str());
                format!("Add “{}” as", crate::text_util::preview(&text[s.range()], 18))
            }
            None => "Select text to add it as".to_string(),
        };
        let clicked = ghost_button(ui, tk, &format!("+ {label}"), sel.is_some()).clicked();
        let c = type_color(&app.catalog, &app.text.manual_type);
        egui::ComboBox::from_id_salt("manual_type")
            .selected_text(RichText::new(app.text.manual_type.as_str()).color(c))
            .height(320.0)
            .show_ui(ui, |ui| {
                for t in &types {
                    let tc = type_color(&app.catalog, t);
                    ui.selectable_value(&mut app.text.manual_type, t.clone(), RichText::new(t.as_str()).color(tc));
                }
            });
        if clicked && let Some(span) = sel {
            let threshold = app.settings.threshold;
            let reviews = app.text.reviews.get_or_insert_with(|| {
                vec![SegmentReview {
                    id: SegmentId(0),
                    kind: SegmentKind::Body,
                    text: app.text.input.clone(),
                    review: Review::new(Vec::new(), threshold),
                }]
            });
            let seg = &mut reviews[0];
            if seg.review.add_manual(&seg.text, span, app.text.manual_type.clone()).is_some() {
                app.text.selection = None;
                app.text.output.clear();
                app.text.summary = None;
            }
        }
    });
}

fn review_card(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        ui.set_max_width(ui.available_width());
        review_header(app, ui, tk);
        ui.add_space(6.0);
        let types = app.settings.all_types(&app.catalog);
        let catalog = app.catalog.clone();
        let mut hovered = None;
        let mut changed = false;
        match &mut app.text.reviews {
            None => {
                empty_state(
                    ui,
                    tk,
                    if app.text.detecting.is_some() { "Detecting…" } else { "No Review yet" },
                    "Run Detect to see Candidates here. Accept, reject or retype each one, then Apply.",
                );
            }
            Some(reviews) => {
                let summary_h = if app.text.summary.is_some() { 110.0 } else { 0.0 };
                ScrollArea::vertical()
                    .id_salt("text_review_list")
                    .max_height(ui.available_height() - summary_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if review_ui::visible_count(reviews) == 0 {
                            empty_state(ui, tk, "Nothing above the Threshold", "Lower the Threshold or add an Entity by hand.");
                        }
                        changed = review_ui::candidate_rows(ui, reviews, None, &types, &catalog, tk, &mut hovered);
                    });
            }
        }
        if changed {
            app.text.output.clear();
            app.text.summary = None;
        }
        app.hovered = hovered;
        if let Some(s) = &app.text.summary {
            ui.separator();
            caption(ui, tk, "Summary");
            review_ui::summary_view(ui, s, &app.catalog, tk);
        }
    });
}

/// Threshold slider and counts; shared look with the File workspace.
pub fn review_header(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let reviews = match app.workspace {
        crate::app::Workspace::Text => app.text.reviews.as_deref(),
        crate::app::Workspace::File => app.file.reviews.as_deref(),
    };
    let (vis, hid) = reviews.map_or((0, 0), |r| (review_ui::visible_count(r), review_ui::hidden_count(r)));
    card_header(ui, tk, "Review", |ui| {
        if reviews.is_some() {
            let s = if hid > 0 { format!("{vis} shown · {hid} below Threshold") } else { format!("{vis} Candidates") };
            ui.label(RichText::new(s).size(11.5).color(tk.text_faint));
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Threshold").size(12.0).color(tk.text_dim));
        let mut t = app.settings.threshold;
        let r = ui.add(egui::Slider::new(&mut t, 0.0..=1.0).step_by(0.01).fixed_decimals(2));
        if r.changed() {
            app.set_threshold(t);
        }
    });
}

pub fn empty_state(ui: &mut Ui, tk: &Tokens, title: &str, body: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(14.0).strong().color(tk.text_dim));
        ui.add_space(4.0);
        ui.label(RichText::new(body).size(12.0).color(tk.text_faint));
    });
    ui.add_space(24.0);
}

fn result_card(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    let ctx = ui.ctx().clone();
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        let has = !app.text.output.is_empty();
        let mut copy = false;
        let mut apply = false;
        card_header(ui, tk, "Result", |ui| {
            copy = ghost_button(ui, tk, "Copy", has).on_hover_text("Copy the Result to the clipboard").clicked();
            if !has {
                apply = primary_button(ui, tk, "Apply", app.text.reviews.is_some()).clicked();
            }
        });
        ui.add_space(6.0);
        if has {
            ScrollArea::vertical().id_salt("result_scroll").auto_shrink([false, false]).show(ui, |ui| {
                let mut out: &str = &app.text.output;
                ui.add(
                    TextEdit::multiline(&mut out)
                        .id_salt("result_view")
                        .font(editor_font())
                        .frame(egui::Frame::NONE)
                        .margin(Margin::same(4))
                        .desired_width(f32::INFINITY),
                );
            });
        } else {
            empty_state(
                ui,
                tk,
                "No Result yet",
                &format!("Apply replaces every accepted Entity using {}.", crate::app::strategy_label(app.settings.strategy)),
            );
        }
        if copy {
            app.copy_result(&ctx);
        }
        if apply {
            app.apply_text();
        }
    });
}
