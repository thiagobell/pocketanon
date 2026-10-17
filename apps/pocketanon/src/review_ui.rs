//! The Review model shared by both workspaces, and its widgets: highlighted text, the
//! candidate list, and the Summary.

use egui::text::{LayoutJob, TextFormat};
use std::collections::HashMap;

use egui::{Color32, FontId, RichText, ScrollArea, Stroke, Ui};
use pocketanon_core::{
    CandidateId, DetectorKind, EntityType, Indexer, Placement, Replacement, Review, ReviewItem, SegmentId, SegmentKind, Span,
    Strategy, Summary, TypeCatalog, plan_replacements,
};

use crate::text_util::preview;
use crate::theme::{Tokens, highlight_fill, type_color};
use crate::widgets::chip;

/// One segment under Review: the exact text detectors saw, plus its Review.
#[derive(Clone, Debug)]
pub struct SegmentReview {
    pub id: SegmentId,
    pub kind: SegmentKind,
    pub text: String,
    pub review: Review,
}

/// A candidate the pointer is over, in the list or in the text.
pub type Hovered = Option<(SegmentId, CandidateId)>;

/// The only Replacement Strategy the app offers.
pub const STRATEGY: Strategy = Strategy::IndexedRedaction;

/// Plan replacements for every segment with ONE shared Indexer, so numbering is consistent
/// across the whole input (body, headers, footnotes, …).
pub fn plan_all(reviews: &[SegmentReview]) -> (Vec<(SegmentId, Vec<Replacement>)>, Summary) {
    let mut indexer = Indexer::new(STRATEGY);
    let mut summary = Summary::default();
    let mut out = Vec::new();
    for r in reviews {
        let (reps, s) = plan_replacements(&r.text, &r.review.entities(), &mut indexer);
        summary.merge_from(&s);
        if !reps.is_empty() {
            out.push((r.id, reps));
        }
    }
    (out, summary)
}

pub fn set_threshold(reviews: &mut [SegmentReview], t: f32) {
    for r in reviews {
        r.review.set_threshold(t);
    }
}

pub fn visible_count(reviews: &[SegmentReview]) -> usize {
    reviews.iter().map(|r| r.review.visible().count()).sum()
}

pub fn hidden_count(reviews: &[SegmentReview]) -> usize {
    reviews.iter().map(|r| r.review.items().len() - r.review.visible().count()).sum()
}

/// `text` as a LayoutJob with every visible candidate highlighted in its Type Group colour.
/// Rejected candidates are struck through; the hovered one is emphasised.
#[allow(clippy::too_many_arguments)]
pub fn highlight_job(
    text: &str,
    review: Option<&Review>,
    seg: SegmentId,
    hovered: Hovered,
    catalog: &TypeCatalog,
    tk: &Tokens,
    font: FontId,
    wrap_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let plain = TextFormat { font_id: font.clone(), color: tk.text, ..Default::default() };
    let mut cursor = 0;
    if let Some(review) = review {
        for item in review.visible() {
            let span = item.candidate.span;
            if span.start < cursor || !span.is_valid_in(text) {
                continue;
            }
            push(&mut job, &text[cursor..span.start], plain.clone());
            let c = type_color(catalog, &item.candidate.entity_type);
            let is_hover = hovered == Some((seg, item.id));
            let mut f = TextFormat { font_id: font.clone(), ..Default::default() };
            if item.accepted {
                f.color = tk.text;
                f.background = if is_hover { c.gamma_multiply(0.75) } else { highlight_fill(c, tk.dark) };
                f.underline = Stroke::new(1.5, c);
            } else {
                f.color = tk.text_faint;
                f.strikethrough = Stroke::new(1.0, tk.text_faint);
                f.background = if is_hover { highlight_fill(c, tk.dark) } else { Color32::TRANSPARENT };
            }
            push(&mut job, &text[span.range()], f);
            cursor = span.end;
        }
    }
    push(&mut job, &text[cursor..], plain);
    job
}

fn push(job: &mut LayoutJob, s: &str, f: TextFormat) {
    if !s.is_empty() {
        job.append(s, 0.0, f);
    }
}

pub fn detector_label(k: DetectorKind) -> &'static str {
    match k {
        DetectorKind::Model => "model",
        DetectorKind::Rule => "rule",
        DetectorKind::Manual => "manual",
    }
}

/// A change the user asked for from a row or a context menu; applied by the workspace.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewOp {
    SetType(SegmentId, CandidateId, EntityType),
    SetAccepted(SegmentId, CandidateId, bool),
    Remove(SegmentId, CandidateId),
    /// Add a Term Rule (and re-run detection).
    AddTerm(String, EntityType),
}

/// Apply the review-local ops; returns the Term Rules to add. `true` in the second slot when the
/// review changed.
pub fn apply_ops(reviews: &mut [SegmentReview], ops: Vec<ReviewOp>) -> (Vec<(String, EntityType)>, bool) {
    let mut terms = Vec::new();
    let mut changed = false;
    for op in ops {
        let seg_of = |reviews: &mut [SegmentReview], s: SegmentId| reviews.iter_mut().position(|r| r.id == s);
        match op {
            ReviewOp::SetType(s, id, t) => {
                if let Some(i) = seg_of(reviews, s) {
                    changed |= reviews[i].review.set_type(id, t);
                }
            }
            ReviewOp::SetAccepted(s, id, a) => {
                if let Some(i) = seg_of(reviews, s) {
                    changed |= reviews[i].review.set_accepted(id, a);
                }
            }
            ReviewOp::Remove(s, id) => {
                if let Some(i) = seg_of(reviews, s) {
                    changed |= reviews[i].review.remove(id);
                }
            }
            ReviewOp::AddTerm(term, t) => terms.push((term, t)),
        }
    }
    (terms, changed)
}

/// The placeholder each accepted, visible candidate will become, without brackets
/// (`PERSON_1`), computed with ONE shared Indexer exactly like the Result.
pub fn placeholder_labels(reviews: &[SegmentReview]) -> HashMap<(SegmentId, CandidateId), String> {
    let (plan, _) = plan_all(reviews);
    let mut by_span: HashMap<(SegmentId, Span), String> = HashMap::new();
    for (seg, reps) in plan {
        for r in reps {
            by_span.insert((seg, r.span), r.with.trim_start_matches('[').trim_end_matches(']').to_string());
        }
    }
    let mut out = HashMap::new();
    for r in reviews {
        for item in r.review.visible().filter(|i| i.accepted) {
            if let Some(l) = by_span.get(&(r.id, item.candidate.span)) {
                out.insert((r.id, item.id), l.clone());
            }
        }
    }
    out
}

/// The tag drawn next to a highlight: the placeholder for an accepted candidate, the plain type
/// name for a rejected one (drawn dimmed and struck through).
pub fn candidate_label(labels: &HashMap<(SegmentId, CandidateId), String>, seg: SegmentId, item: &ReviewItem) -> String {
    if item.accepted
        && let Some(l) = labels.get(&(seg, item.id))
    {
        return l.clone();
    }
    item.candidate.entity_type.as_str().to_string()
}

/// One block of the File Review list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageGroup {
    /// `None`: candidates without a placement yet ("Unplaced"), or a format without pages.
    pub page: Option<usize>,
    pub items: Vec<(SegmentId, CandidateId)>,
}

/// Group visible candidates by the page of their first placement. Within a page: body
/// candidates first in document order, then header/footer/footnote/… ones (also in document
/// order). Candidates with no placement come last, in one `page: None` group.
pub fn group_by_page(reviews: &[SegmentReview], placements: &HashMap<(SegmentId, Span), Vec<Placement>>) -> Vec<PageGroup> {
    /// (page, non-body?, segment order, span start)
    type Key = (usize, bool, usize, usize);
    let mut keyed: Vec<(Key, Option<usize>, (SegmentId, CandidateId))> = Vec::new();
    for (order, r) in reviews.iter().enumerate() {
        for item in r.review.visible() {
            let page = placements.get(&(r.id, item.candidate.span)).and_then(|p| p.iter().map(|p| p.page).min());
            let key = (page.unwrap_or(usize::MAX), r.kind != SegmentKind::Body, order, item.candidate.span.start);
            keyed.push((key, page, (r.id, item.id)));
        }
    }
    keyed.sort_by_key(|k| k.0);
    let mut out: Vec<PageGroup> = Vec::new();
    for (_, page, it) in keyed {
        match out.last_mut() {
            Some(g) if g.page == page => g.items.push(it),
            _ => out.push(PageGroup { page, items: vec![it] }),
        }
    }
    out
}

/// A submenu listing `types` plus a "New type…" field. Returns the chosen type.
pub fn type_submenu(
    ui: &mut Ui,
    label: &str,
    types: &[EntityType],
    catalog: &TypeCatalog,
    new_type: &mut String,
) -> Option<EntityType> {
    let mut chosen = None;
    ui.menu_button(label, |ui| {
        ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for t in types {
                if ui.button(RichText::new(t.as_str()).color(type_color(catalog, t))).clicked() {
                    chosen = Some(t.clone());
                    ui.close();
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(new_type).hint_text("New type…").desired_width(130.0));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || enter) && !new_type.trim().is_empty() {
                chosen = Some(EntityType::new(new_type.as_str()));
                new_type.clear();
                ui.close();
            }
        });
    });
    chosen
}

/// The context menu for one candidate: change type, reject/accept, always redact its text.
pub fn candidate_menu(
    ui: &mut Ui,
    seg: &SegmentReview,
    item: &ReviewItem,
    types: &[EntityType],
    catalog: &TypeCatalog,
    new_type: &mut String,
) -> Option<ReviewOp> {
    let mut op = None;
    let text = item.candidate.text(&seg.text).to_string();
    if let Some(t) = type_submenu(ui, "Change type", types, catalog, new_type) {
        op = Some(ReviewOp::SetType(seg.id, item.id, t));
    }
    let toggle = if item.accepted { "Reject" } else { "Accept" };
    if ui.button(toggle).clicked() {
        op = Some(ReviewOp::SetAccepted(seg.id, item.id, !item.accepted));
        ui.close();
    }
    if let Some(t) = type_submenu(ui, &format!("Always redact “{}” as", preview(&text, 24)), types, catalog, new_type) {
        op = Some(ReviewOp::AddTerm(text, t));
    }
    op
}

/// The candidate rows for `items` (from [`group_by_page`] or all visible ones). Non-body
/// candidates get a small kind tag.
#[allow(clippy::too_many_arguments)]
pub fn candidate_rows(
    ui: &mut Ui,
    reviews: &[SegmentReview],
    items: &[(SegmentId, CandidateId)],
    labels: &HashMap<(SegmentId, CandidateId), String>,
    types: &[EntityType],
    catalog: &TypeCatalog,
    tk: &Tokens,
    hovered: &mut Hovered,
    new_type: &mut String,
) -> Vec<ReviewOp> {
    let mut ops = Vec::new();
    for &(sid, id) in items {
        let Some(seg) = reviews.iter().find(|r| r.id == sid) else { continue };
        let Some(item) = seg.review.get(id) else { continue };
        let c = &item.candidate;
        let color = type_color(catalog, &c.entity_type);
        let row_id = ui.id().with(("cand", sid, id));
        let is_hover = *hovered == Some((sid, id));
        let frame = egui::Frame::new()
            .fill(if is_hover { tk.card_hover } else { tk.input })
            .stroke(Stroke::new(1.0, if is_hover { color.gamma_multiply(0.6) } else { tk.border }))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(8, 6));
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut accepted = item.accepted;
                    if ui.checkbox(&mut accepted, "").on_hover_text("Accept: replace this Entity").changed() {
                        ops.push(ReviewOp::SetAccepted(sid, id, accepted));
                    }
                    let text = preview(c.text(&seg.text), 24);
                    let rich = if accepted {
                        RichText::new(text).color(tk.text).strong()
                    } else {
                        RichText::new(text).color(tk.text_faint).strikethrough()
                    };
                    ui.add(egui::Label::new(rich).truncate()).on_hover_text(c.text(&seg.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if c.detector == DetectorKind::Manual
                            && ui.small_button("🗙").on_hover_text("Remove this manual Entity").clicked()
                        {
                            ops.push(ReviewOp::Remove(sid, id));
                        }
                        if c.detector != DetectorKind::Manual {
                            ui.label(RichText::new(format!("{:.2}", c.confidence)).monospace().size(11.0).color(tk.text_dim))
                                .on_hover_text("Confidence");
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    egui::ComboBox::from_id_salt(row_id.with("type"))
                        .selected_text(RichText::new(c.entity_type.as_str()).color(color).size(12.0))
                        .height(320.0)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for t in types {
                                let tc = type_color(catalog, t);
                                if ui.selectable_label(*t == c.entity_type, RichText::new(t.as_str()).color(tc)).clicked()
                                    && *t != c.entity_type
                                {
                                    ops.push(ReviewOp::SetType(sid, id, t.clone()));
                                }
                            }
                        })
                        .response
                        .on_hover_text("Change the Entity Type");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(detector_label(c.detector)).size(11.0).color(tk.text_faint));
                        if seg.kind != SegmentKind::Body {
                            chip(ui, &seg.kind.to_string(), tk.text_dim, tk.dark);
                        }
                        let label = candidate_label(labels, sid, item);
                        if item.accepted {
                            ui.label(RichText::new(label).monospace().size(10.5).color(color));
                        }
                    });
                });
            })
            .response;
        let resp = resp.interact(egui::Sense::click());
        if resp.hovered() || resp.contains_pointer() {
            *hovered = Some((sid, id));
        }
        resp.context_menu(|ui| {
            if let Some(op) = candidate_menu(ui, seg, item, types, catalog, new_type) {
                ops.push(op);
            }
        });
        ui.add_space(2.0);
    }
    ops
}

/// Every visible candidate in document order (the Text workspace list).
pub fn all_visible(reviews: &[SegmentReview]) -> Vec<(SegmentId, CandidateId)> {
    reviews.iter().flat_map(|r| r.review.visible().map(move |i| (r.id, i.id))).collect()
}

/// Summary: total plus one chip per Entity Type.
pub fn summary_view(ui: &mut Ui, summary: &Summary, catalog: &TypeCatalog, tk: &Tokens) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}", summary.total())).size(22.0).strong().color(tk.text));
        ui.label(RichText::new(if summary.total() == 1 { "Entity replaced" } else { "Entities replaced" }).color(tk.text_dim));
    });
    ui.horizontal_wrapped(|ui| {
        for (t, n) in &summary.counts {
            chip(ui, &format!("{}  {}", t.as_str(), n), type_color(catalog, t), tk.dark);
        }
    });
}

/// Every candidate span of every review (visible or not), for document placement lookups.
pub fn all_spans(reviews: &[SegmentReview]) -> Vec<(SegmentId, Span)> {
    reviews.iter().flat_map(|r| r.review.items().iter().map(move |i| (r.id, i.candidate.span))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketanon_core::Candidate;

    fn seg(id: usize, text: &str, cands: Vec<(usize, usize, &str)>) -> SegmentReview {
        let c = cands
            .into_iter()
            .map(|(s, e, t)| Candidate::new(Span::new(s, e), t, 0.9, DetectorKind::Model))
            .collect();
        SegmentReview { id: SegmentId(id), kind: SegmentKind::Body, text: text.into(), review: Review::new(c, 0.5) }
    }

    #[test]
    fn indices_are_shared_across_segments() {
        let reviews = vec![seg(0, "Anna Muster", vec![(0, 11, "person")]), seg(1, "Bob und Anna Muster", vec![(0, 3, "person"), (8, 19, "person")])];
        let (plan, summary) = plan_all(&reviews);
        assert_eq!(plan[0].1[0].with, "[PERSON_1]");
        assert_eq!(plan[1].1[0].with, "[PERSON_2]");
        assert_eq!(plan[1].1[1].with, "[PERSON_1]");
        assert_eq!(summary.total(), 3);
    }

    #[test]
    fn labels_follow_the_shared_indexer() {
        let mut reviews = vec![seg(0, "Anna Muster", vec![(0, 11, "person")]), seg(1, "Bob und Anna Muster", vec![(0, 3, "person"), (8, 19, "person")])];
        let labels = placeholder_labels(&reviews);
        let item = |r: &[SegmentReview], s: usize, i: usize| r[s].review.items()[i].clone();
        assert_eq!(candidate_label(&labels, SegmentId(0), &item(&reviews, 0, 0)), "PERSON_1");
        assert_eq!(candidate_label(&labels, SegmentId(1), &item(&reviews, 1, 0)), "PERSON_2");
        assert_eq!(candidate_label(&labels, SegmentId(1), &item(&reviews, 1, 1)), "PERSON_1");
        // Rejecting Anna in segment 0 renumbers: Bob is now PERSON_1, the rejected one shows its type.
        let id = reviews[0].review.items()[0].id;
        reviews[0].review.set_accepted(id, false);
        let labels = placeholder_labels(&reviews);
        assert_eq!(candidate_label(&labels, SegmentId(0), &item(&reviews, 0, 0)), "person");
        assert_eq!(candidate_label(&labels, SegmentId(1), &item(&reviews, 1, 0)), "PERSON_1");
    }

    #[test]
    fn grouping_by_page_puts_footers_last_and_unplaced_at_the_end() {
        use pocketanon_core::Rect;
        let mut footer = seg(0, "Max Footer", vec![(0, 3, "person")]);
        footer.kind = SegmentKind::Footer;
        let reviews = vec![
            footer,                                                     // seg 0: footer, page 0
            seg(1, "Anna and Bob", vec![(0, 4, "person"), (9, 12, "person")]), // body, page 0 / unplaced
            seg(2, "Carl", vec![(0, 4, "person")]),                     // body, page 1
        ];
        let rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        let at = |page| vec![Placement { page, rects: vec![rect] }];
        let mut pl = HashMap::new();
        pl.insert((SegmentId(0), Span::new(0, 3)), at(0));
        pl.insert((SegmentId(1), Span::new(0, 4)), at(0));
        pl.insert((SegmentId(2), Span::new(0, 4)), [at(3), at(1)].concat());
        let groups = group_by_page(&reviews, &pl);
        let pages: Vec<_> = groups.iter().map(|g| g.page).collect();
        assert_eq!(pages, vec![Some(0), Some(1), None]);
        // Page 0: body Anna first, footer Max last.
        let ids = |g: &PageGroup| g.items.iter().map(|(s, _)| s.0).collect::<Vec<_>>();
        assert_eq!(ids(&groups[0]), vec![1, 0]);
        assert_eq!(ids(&groups[1]), vec![2]);
        assert_eq!(ids(&groups[2]), vec![1]); // Bob, unplaced
    }

    #[test]
    fn ops_apply_and_collect_terms() {
        let mut reviews = vec![seg(0, "Anna", vec![(0, 4, "person")])];
        let id = reviews[0].review.items()[0].id;
        let (terms, changed) = apply_ops(
            &mut reviews,
            vec![ReviewOp::SetType(SegmentId(0), id, EntityType::new("organization")), ReviewOp::AddTerm("Anna".into(), EntityType::new("person"))],
        );
        assert!(changed);
        assert_eq!(terms, vec![("Anna".to_string(), EntityType::new("person"))]);
        assert_eq!(reviews[0].review.entities()[0].entity_type.as_str(), "organization");
    }

    #[test]
    fn highlight_job_covers_text_exactly() {
        let r = seg(0, "Grüße an Jürgen", vec![(11, 18, "person")]);
        let job = highlight_job(&r.text, Some(&r.review), r.id, None, &TypeCatalog::builtin(), &Tokens::dark(), FontId::default(), 100.0);
        assert_eq!(job.text, r.text);
        assert_eq!(job.sections.len(), 2, "plain prefix + highlighted name");
    }
}
