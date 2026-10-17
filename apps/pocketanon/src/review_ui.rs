//! The Review model shared by both workspaces, and its widgets: highlighted text, the
//! candidate list, and the Summary.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, RichText, Stroke, Ui};
use pocketanon_core::{
    CandidateId, DetectorKind, EntityType, Indexer, Replacement, Review, SegmentId, SegmentKind, Span, Strategy, Summary,
    TypeCatalog, plan_replacements,
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

/// Plan replacements for every segment with ONE shared Indexer, so numbering is consistent
/// across the whole input (body, headers, footnotes, …).
pub fn plan_all(reviews: &[SegmentReview], strategy: Strategy) -> (Vec<(SegmentId, Vec<Replacement>)>, Summary) {
    let mut indexer = Indexer::new(strategy);
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

/// The candidate rows for the given reviews (optionally only segments of one kind).
/// Returns `true` when the user changed anything.
pub fn candidate_rows(
    ui: &mut Ui,
    reviews: &mut [SegmentReview],
    kind: Option<SegmentKind>,
    types: &[EntityType],
    catalog: &TypeCatalog,
    tk: &Tokens,
    hovered: &mut Hovered,
) -> bool {
    let mut changed = false;
    for seg in reviews.iter_mut().filter(|r| kind.is_none_or(|k| r.kind == k)) {
        let ids: Vec<CandidateId> = seg.review.visible().map(|i| i.id).collect();
        for id in ids {
            let Some(item) = seg.review.get(id).cloned() else { continue };
            let c = &item.candidate;
            let color = type_color(catalog, &c.entity_type);
            let row_id = ui.id().with(("cand", seg.id, id));
            let is_hover = *hovered == Some((seg.id, id));
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
                            seg.review.set_accepted(id, accepted);
                            changed = true;
                        }
                        let text = preview(c.text(&seg.text), 28);
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
                                seg.review.remove(id);
                                changed = true;
                            }
                            ui.label(
                                RichText::new(if c.detector == DetectorKind::Manual {
                                    String::new()
                                } else {
                                    format!("{:.2}", c.confidence)
                                })
                                .monospace()
                                .size(11.0)
                                .color(tk.text_dim),
                            )
                            .on_hover_text("Confidence");
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(26.0);
                        egui::ComboBox::from_id_salt(row_id.with("type"))
                            .selected_text(RichText::new(c.entity_type.as_str()).color(color).size(12.0))
                            .height(320.0)
                            .show_ui(ui, |ui| {
                                for t in types {
                                    let tc = type_color(catalog, t);
                                    if ui
                                        .selectable_label(*t == c.entity_type, RichText::new(t.as_str()).color(tc))
                                        .clicked()
                                        && *t != c.entity_type
                                    {
                                        seg.review.set_type(id, t.clone());
                                        changed = true;
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Change the Entity Type");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(detector_label(c.detector)).size(11.0).color(tk.text_faint));
                        });
                    });
                })
                .response;
            if resp.hovered() || resp.contains_pointer() {
                *hovered = Some((seg.id, id));
            }
            ui.add_space(2.0);
        }
    }
    changed
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
        let (plan, summary) = plan_all(&reviews, Strategy::IndexedRedaction);
        assert_eq!(plan[0].1[0].with, "[PERSON_1]");
        assert_eq!(plan[1].1[0].with, "[PERSON_2]");
        assert_eq!(plan[1].1[1].with, "[PERSON_1]");
        assert_eq!(summary.total(), 3);
    }

    #[test]
    fn highlight_job_covers_text_exactly() {
        let r = seg(0, "Grüße an Jürgen", vec![(11, 18, "person")]);
        let job = highlight_job(&r.text, Some(&r.review), r.id, None, &TypeCatalog::builtin(), &Tokens::dark(), FontId::default(), 100.0);
        assert_eq!(job.text, r.text);
        assert_eq!(job.sections.len(), 2, "plain prefix + highlighted name");
    }
}
