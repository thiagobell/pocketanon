//! Built-in entity types, grouped into type groups the user toggles together.

use serde::{Deserialize, Serialize};

use crate::types::EntityType;

/// A named cluster of entity types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeGroup {
    pub name: String,
    pub types: Vec<EntityType>,
    /// Whether the group is active in a fresh installation.
    pub default_enabled: bool,
}

impl TypeGroup {
    pub fn new(name: &str, types: &[&str], default_enabled: bool) -> Self {
        TypeGroup {
            name: name.to_string(),
            types: types.iter().map(EntityType::new).collect(),
            default_enabled,
        }
    }
}

/// The built-in groups. Labels are phrased the way GLiNER PII models were trained on them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeCatalog {
    pub groups: Vec<TypeGroup>,
}

impl TypeCatalog {
    pub fn builtin() -> Self {
        TypeCatalog {
            groups: vec![
                TypeGroup::new(
                    "Identity",
                    &["person", "organization", "location", "address", "date of birth"],
                    true,
                ),
                TypeGroup::new("Contact", &["email", "phone number", "url"], true),
                TypeGroup::new(
                    "Identifiers",
                    &[
                        "iban",
                        "credit card number",
                        "passport number",
                        "national id number",
                        "tax number",
                        "customer number",
                        "case number",
                    ],
                    true,
                ),
                TypeGroup::new(
                    "Sensitive",
                    &["health condition", "religion", "political affiliation", "ethnicity"],
                    false,
                ),
            ],
        }
    }

    pub fn group(&self, name: &str) -> Option<&TypeGroup> {
        self.groups.iter().find(|g| g.name == name)
    }

    /// The group an entity type belongs to, if any.
    pub fn group_of(&self, t: &EntityType) -> Option<&TypeGroup> {
        self.groups.iter().find(|g| g.types.contains(t))
    }

    /// Every type in every group, in catalog order.
    pub fn all_types(&self) -> Vec<EntityType> {
        self.groups.iter().flat_map(|g| g.types.iter().cloned()).collect()
    }

    /// Types active in a fresh installation.
    pub fn default_types(&self) -> Vec<EntityType> {
        self.groups.iter().filter(|g| g.default_enabled).flat_map(|g| g.types.iter().cloned()).collect()
    }
}

impl Default for TypeCatalog {
    fn default() -> Self {
        TypeCatalog::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_is_off_by_default() {
        let c = TypeCatalog::builtin();
        let d = c.default_types();
        assert!(d.contains(&EntityType::new("person")));
        assert!(d.contains(&EntityType::new("iban")));
        assert!(!d.contains(&EntityType::new("religion")));
        assert_eq!(c.group_of(&EntityType::new("Email")).unwrap().name, "Contact");
    }

    #[test]
    fn no_duplicate_types_across_groups() {
        let all = TypeCatalog::builtin().all_types();
        let mut dedup = all.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(all.len(), dedup.len());
    }
}
