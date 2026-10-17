//! File Anonymization: open a Document, Review its segments, Apply & Save.

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect, RichText, ScrollArea, Stroke, TextEdit, Ui};
use std::time::Instant;

use pocketanon_core::{CandidateId, SegmentId};

use crate::app::{PageSelection, PocketAnon, Workspace, ZOOM_RANGE};
use crate::review_ui::{self, ReviewOp};
use crate::text_util::{selection_span, trim_span};
use crate::text_ws::{empty_state, review_header};
use crate::theme::{Tokens, highlight_fill, type_color};
use crate::widgets::{caption, card, chip, ghost_button, primary_button};

const REVIEW_WIDTH: f32 = 340.0;
/// A4 in points, used until a page has been rendered.
const PLACEHOLDER_PAGE: egui::Vec2 = egui::vec2(595.0, 842.0);


pub fn show(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    if app.file.doc.is_none() {
        drop_zone(app, ui, tk, hovering_files);
        return;
    }
    let full = ui.available_size();
    let gap = 12.0;
    let review_w = REVIEW_WIDTH.min(full.x * 0.42);
    let centre = (full.x - review_w - gap).max(240.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(egui::vec2(centre, full.y), Layout::top_down(egui::Align::Min), |ui| {
            doc_header(app, ui, tk);
            ui.add_space(10.0);
            if app.file.page_count > 0 {
                pages_view(app, ui, tk);
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
                    ui.add(egui::Label::new(RichText::new(&app.file.name).size(14.0).strong().color(tk.text)).truncate());
                    if let Some(f) = app.file.format {
                        chip(ui, &f.to_string(), tk.accent, tk.dark);
                    }
                });
                if let Some(f) = app.file.format {
                    ui.add(egui::Label::new(RichText::new(format!("Fidelity · {}", f.fidelity())).size(12.0).color(tk.text_dim)).truncate());
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            zoom_controls(app, ui, tk);
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, tk, "Close", true).clicked() {
                    app.clear();
                }
                if ghost_button(ui, tk, "Open…", true).clicked() {
                    app.open_file_dialog();
                }
            });
        });
        if let Some(out) = &app.file.saved_to {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Saved").strong().color(tk.ok));
                ui.add(egui::Label::new(RichText::new(format!("Result written to {}", out.display())).size(12.0).color(tk.text_dim)).truncate());
            });
        }
    });
}

/// − / slider / + / Fit, with the percentage. ⌘+ ⌘− ⌘0 do the same.
fn zoom_controls(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    ui.spacing_mut().item_spacing.x = 4.0;
    ui.label(RichText::new("Zoom").size(12.0).color(tk.text_dim));
    if ui.small_button(" − ").on_hover_text("Zoom out  ⌘−").clicked() {
        app.file.zoom_by(1.0 / 1.2);
    }
    let mut z = app.file.zoom;
    ui.spacing_mut().slider_width = 110.0;
    if ui.add(egui::Slider::new(&mut z, ZOOM_RANGE).logarithmic(true).show_value(false)).changed() {
        app.file.zoom = z;
        app.file.fit_pending = false;
    }
    if ui.small_button(" + ").on_hover_text("Zoom in  ⌘+").clicked() {
        app.file.zoom_by(1.2);
    }
    ui.label(RichText::new(format!("{:>3.0}%", app.file.zoom * 100.0)).monospace().size(11.5).color(tk.text_dim));
    if ui.small_button("Fit").on_hover_text("Fit the page width  ⌘0").clicked() {
        app.file.fit_pending = true;
    }
}

/// Font size of placeholder tags at `zoom`: grows gently with the page (so tags do not swamp
/// the text when zoomed in), never below 9 pt.
pub fn label_size(zoom: f32) -> f32 {
    (10.0 * zoom.sqrt()).clamp(9.0, 14.0)
}

/// Place a tag of `size` at the top-left above `anchor`, moving it off tags already placed.
pub fn place_label(anchor: Rect, size: egui::Vec2, taken: &[Rect]) -> Rect {
    let mut r = Rect::from_min_size(egui::pos2(anchor.left(), anchor.top() - size.y - 1.0), size);
    for _ in 0..6 {
        match taken.iter().find(|t| t.intersects(r)) {
            Some(t) => r = r.translate(egui::vec2(t.right() - r.left() + 3.0, 0.0)),
            None => break,
        }
    }
    r
}

/// Rendered pages, lazily, with highlight rectangles from `locate()` and a placeholder tag on
/// each. Right-click a highlight for its menu.
fn pages_view(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let ppp = ui.ctx().pixels_per_point();
    // Pages are rendered at screen resolution, but never beyond the GPU's texture limit.
    let max_side = ui.ctx().input(|i| i.max_texture_side) as f32 - 2.0;
    let mut requests: Vec<(usize, f32)> = Vec::new();
    let mut new_hover = None;
    let mut ops: Vec<ReviewOp> = Vec::new();
    let mut sel_event: Option<SelEvent> = None;
    let mut sel_anchor: Option<egui::Pos2> = None;
    let mut page_rects: Vec<(usize, Rect)> = Vec::new();
    let types = app.settings.all_types(&app.catalog);
    // The current page selection, as (segment review, span, text) for menus.
    let selection: Option<(usize, pocketanon_core::Span, String)> = app.file.page_sel.as_ref().and_then(|s| {
        let span = s.span?;
        let reviews = app.file.reviews.as_ref()?;
        let si = reviews.iter().position(|r| r.id == s.anchor.segment)?;
        Some((si, span, reviews[si].text[span.range()].to_string()))
    });
    let labels = app.file.reviews.as_deref().map(review_ui::placeholder_labels).unwrap_or_default();
    // Index candidates per page once.
    let mut per_page: HashMap<usize, Vec<(usize, CandidateId)>> = HashMap::new();
    if let Some(reviews) = &app.file.reviews {
        for (si, seg) in reviews.iter().enumerate() {
            for item in seg.review.visible() {
                if let Some(pls) = app.file.placements.get(&(seg.id, item.candidate.span)) {
                    let mut pages: Vec<usize> = pls.iter().map(|p| p.page).collect();
                    pages.dedup();
                    for p in pages {
                        per_page.entry(p).or_default().push((si, item.id));
                    }
                }
            }
        }
    }
    let frame = egui::Frame::new().fill(tk.canvas).corner_radius(CornerRadius::same(10)).stroke(Stroke::new(1.0, tk.border));
    let frame_resp = frame.show(ui, |ui| {
        let width_avail = ui.available_width();
        if app.file.fit_pending {
            let w1 = app.file.pages.get(&0).map_or(PLACEHOLDER_PAGE.x, |p| p.size.x);
            app.file.zoom = ((width_avail - 40.0) / w1).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
            if app.file.pages.contains_key(&0) {
                app.file.fit_pending = false;
            }
        }
        let zoom = app.file.zoom;
        let want_scale = zoom * ppp;
        ScrollArea::both().id_salt("pages").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            let mut y = 16.0;
            let origin = ui.min_rect().min;
            let mut max_w: f32 = 0.0;
            for page in 0..app.file.page_count {
                let size1 = app.file.pages.get(&page).map_or(PLACEHOLDER_PAGE, |p| p.size);
                let size = size1 * zoom;
                let page_scale = want_scale.min(max_side / size1.max_elem());
                max_w = max_w.max(size.x + 32.0);
                let x = ((width - size.x) / 2.0).max(16.0);
                let local = Rect::from_min_size(egui::pos2(x, y), size);
                let rect = local.translate(origin.to_vec2());
                y += size.y + 20.0;
                if !local.intersects(viewport.expand(400.0)) {
                    continue;
                }
                let painter = ui.painter().clone();
                painter.rect_filled(rect.translate(egui::vec2(0.0, 3.0)).expand(1.0), CornerRadius::same(3), Color32::from_black_alpha(90));
                painter.rect_filled(rect, CornerRadius::same(2), Color32::WHITE);
                match app.file.pages.get(&page) {
                    Some(tex) => {
                        painter.image(tex.texture.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        if (tex.scale - page_scale).abs() > 0.05 && !app.file.pending_pages.contains(&page) {
                            requests.push((page, page_scale));
                        }
                    }
                    None => {
                        painter.text(rect.center(), Align2::CENTER_CENTER, format!("Page {}", page + 1), FontId::proportional(14.0), Color32::GRAY);
                        if !app.file.pending_pages.contains(&page) {
                            requests.push((page, page_scale));
                        }
                    }
                }
                painter.text(
                    rect.left_bottom() + egui::vec2(0.0, 4.0),
                    Align2::LEFT_TOP,
                    format!("{}", page + 1),
                    FontId::proportional(10.5),
                    tk.text_faint,
                );
                // Dragging on the page selects text (hit-tested after the frame).
                page_rects.push((page, rect));
                let page_resp = ui.interact(rect, ui.id().with(("page", page)), egui::Sense::click_and_drag());
                if let Some(pos) = page_resp.interact_pointer_pos() {
                    if page_resp.drag_started() {
                        sel_event = Some(SelEvent::Start(pos));
                    } else if page_resp.dragged() {
                        sel_event = Some(SelEvent::Move(pos));
                    } else if page_resp.clicked() {
                        sel_event = Some(SelEvent::Clear);
                    }
                }
                if let Some((si, span, text)) = &selection {
                    page_resp.context_menu(|ui| {
                        if let Some(reviews) = &app.file.reviews {
                            ops.extend(selection_menu(ui, reviews[*si].id, *span, text, &types, &app.catalog, &mut app.new_type_input));
                        }
                    });
                }
                // The selection, under the entity highlights.
                if let Some(sel) = &app.file.page_sel {
                    for pl in sel.placements.iter().filter(|p| p.page == page) {
                        for r in &pl.rects {
                            let sr = Rect::from_min_size(rect.min + egui::vec2(r.x, r.y) * zoom, egui::vec2(r.w, r.h) * zoom);
                            if sel_anchor.is_none() && viewport.translate(origin.to_vec2()).contains(sr.left_top()) {
                                sel_anchor = Some(sr.left_top());
                            }
                            painter.rect_filled(sr, CornerRadius::same(1), tk.accent.gamma_multiply(0.35));
                        }
                    }
                }
                let (Some(reviews), Some(items)) = (&app.file.reviews, per_page.get(&page)) else { continue };
                let mut taken: Vec<Rect> = Vec::new();
                for &(si, id) in items {
                    let seg = &reviews[si];
                    let Some(item) = seg.review.get(id) else { continue };
                    let Some(pls) = app.file.placements.get(&(seg.id, item.candidate.span)) else { continue };
                    let c = type_color(&app.catalog, &item.candidate.entity_type);
                    let hover = app.hovered == Some((seg.id, id));
                    let mut first: Option<Rect> = None;
                    for pl in pls.iter().filter(|p| p.page == page) {
                        for r in &pl.rects {
                            let hr = Rect::from_min_size(rect.min + egui::vec2(r.x, r.y) * zoom, egui::vec2(r.w, r.h) * zoom);
                            first.get_or_insert(hr);
                            let fill = if !item.accepted {
                                Color32::TRANSPARENT
                            } else if hover {
                                c.gamma_multiply(0.6)
                            } else {
                                highlight_fill(c, false)
                            };
                            let stroke = if item.accepted { Stroke::new(1.0, c) } else { Stroke::new(1.0, c.gamma_multiply(0.5)) };
                            painter.rect(hr, CornerRadius::same(2), fill, stroke, egui::StrokeKind::Outside);
                            let resp = ui.interact(hr, ui.id().with(("hl", seg.id, id, page, r.x as i32, r.y as i32)), egui::Sense::click());
                            if resp.hovered() {
                                new_hover = Some((seg.id, id));
                            }
                            resp.on_hover_text(format!("{} · right-click for options", review_ui::candidate_label(&labels, seg.id, item)))
                                .context_menu(|ui| {
                                    // With a selection, offer it first; the highlight's own menu follows.
                                    if let Some((si, span, text)) = &selection {
                                        ops.extend(selection_menu(ui, reviews[*si].id, *span, text, &types, &app.catalog, &mut app.new_type_input));
                                        ui.separator();
                                    }
                                    if let Some(op) = review_ui::candidate_menu(ui, seg, item, &types, &app.catalog, &mut app.new_type_input) {
                                        ops.push(op);
                                    }
                                });
                        }
                    }
                    // Placeholder tag at the top-left of the first rectangle.
                    if let Some(anchor) = first {
                        let text = review_ui::candidate_label(&labels, seg.id, item);
                        let font = FontId::monospace(label_size(zoom));
                        let (fg, bg) = if item.accepted { (Color32::from_rgb(0x10, 0x10, 0x14), c) } else { (Color32::from_gray(90), Color32::from_gray(225)) };
                        let galley = painter.layout_no_wrap(text, font, fg);
                        let size = galley.size() + egui::vec2(6.0, 2.0);
                        let lr = place_label(anchor, size, &taken);
                        taken.push(lr);
                        painter.rect_filled(lr, CornerRadius::same(3), bg);
                        let pos = lr.min + egui::vec2(3.0, 1.0);
                        painter.galley(pos, galley.clone(), fg);
                        if !item.accepted {
                            let mid = lr.center().y;
                            painter.line_segment([egui::pos2(lr.left() + 2.0, mid), egui::pos2(lr.right() - 2.0, mid)], Stroke::new(1.0, fg));
                        }
                    }
                }
            }
            ui.allocate_space(egui::vec2(max_w.max(width), y));
        });
    });
    // Apply & Save in progress: dim the pages and say what is happening.
    if app.file.saving {
        let r = frame_resp.response.rect;
        let p = ui.painter_at(r);
        p.rect_filled(r, CornerRadius::same(10), Color32::from_black_alpha(150));
        let msg = format!("Applying {} replacements and saving…", app.file.saving_count);
        p.text(r.center() + egui::vec2(0.0, 18.0), Align2::CENTER_CENTER, msg, FontId::proportional(15.0), Color32::WHITE);
        let spin = Rect::from_center_size(r.center() - egui::vec2(0.0, 14.0), egui::vec2(24.0, 24.0));
        ui.put(spin, egui::Spinner::new().size(24.0).color(tk.accent));
        // Swallow clicks on the pages while saving.
        ui.interact(r, ui.id().with("saving_overlay"), egui::Sense::click_and_drag());
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        sel_event = Some(SelEvent::Clear);
    }
    // The Redact bar above the page selection, once the drag is over.
    if let (Some(pos), Some((si, span, text))) = (sel_anchor, &selection)
        && !app.file.saving
        && !crate::redact_bar::suppressed(ui.ctx())
        && let Some(seg) = app.file.reviews.as_ref().map(|r| r[*si].id)
    {
        app.redact_bar(ui.ctx(), Workspace::File, seg, *span, text, pos);
    }
    if sel_event.is_some() {
        app.file.sel_pending = sel_event;
    }
    if let Some(ev) = app.file.sel_pending.take() {
        update_selection(app, ev, &page_rects);
        if app.file.sel_pending.is_some() {
            ui.ctx().request_repaint();
        }
    }
    if let Some(doc) = app.file.doc.clone() {
        for (page, scale) in requests {
            app.file.pending_pages.insert(page);
            app.jobs.render_page(doc.clone(), app.file.ticket, page, scale);
        }
    }
    if new_hover.is_some() {
        app.hovered = new_hover;
    }
    app.apply_review_ops(Workspace::File, ops);
}

/// A pointer event for the page selection, in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelEvent {
    Start(egui::Pos2),
    Move(egui::Pos2),
    Clear,
}

/// Hit-test the pointer on the page under it and update `page_sel`. Runs on the UI thread with
/// `try_lock`: when a worker holds the document (rendering), the event is kept and retried on
/// the next frame.
fn update_selection(app: &mut PocketAnon, ev: SelEvent, page_rects: &[(usize, Rect)]) {
    let pos = match ev {
        SelEvent::Clear => {
            app.file.page_sel = None;
            return;
        }
        SelEvent::Start(p) | SelEvent::Move(p) => p,
    };
    let Some(doc) = app.file.doc.clone() else { return };
    let Ok(d) = doc.try_lock() else {
        app.file.sel_pending = Some(ev);
        return;
    };
    let zoom = app.file.zoom;
    // The page under the pointer, or the nearest one vertically while dragging past a gap.
    let Some(&(page, rect)) = page_rects
        .iter()
        .find(|(_, r)| r.contains(pos))
        .or_else(|| page_rects.iter().min_by(|a, b| a.1.distance_to_pos(pos).total_cmp(&b.1.distance_to_pos(pos))))
    else {
        return;
    };
    let local = ((pos - rect.min) / zoom).max(egui::Vec2::ZERO);
    let started = Instant::now();
    let hit = d.hit_test(page, local.x, local.y, 1.0);
    let took = started.elapsed();
    if took.as_millis() > 2 {
        log::debug!("hit_test took {took:?}");
    }
    let order = |s: SegmentId| app.file.segments.iter().position(|x| x.id == s).unwrap_or(0);
    match ev {
        SelEvent::Start(_) => {
            app.file.page_sel = hit.map(|h| PageSelection { anchor: h, head: h, span: None, placements: Vec::new() });
        }
        SelEvent::Move(_) => {
            let Some(sel) = &mut app.file.page_sel else { return };
            let Some(h) = hit else { return };
            if h == sel.head {
                return;
            }
            sel.head = h;
            let Some(text) = app.file.segments.iter().find(|s| s.id == sel.anchor.segment).map(|s| s.text.as_str()) else { return };
            sel.span = crate::text_util::clamp_selection(
                text,
                (order(sel.anchor.segment), sel.anchor.offset),
                (order(h.segment), h.offset),
            );
            sel.placements = sel.span.map(|s| d.locate(sel.anchor.segment, s, 1.0)).unwrap_or_default();
        }
        SelEvent::Clear => {}
    }
}

/// "Redact “…” as ▸" (a manual Entity in that segment) and "Always redact as ▸" (a Term Rule).
fn selection_menu(
    ui: &mut Ui,
    seg: SegmentId,
    span: pocketanon_core::Span,
    text: &str,
    types: &[pocketanon_core::EntityType],
    catalog: &pocketanon_core::TypeCatalog,
    new_type: &mut String,
) -> Vec<ReviewOp> {
    let mut ops = Vec::new();
    ui.label(RichText::new(format!("“{}”", crate::text_util::preview(text, 32))).strong());
    if let Some(t) = review_ui::type_submenu(ui, "Redact as", types, catalog, new_type) {
        ops.push(ReviewOp::AddManual(seg, span, t));
    }
    if let Some(t) = review_ui::type_submenu(ui, "Always redact as (Term Rule)", types, catalog, new_type) {
        ops.push(ReviewOp::AddTerm(text.to_string(), t));
    }
    ops
}

/// No page preview: the segment texts with highlights. Text can be selected; right-click a
/// selection to always redact it.
fn segments_view(app: &mut PocketAnon, ui: &mut Ui, tk: &Tokens) {
    let h = ui.available_height();
    let font_size = 14.0 * app.file.zoom;
    let menu_types = app.settings.active_types(&app.catalog);
    let mut choice = None;
    let mut bar_anchor = None;
    card(tk).show(ui, |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), h - 26.0));
        ScrollArea::vertical().id_salt("segments").auto_shrink([false, false]).show(ui, |ui| {
            let mut last_kind = None;
            let hovered = app.hovered;
            let catalog = &app.catalog;
            let new_type = &mut app.new_type_input;
            let file = &mut app.file;
            for seg in &file.segments {
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
                let review = file.reviews.as_ref().and_then(|r| r.iter().find(|r| r.id == seg.id)).map(|r| &r.review);
                let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let job = review_ui::highlight_job(buf.as_str(), review, seg.id, hovered, catalog, tk, FontId::proportional(font_size), wrap);
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let mut text: &str = &seg.text;
                let out = TextEdit::multiline(&mut text)
                    .id_salt(("segment", seg.id))
                    .frame(egui::Frame::NONE)
                    .margin(Margin::same(2))
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .layouter(&mut layouter)
                    .show(ui);
                if let Some(cr) = out.cursor_range {
                    let sel = selection_span(&seg.text, cr.primary.index.0, cr.secondary.index.0).and_then(|s| trim_span(&seg.text, s));
                    if let Some(s) = sel {
                        file.selection = Some((seg.id, s));
                    } else if out.response.has_focus() {
                        file.selection = None;
                    }
                }
                let sel = file.selection.filter(|(id, s)| *id == seg.id && s.is_valid_in(&seg.text));
                if let Some((_, s)) = sel {
                    let ci = crate::text_util::byte_to_char(&seg.text, s.start);
                    let r = out.galley.pos_from_cursor(egui::text::CCursor::new(ci)).translate(out.galley_pos.to_vec2());
                    if ui.clip_rect().contains(r.left_top()) {
                        bar_anchor = Some((seg.id, s, seg.text[s.range()].to_string(), r.left_top()));
                    }
                }
                out.response.context_menu(|ui| match sel {
                    Some((_, s)) => {
                        let term = seg.text[s.range()].to_string();
                        let label = format!("Redact “{}” as", crate::text_util::preview(&term, 24));
                        if let Some(t) = review_ui::type_submenu(ui, &label, &menu_types, catalog, new_type) {
                            choice = Some((term, t));
                        }
                    }
                    None => {
                        ui.label(RichText::new("Select text to redact it").color(tk.text_faint));
                    }
                });
                ui.add_space(4.0);
            }
        });
    });
    if let Some((seg, span, text, pos)) = bar_anchor
        && !crate::redact_bar::suppressed(ui.ctx())
    {
        app.redact_bar(ui.ctx(), Workspace::File, seg, span, &text, pos);
    }
    if let Some((term, t)) = choice {
        app.add_term_rule(&term, &t);
    }
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
        let mut ops = Vec::new();
        let has_pages = app.file.page_count > 0;
        match &app.file.reviews {
            None => empty_state(
                ui,
                tk,
                if app.file.detecting.is_some() { "Detecting…" } else if app.file.summary.is_some() { "Saved" } else { "No Review yet" },
                "Detect runs over every part of the Document: body, headers, footers, footnotes and comments.",
            ),
            Some(reviews) => {
                let labels = review_ui::placeholder_labels(reviews);
                let groups = review_ui::group_by_page(reviews, &app.file.placements);
                ScrollArea::vertical().id_salt("file_review").max_height(ui.available_height() - summary_h).auto_shrink([false, false]).show(ui, |ui| {
                    if groups.is_empty() {
                        empty_state(ui, tk, "Nothing above the Threshold", "Lower the Threshold to see more Candidates.");
                    }
                    for g in &groups {
                        let title = match g.page {
                            Some(p) => format!("Page {}", p + 1),
                            None if has_pages => "Unplaced".to_string(),
                            None => "Findings".to_string(),
                        };
                        egui::CollapsingHeader::new(RichText::new(format!("{title}  ·  {}", g.items.len())).strong().color(tk.text_dim))
                            .id_salt(("page_group", g.page))
                            .default_open(true)
                            .show(ui, |ui| {
                                ops.extend(review_ui::candidate_rows(ui, reviews, &g.items, &labels, &types, &catalog, tk, &mut hovered, &mut app.new_type_input));
                            });
                    }
                });
            }
        }
        if hovered.is_some() || !ui.ui_contains_pointer() {
            app.hovered = hovered;
        }
        if let Some(s) = &app.file.summary {
            ui.separator();
            caption(ui, tk, "Summary");
            review_ui::summary_view(ui, s, &app.catalog, tk);
        }
        app.apply_review_ops(Workspace::File, ops);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_do_not_overlap_and_stay_legible() {
        assert_eq!(label_size(0.5), 9.0);
        assert_eq!(label_size(1.0), 10.0);
        assert!(label_size(4.0) <= 14.0);
        let a = Rect::from_min_size(egui::pos2(10.0, 50.0), egui::vec2(40.0, 12.0));
        let size = egui::vec2(30.0, 10.0);
        let first = place_label(a, size, &[]);
        assert_eq!(first.bottom(), a.top() - 1.0);
        let second = place_label(a, size, &[first]);
        assert!(!second.intersects(first));
    }
}
