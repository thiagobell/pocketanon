//! The floating "Redact" toolbar shown on a text selection (Text editor, document pages and
//! segment text). Its menu lists the most common Entity Types of this input first, then every
//! type in its Type Group.

use std::collections::HashMap;

use egui::{Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke};
use pocketanon_core::{EntityType, TypeCatalog};

use crate::review_ui::SegmentReview;
use crate::theme::{Tokens, group_color, type_color};

/// What the user picked in the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarChoice {
    pub entity_type: EntityType,
    /// Also add a Term Rule, so every occurrence is redacted.
    pub always: bool,
}

/// Per-selection bar state, kept by the workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarState {
    /// The menu is open (the Redact button was clicked).
    pub open: bool,
    pub always: bool,
}

/// The `n` most common Entity Types among the visible Candidates of `reviews` (accepted or
/// not), most frequent first; ties in catalog order, then alphabetically.
pub fn top_types(reviews: Option<&[SegmentReview]>, catalog: &TypeCatalog, n: usize) -> Vec<EntityType> {
    let mut counts: HashMap<EntityType, usize> = HashMap::new();
    for r in reviews.unwrap_or_default() {
        for i in r.review.visible() {
            *counts.entry(i.candidate.entity_type.clone()).or_default() += 1;
        }
    }
    let all = catalog.all_types();
    let rank = |t: &EntityType| all.iter().position(|a| a == t).unwrap_or(usize::MAX);
    let mut v: Vec<(EntityType, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(rank(&a.0).cmp(&rank(&b.0))).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|(t, _)| t).collect()
}

/// While the user is still dragging out a selection the bar stays hidden; a press that starts
/// on the bar itself does not count, or the bar would vanish under its own click.
pub fn suppressed(ctx: &egui::Context) -> bool {
    let bar = ctx.memory(|m| m.area_rect(egui::Id::new("redact_bar")));
    ctx.input(|i| {
        i.pointer.primary_down() && !matches!((bar, i.pointer.press_origin()), (Some(r), Some(p)) if r.contains(p))
    })
}

/// Draw the bar with its top-left at `anchor` (screen space, just above the selection).
/// `preview` is the selected text, shown in the menu header.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ctx: &egui::Context,
    anchor: egui::Pos2,
    preview: &str,
    state: &mut BarState,
    top: &[EntityType],
    custom: &[EntityType],
    catalog: &TypeCatalog,
    tk: &Tokens,
) -> Option<BarChoice> {
    let mut choice = None;
    let screen = ctx.content_rect();
    let pos = egui::pos2(anchor.x.clamp(screen.left() + 4.0, screen.right() - 160.0), (anchor.y - 38.0).max(screen.top() + 4.0));
    egui::Area::new(egui::Id::new("redact_bar")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        Frame::new()
            .fill(tk.menu)
            .stroke(Stroke::new(1.0, tk.border_strong))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(4))
            .shadow(ctx.global_style().visuals.popup_shadow)
            .show(ui, |ui| {
                let b = egui::Button::new(RichText::new("Redact").strong().color(tk.on_accent))
                    .fill(tk.accent)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(6))
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(b).on_hover_text("Redact the selected text").clicked() {
                    state.open = !state.open;
                }
                if !state.open {
                    return;
                }
                ui.set_max_width(260.0);
                ui.add_space(4.0);
                ui.label(RichText::new(format!("“{}” as", crate::text_util::preview(preview, 28))).size(11.5).color(tk.text_dim));
                ScrollArea::vertical().max_height(380.0).min_scrolled_height(340.0).show(ui, |ui| {
                    let mut item = |ui: &mut egui::Ui, t: &EntityType, c: Color32| {
                        let r = ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.0, c);
                            ui.add(egui::Button::new(RichText::new(t.as_str()).color(c)).frame(false))
                        });
                        if r.inner.clicked() {
                            choice = Some(BarChoice { entity_type: t.clone(), always: state.always });
                        }
                    };
                    for t in top {
                        item(ui, t, type_color(catalog, t));
                    }
                    if !top.is_empty() {
                        ui.separator();
                    }
                    for g in &catalog.groups {
                        ui.label(RichText::new(&g.name).size(11.0).strong().color(tk.text_faint));
                        ui.indent(("bar_group", &g.name), |ui| {
                            for t in &g.types {
                                item(ui, t, group_color(Some(&g.name)));
                            }
                        });
                    }
                    let custom: Vec<&EntityType> = custom.iter().filter(|c| !catalog.all_types().contains(c)).collect();
                    if !custom.is_empty() {
                        ui.label(RichText::new("Custom").size(11.0).strong().color(tk.text_faint));
                        ui.indent("bar_custom", |ui| {
                            for t in custom {
                                item(ui, t, group_color(None));
                            }
                        });
                    }
                });
                ui.separator();
                ui.checkbox(&mut state.always, RichText::new("Always redact this term").size(12.0))
                    .on_hover_text("Also add a Term Rule, so every occurrence is redacted");
            });
    });
    if choice.is_some() {
        *state = BarState::default();
    }
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketanon_core::{Candidate, DetectorKind, Review, SegmentId, SegmentKind, Span};

    fn seg(id: usize, cands: &[(usize, &str)]) -> SegmentReview {
        let c = cands.iter().map(|(s, t)| Candidate::new(Span::new(*s, s + 1), *t, 0.9, DetectorKind::Model)).collect();
        SegmentReview::new(SegmentId(id), SegmentKind::Body, "x".repeat(40), Review::new(c, 0.5))
    }

    #[test]
    fn most_common_types_first_ties_in_catalog_order() {
        let c = TypeCatalog::builtin();
        let reviews = vec![
            seg(0, &[(0, "email"), (2, "person"), (4, "person"), (6, "iban")]),
            seg(1, &[(0, "person"), (2, "email"), (4, "location"), (6, "url"), (8, "project code name")]),
        ];
        let top = top_types(Some(&reviews), &c, 4);
        let names: Vec<&str> = top.iter().map(|t| t.as_str()).collect();
        // person 3, email 2, then ties at 1 in catalog order: location before url/iban.
        assert_eq!(names, vec!["person", "email", "location", "url"]);
        assert!(top_types(None, &c, 4).is_empty());
    }
}
