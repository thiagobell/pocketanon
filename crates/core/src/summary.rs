//! Counts of replaced entities per type, shown in the UI after a run. Never persisted.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::types::EntityType;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub counts: BTreeMap<EntityType, usize>,
}

impl Summary {
    pub fn record(&mut self, t: &EntityType) {
        *self.counts.entry(t.clone()).or_insert(0) += 1;
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Fold another summary into this one (e.g. one per paragraph of a document).
    pub fn merge_from(&mut self, other: &Summary) {
        for (t, n) in &other.counts {
            *self.counts.entry(t.clone()).or_insert(0) += n;
        }
    }
}
