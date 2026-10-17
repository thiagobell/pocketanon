//! File Anonymization: open a Document, Review its segments, Apply & Save.

use egui::{Align2, CornerRadius, Layout, RichText, ScrollArea, Stroke, Ui};
use pocketanon_core::SegmentKind;

use crate::app::PocketAnon;
use crate::review_ui;
use crate::text_ws::{empty_state, review_header};
use crate::theme::{Tokens, highlight_fill, type_color};
use crate::widgets::{caption, card, chip, ghost_button, primary_button};

const REVIEW_WIDTH: f32 = 380.0;
/// A4 in points, used until a page has been rendered.
const PLACEHOLDER_PAGE: egui::Vec2 = egui::vec2(595.0, 842.0);

const KIND_ORDER: [SegmentKind; 8] = [
    SegmentKind::Body,
    SegmentKind::Header,
    SegmentKind::Footer,
    SegmentKind::Footnote,
    SegmentKind::Endnote,
    SegmentKind::Comment,
    SegmentKind::TextBox,
    SegmentKind::Other,
];

pub fn show(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    if app.file.doc.is_none() {
        drop_zone(app, ui, tk, hovering_files);
        return;
    }
    let full = ui.available_size();
    let gap = 12.0;
    let review_w = REVIEW_WIDTH.min(full.x * 0.4);
    let centre = (full.x - review_w - gap).max(240.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(egui::vec2(centre, full.y), Layout::top_down(egui::Align::Min), |ui| {
            doc_header(app, ui, tk);
            ui.add_space(10.0);
            if app.file.page_count > 0 {
                pages_view(app, ui);
            } else {
                segments_view(app, ui, tk);
            }
        });
        ui.allocate_ui_with_layout(egui::vec2(review_w, full.y), Layout::top_down(egui::Align::Min), |ui| {
            review_panel(app, ui, tk);
        });
    });
}

fn drop_zone(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens, hovering: bool) {
    let avail = ui.available_rect_before_wrap();
    let rect = egui::Rect::from_center_size(avail.center(), egui::vec2(avail.width().min(560.0), 300.0));
    let fill = if hovering { tk.accent_soft } else { tk.card };
    let stroke = Stroke::new(if hovering { 2.0 } else { 1.0 }, if hovering { tk.accent } else { tk.border_strong });
    ui.painter().rect(rect, CornerRadius::same(16), fill, stroke, egui::StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(28.0)).layout(Layout::top_down(egui::Align::Center)));
    child.add_space(24.0);
    child.label(RichText::new("📄").size(40.0).color(if hovering { tk.accent } else { tk.text_dim }));
    child.add_space(8.0);
    child.label(RichText::new(if app.file.loading { "Opening…" } else { "Drop a Document here" }).size(18.0).strong().color(tk.text));
    child.add_space(4.0);
    child.label(RichText::new("TXT, DOCX or RTF. The input is never overwritten; the Result is saved beside it.").color(tk.text_dim));
    child.add_space(16.0);
    if primary_button(&mut child, tk, "  Open…  ", !app.file.loading).clicked() {
        app.open_file_dialog();
    }
    child.add_space(10.0);
    child.label(RichText::new("⌘O").size(11.0).color(tk.text_faint));
}

fn doc_header(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    card(tk).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("📄").size(22.0).color(tk.accent));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&app.file.name).size(14.0).strong().color(tk.text));
                    if let Some(f) = app.file.format {
                        chip(ui, &f.to_string(), tk.accent, tk.dark);
                    }
                });
                if let Some(f) = app.file.format {
                    ui.label(RichText::new(format!("Fidelity · {}", f.fidelity())).size(12.0).color(tk.text_dim));
                }
            });
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, tk, "Close", true).clicked() {
                    app.clear();
                }
                if ghost_button(ui, tk, "Open…", true).clicked() {
                    app.open_file_dialog();
                }
                if app.file.page_count > 0 {
                    ui.add(egui::Slider::new(&mut app.file.zoom, 0.4..=2.5).show_value(false));
                    ui.label(RichText::new(format!("{:.0}%", app.file.zoom * 100.0)).size(12.0).color(tk.text_dim));
                }
            });
        });
        if let Some(out) = &app.file.saved_to {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Saved").strong().color(tk.ok));
                ui.label(RichText::new(format!("Result written to {}", out.display())).size(12.0).color(tk.text_dim));
            });
        }
    });
}

/// Rendered pages, lazily, with highlight rectangles from `locate()`.
fn pages_view(app: &mut PocketAnon, ui: &mut Ui) {
    let ppp = ui.ctx().pixels_per_point();
    let zoom = app.file.zoom;
    let want_scale = zoom * ppp;
    let mut requests = Vec::new();
    ScrollArea::both().id_salt("pages").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let width = ui.available_width();
        let mut y = 8.0;
        let origin = ui.min_rect().min;
        for page in 0..app.file.page_count {
            let size1 = app.file.pages.get(&page).map_or(PLACEHOLDER_PAGE, |p| p.size);
            let size = size1 * zoom;
            let x = ((width - size.x) / 2.0).max(8.0);
            let local = egui::Rect::from_min_size(egui::pos2(x, y), size);
            let rect = local.translate(origin.to_vec2());
            y += size.y + 16.0;
            if !local.intersects(viewport.expand(400.0)) {
                continue;
            }
            // Page shadow + paper.
            ui.painter().rect_filled(rect.translate(egui::vec2(0.0, 3.0)).expand(1.0), CornerRadius::same(3), egui::Color32::from_black_alpha(90));
            ui.painter().rect_filled(rect, CornerRadius::same(2), egui::Color32::WHITE);
            match app.file.pages.get(&page) {
                Some(tex) => {
                    ui.painter().image(tex.texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    if (tex.scale - want_scale).abs() > 0.05 && !app.file.pending_pages.contains(&page) {
                        requests.push(page);
                    }
                }
                None => {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("Page {}", page + 1), egui::FontId::proportional(14.0), egui::Color32::GRAY);
                    if !app.file.pending_pages.contains(&page) {
                        requests.push(page);
                    }
                }
            }
            // Highlights.
            if let Some(reviews) = &app.file.reviews {
                for seg in reviews {
                    for item in seg.review.visible() {
                        let Some(pls) = app.file.placements.get(&(seg.id, item.candidate.span)) else { continue };
                        let c = type_color(&app.catalog, &item.candidate.entity_type);
                        let hover = app.hovered == Some((seg.id, item.id));
                        for pl in pls.iter().filter(|p| p.page == page) {
                            for r in &pl.rects {
                                let hr = egui::Rect::from_min_size(rect.min + egui::vec2(r.x, r.y) * zoom, egui::vec2(r.w, r.h) * zoom);
                                let fill = if !item.accepted {
                                    egui::Color32::TRANSPARENT
                                } else if hover {
                                    c.gamma_multiply(0.6)
                                } else {
                                    highlight_fill(c, false)
                                };
                                ui.painter().rect(hr, CornerRadius::same(2), fill, Stroke::new(1.0, c), egui::StrokeKind::Outside);
                            }
                        }
                    }
                }
            }
        }
        ui.allocate_space(egui::vec2(width, y));
    });
    if let Some(doc) = app.file.doc.clone() {
        for page in requests {
            app.file.pending_pages.insert(page);
            app.jobs.render_page(doc.clone(), app.file.ticket, page, want_scale);
        }
    }
}

/// No page preview: the segment texts with highlights, grouped like the Review list.
fn segments_view(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        ScrollArea::vertical().id_salt("segments").auto_shrink([false, false]).show(ui, |ui| {
            let wrap = ui.available_width();
            let mut last_kind = None;
            for seg in &app.file.segments {
                if seg.text.trim().is_empty() {
                    continue;
                }
                if last_kind != Some(seg.kind) {
                    if last_kind.is_some() {
                        ui.add_space(8.0);
                    }
                    caption(ui, tk, &seg.kind.to_string());
                    last_kind = Some(seg.kind);
                }
                let review = app.file.reviews.as_ref().and_then(|r| r.iter().find(|r| r.id == seg.id)).map(|r| &r.review);
                let job = review_ui::highlight_job(&seg.text, review, seg.id, app.hovered, &app.catalog, tk, egui::FontId::proportional(14.0), wrap);
                ui.add(egui::Label::new(job).selectable(true));
                ui.add_space(6.0);
            }
        });
    });
}

fn review_panel(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        ui.set_max_width(ui.available_width());
        review_header(app, ui, tk);
        ui.add_space(6.0);
        let types = app.settings.all_types(&app.catalog);
        let catalog = app.catalog.clone();
        let mut hovered = None;
        let summary_h = if app.file.summary.is_some() { 110.0 } else { 0.0 };
        let mut changed = false;
        match &mut app.file.reviews {
            None => empty_state(
                ui,
                tk,
                if app.file.detecting.is_some() { "Detecting…" } else if app.file.summary.is_some() { "Saved" } else { "No Review yet" },
                "Detect runs over every part of the Document: body, headers, footers, footnotes and comments.",
            ),
            Some(reviews) => {
                ScrollArea::vertical().id_salt("file_review").max_height(ui.available_height() - summary_h).auto_shrink([false, false]).show(ui, |ui| {
                    if review_ui::visible_count(reviews) == 0 {
                        empty_state(ui, tk, "Nothing above the Threshold", "Lower the Threshold to see more Candidates.");
                    }
                    for kind in KIND_ORDER {
                        let n: usize = reviews.iter().filter(|r| r.kind == kind).map(|r| r.review.visible().count()).sum();
                        if n == 0 {
                            continue;
                        }
                        egui::CollapsingHeader::new(RichText::new(format!("{}  ·  {n}", capitalise(&kind.to_string()))).strong().color(tk.text_dim))
                            .id_salt(("kind", kind))
                            .default_open(true)
                            .show(ui, |ui| {
                                changed |= review_ui::candidate_rows(ui, reviews, Some(kind), &types, &catalog, tk, &mut hovered);
                            });
                    }
                });
            }
        }
        app.hovered = hovered;
        if changed {
            app.file.summary = None;
        }
        if let Some(s) = &app.file.summary {
            ui.separator();
            caption(ui, tk, "Summary");
            review_ui::summary_view(ui, s, &app.catalog, tk);
        }
    });
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}
