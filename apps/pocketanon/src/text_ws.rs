//! Text Anonymization: input, Review, Result.

use std::time::Instant;

use egui::{FontId, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};
use pocketanon_core::{Review, SegmentId, SegmentKind};

use crate::app::PocketAnon;
use crate::review_ui::{self, SegmentReview};
use crate::text_util::{selection_span, trim_span};
use crate::theme::{Tokens, type_color};
use crate::widgets::{caption, card, card_header, chip, ghost_button};

const REVIEW_WIDTH: f32 = 330.0;

pub fn show(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let full = ui.available_size();
    let gap = 12.0;
    let review_w = REVIEW_WIDTH.min(full.x * 0.36);
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
        let current = app.text.review_current();
        let working = app.text.detecting.is_some() || app.text.dirty;
        card_header(ui, tk, "Input", |ui| {
            let n = app.text.input.chars().count();
            ui.label(RichText::new(format!("{n} characters")).size(11.5).color(tk.text_faint));
            if working && n > 0 {
                chip(ui, "Analyzing…", tk.text_faint, tk.dark);
            } else if current {
                chip(ui, "Review", tk.accent, tk.dark);
            }
        });
        ui.add_space(6.0);
        let avail = ui.available_size();
        let menu_types = app.settings.active_types(&app.catalog);
        let mut menu_choice: Option<(String, pocketanon_core::EntityType)> = None;
        let mut bar_anchor: Option<(pocketanon_core::Span, egui::Pos2)> = None;
        ScrollArea::vertical().id_salt("input_scroll").max_height(avail.y - 44.0).auto_shrink([false, false]).show(ui, |ui| {
            // The editor stays editable at all times. While the Review matches the text, the
            // layouter paints Candidate highlights; after an edit they vanish until detection
            // catches up (a second after the last keystroke).
            let hovered = app.hovered;
            let catalog = &app.catalog;
            let new_type = &mut app.new_type_input;
            let text = &mut app.text;
            let review = if current { text.reviews.as_ref().map(|r| &r[0].review) } else { None };
            let seg_id = SegmentId(0);
            let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                let job = review_ui::highlight_job(buf.as_str(), review, seg_id, hovered, catalog, tk, editor_font(), wrap);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let out = TextEdit::multiline(&mut text.input)
                .id_salt("input_editor")
                .hint_text("Paste or type text with personal information…\n\nExample: Sehr geehrter Herr Max Mustermann, bitte senden Sie die Unterlagen an max.mustermann@example.com.")
                .font(editor_font())
                .frame(egui::Frame::NONE)
                .margin(Margin::same(4))
                .desired_width(f32::INFINITY)
                .desired_rows(16)
                .layouter(&mut layouter)
                .show(ui);
            if out.response.changed() {
                text.edited(Instant::now());
            }
            // Right-click a selection: always redact it as some type (a Term Rule).
            let sel = text.selection.filter(|s| s.is_valid_in(&text.input));
            out.response.context_menu(|ui| match sel {
                Some(s) => {
                    let term = text.input[s.range()].to_string();
                    let label = format!("Redact “{}” as", crate::text_util::preview(&term, 24));
                    if let Some(t) = review_ui::type_submenu(ui, &label, &menu_types, catalog, new_type) {
                        menu_choice = Some((term, t));
                    }
                }
                None => {
                    ui.label(RichText::new("Select text to redact it").color(tk.text_faint));
                }
            });
            if let Some(cr) = out.cursor_range {
                let sel = selection_span(&text.input, cr.primary.index.0, cr.secondary.index.0).and_then(|s| trim_span(&text.input, s));
                if sel.is_some() || out.response.has_focus() {
                    text.selection = sel;
                }
            }
            // Where the Redact bar goes: just above the start of the selection.
            if let Some(s) = text.selection.filter(|s| s.is_valid_in(&text.input)) {
                let ci = crate::text_util::byte_to_char(&text.input, s.start);
                let r = out.galley.pos_from_cursor(egui::text::CCursor::new(ci)).translate(out.galley_pos.to_vec2());
                if out.text_clip_rect.expand(4.0).contains(r.left_top()) {
                    bar_anchor = Some((s, r.left_top()));
                }
            }
        });
        if let Some((s, pos)) = bar_anchor
            && !crate::redact_bar::suppressed(ui.ctx())
        {
            let term = app.text.input[s.range()].to_string();
            app.redact_bar(ui.ctx(), crate::app::Workspace::Text, SegmentId(0), s, &term, pos);
        }
        if let Some((term, t)) = menu_choice {
            app.add_term_rule(&term, &t);
        }
        ui.add_space(4.0);
        manual_add_row(app, ui, tk);
    });
}

/// "Add selection as [type]": adds a manual Entity from the editor selection.
fn manual_add_row(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let types = app.settings.all_types(&app.catalog);
    ui.horizontal(|ui| {
        // Manual additions need a Review that matches the text on screen.
        let sel = app.text.selection.filter(|s| s.is_valid_in(&app.text.input));
        let label = match sel {
            Some(s) => format!("Add “{}” as", crate::text_util::preview(&app.text.input[s.range()], 18)),
            None => "Select text to add it as".to_string(),
        };
        let can_add = sel.is_some() && !app.text.dirty && app.text.detecting.is_none();
        let clicked = ghost_button(ui, tk, &format!("+ {label}"), can_add).clicked();
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
                vec![SegmentReview::new(SegmentId(0), SegmentKind::Body, app.text.input.clone(), Review::new(Vec::new(), threshold))]
            });
            let seg = &mut reviews[0];
            if seg.text == app.text.input && seg.review.add_manual(&seg.text, span, app.text.manual_type.clone()).is_some() {
                app.text.selection = None;
                app.text.output_stale = true;
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
        let mut ops = Vec::new();
        match &app.text.reviews {
            None => {
                empty_state(
                    ui,
                    tk,
                    if app.text.detecting.is_some() || app.text.dirty { "Analyzing…" } else { "No Review yet" },
                    "Candidates appear here by themselves, a second after you stop typing. Accept, reject or retype each one; the Result follows.",
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
                        let labels = review_ui::placeholder_labels(reviews);
                        let items = review_ui::all_visible(reviews);
                        ops = review_ui::candidate_rows(ui, reviews, &items, &labels, &types, &catalog, tk, &mut hovered, &mut app.new_type_input);
                    });
            }
        }
        app.apply_review_ops(crate::app::Workspace::Text, ops);
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
        let pending = has && (app.text.dirty || app.text.detecting.is_some());
        let mut copy = false;
        card_header(ui, tk, "Result", |ui| {
            copy = ghost_button(ui, tk, "Copy", has).on_hover_text("Copy the Result to the clipboard").clicked();
            if pending {
                ui.label(RichText::new("updating…").size(11.5).color(tk.text_faint));
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
                "The anonymized text appears here by itself: every accepted Entity becomes an indexed placeholder such as [PERSON_1].",
            );
        }
        if copy {
            app.copy_result(&ctx);
        }
    });
}
