//! The review state between detection and replacement.
//!
//! Holds the merged candidates for one input plus the user's decisions: a confidence threshold,
//! per-candidate rejections and type changes, and manually added spans. Produces the final
//! [`Entity`] list.

use serde::{Deserialize, Serialize};

use crate::detector::DetectorKind;
use crate::merge::merge;
use crate::types::{Candidate, Entity, EntityType, Span};

/// Stable identifier of a candidate inside one [`Review`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CandidateId(pub usize);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub id: CandidateId,
    pub candidate: Candidate,
    /// `false` when the user rejected it.
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Review {
    items: Vec<ReviewItem>,
    threshold: f32,
    next_id: usize,
}

impl Review {
    /// Start a review from merged candidates. `threshold` hides low-confidence candidates.
    pub fn new(candidates: Vec<Candidate>, threshold: f32) -> Self {
        let mut r = Review { items: Vec::new(), threshold: threshold.clamp(0.0, 1.0), next_id: 0 };
        for c in merge(candidates) {
            r.push(c);
        }
        r
    }

    fn push(&mut self, candidate: Candidate) -> CandidateId {
        let id = CandidateId(self.next_id);
        self.next_id += 1;
        self.items.push(ReviewItem { id, candidate, accepted: true });
        self.items.sort_by_key(|i| i.candidate.span);
        id
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    pub fn set_threshold(&mut self, t: f32) {
        self.threshold = t.clamp(0.0, 1.0);
    }

    /// Every item, including hidden and rejected ones (for the UI list).
    pub fn items(&self) -> &[ReviewItem] {
        &self.items
    }

    /// Items at or above the threshold; what the UI highlights.
    pub fn visible(&self) -> impl Iterator<Item = &ReviewItem> {
        self.items.iter().filter(|i| self.is_visible(i))
    }

    fn is_visible(&self, i: &ReviewItem) -> bool {
        i.candidate.detector == DetectorKind::Manual || i.candidate.confidence >= self.threshold
    }

    pub fn get(&self, id: CandidateId) -> Option<&ReviewItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn set_accepted(&mut self, id: CandidateId, accepted: bool) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(i) => {
                i.accepted = accepted;
                true
            }
            None => false,
        }
    }

    pub fn set_type(&mut self, id: CandidateId, t: impl Into<EntityType>) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(i) => {
                i.candidate.entity_type = t.into();
                true
            }
            None => false,
        }
    }

    /// Add a span by hand. It wins over anything it overlaps: overlapping items are removed.
    /// Returns `None` if the span is empty or not on char boundaries of `text`.
    pub fn add_manual(&mut self, text: &str, span: Span, t: impl Into<EntityType>) -> Option<CandidateId> {
        if span.is_empty() || !span.is_valid_in(text) {
            return None;
        }
        self.items.retain(|i| !i.candidate.span.overlaps(&span));
        Some(self.push(Candidate::new(span, t, 1.0, DetectorKind::Manual)))
    }

    pub fn remove(&mut self, id: CandidateId) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        before != self.items.len()
    }

    /// Accepted, visible items as entities, in document order.
    pub fn entities(&self) -> Vec<Entity> {
        self.visible().filter(|i| i.accepted).map(|i| Entity::from(&i.candidate)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: usize, e: usize, t: &str, conf: f32) -> Candidate {
        Candidate::new(Span::new(s, e), t, conf, DetectorKind::Model)
    }

    #[test]
    fn threshold_hides_but_keeps_items() {
        let mut r = Review::new(vec![c(0, 3, "person", 0.9), c(5, 8, "person", 0.4)], 0.5);
        assert_eq!(r.entities().len(), 1);
        assert_eq!(r.items().len(), 2);
        r.set_threshold(0.3);
        assert_eq!(r.entities().len(), 2);
    }

    #[test]
    fn reject_and_retype() {
        let mut r = Review::new(vec![c(0, 3, "person", 0.9), c(5, 8, "person", 0.9)], 0.5);
        let ids: Vec<_> = r.items().iter().map(|i| i.id).collect();
        assert!(r.set_accepted(ids[0], false));
        assert!(r.set_type(ids[1], "Organization"));
        let ents = r.entities();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].entity_type.as_str(), "organization");
        assert!(!r.set_accepted(CandidateId(99), true));
    }

    #[test]
    fn manual_addition_overrides_overlaps_and_ignores_threshold() {
        let text = "Projekt FALKE läuft";
        let mut r = Review::new(vec![c(8, 13, "person", 0.3)], 0.9);
        assert_eq!(r.entities().len(), 0);
        let id = r.add_manual(text, Span::new(0, 13), "project code name").unwrap();
        assert_eq!(r.items().len(), 1);
        assert_eq!(r.get(id).unwrap().candidate.detector, DetectorKind::Manual);
        assert_eq!(r.entities()[0].span, Span::new(0, 13));
        assert!(r.add_manual(text, Span::new(2, 2), "x").is_none());
        assert!(r.remove(id));
        assert!(r.entities().is_empty());
    }
}
