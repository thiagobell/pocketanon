//! User settings, persisted as JSON in the platform config dir.
//!
//! ADR-0003: settings persist, content never does. This struct must never gain a field that
//! could hold input text, file paths of processed documents, Summaries or the Mapping. The
//! `json_keys_are_allowlisted` test pins the exact set of keys.

#[cfg(test)]
use std::collections::BTreeSet;
use std::path::PathBuf;

use pocketanon_core::{EntityType, Language, TypeCatalog};
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

/// The Languages offered in the settings drawer.
pub const LANGUAGES: [(&str, &str); 7] = [
    ("de", "German"),
    ("en", "English"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
];

/// Which Model the Model Detector uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum ModelChoice {
    /// A Registry entry, by id.
    Registry(String),
    /// A user-supplied model folder (a model location, not content).
    Folder(PathBuf),
}

/// A user-defined Term Rule: a literal term always redacted as `entity_type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermRule {
    pub term: String,
    pub entity_type: EntityType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub languages: Vec<Language>,
    /// Built-in Type Groups that are on, by name.
    pub enabled_groups: Vec<String>,
    /// Built-in types switched off individually inside an enabled group.
    pub disabled_types: Vec<EntityType>,
    pub custom_types: Vec<EntityType>,
    pub threshold: f32,
    pub model: Option<ModelChoice>,
    pub theme: Theme,
    /// ADR-0003: user-authored configuration (terms the user chose to always redact), not
    /// document content; it is never filled from a document without an explicit user action.
    pub term_rules: Vec<TermRule>,
    /// Whether the Legend panel is open.
    pub legend_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let catalog = TypeCatalog::builtin();
        Settings {
            languages: vec![Language::german(), Language::english()],
            enabled_groups: catalog.groups.iter().filter(|g| g.default_enabled).map(|g| g.name.clone()).collect(),
            disabled_types: Vec::new(),
            custom_types: Vec::new(),
            threshold: 0.3,
            model: None,
            theme: Theme::Dark,
            term_rules: Vec::new(),
            legend_open: true,
        }
    }
}

impl Settings {
    pub fn group_enabled(&self, name: &str) -> bool {
        self.enabled_groups.iter().any(|g| g == name)
    }

    pub fn set_group_enabled(&mut self, name: &str, on: bool) {
        self.enabled_groups.retain(|g| g != name);
        if on {
            self.enabled_groups.push(name.to_string());
        }
    }

    pub fn type_enabled(&self, t: &EntityType) -> bool {
        !self.disabled_types.contains(t)
    }

    pub fn set_type_enabled(&mut self, t: &EntityType, on: bool) {
        self.disabled_types.retain(|d| d != t);
        if !on {
            self.disabled_types.push(t.clone());
        }
    }

    pub fn language_enabled(&self, code: &str) -> bool {
        self.languages.iter().any(|l| l.code() == code)
    }

    pub fn set_language(&mut self, code: &str, on: bool) {
        self.languages.retain(|l| l.code() != code);
        if on {
            self.languages.push(Language::new(code));
            self.languages.sort();
        }
    }

    /// Every Entity Type that detection should look for: enabled built-ins plus Custom Types.
    pub fn active_types(&self, catalog: &TypeCatalog) -> Vec<EntityType> {
        let mut out: Vec<EntityType> = catalog
            .groups
            .iter()
            .filter(|g| self.group_enabled(&g.name))
            .flat_map(|g| g.types.iter())
            .filter(|t| self.type_enabled(t))
            .cloned()
            .collect();
        for c in &self.custom_types {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
        out
    }

    /// Types offered in the retype dropdown: all built-ins plus Custom Types.
    pub fn all_types(&self, catalog: &TypeCatalog) -> Vec<EntityType> {
        let mut out = catalog.all_types();
        out.extend(self.custom_types.iter().filter(|c| !catalog.all_types().contains(c)).cloned());
        out
    }

    /// Add (or retype) a Term Rule. The term is trimmed; terms compare case-insensitively.
    /// A type that is not built in becomes a Custom Type. Returns `false` for an empty term.
    pub fn add_term_rule(&mut self, catalog: &TypeCatalog, term: &str, t: &EntityType) -> bool {
        let term: String = term.split_whitespace().collect::<Vec<_>>().join(" ");
        if term.is_empty() || t.as_str().is_empty() {
            return false;
        }
        if !self.all_types(catalog).contains(t) {
            self.custom_types.push(t.clone());
        }
        let key = term.to_lowercase();
        self.term_rules.retain(|r| r.term.to_lowercase() != key);
        self.term_rules.push(TermRule { term, entity_type: t.clone() });
        true
    }

    pub fn remove_term_rule(&mut self, term: &str) {
        self.term_rules.retain(|r| r.term != term);
    }

    pub fn add_custom_type(&mut self, label: &str) -> bool {
        let t = EntityType::new(label);
        if t.as_str().is_empty() || self.custom_types.contains(&t) {
            return false;
        }
        self.custom_types.push(t);
        true
    }
}

/// Reads and writes `settings.json`. `path == None` (tests, unknown platform) keeps everything
/// in memory.
pub struct SettingsStore {
    path: Option<PathBuf>,
}

impl SettingsStore {
    pub fn platform() -> Self {
        let path = directories::ProjectDirs::from("de", "PocketAnon", "PocketAnon")
            .map(|d| d.config_dir().join("settings.json"));
        SettingsStore { path }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        SettingsStore { path: None }
    }

    pub fn load(&self) -> Settings {
        let Some(path) = &self.path else { return Settings::default() };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("settings.json unreadable, using defaults: {e}");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, s: &Settings) -> Result<(), String> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
/// The JSON keys `settings.json` may contain. Adding a key means checking ADR-0003 first.
pub const ALLOWED_KEYS: [&str; 9] = [
    "languages",
    "enabled_groups",
    "disabled_types",
    "custom_types",
    "threshold",
    "model",
    "theme",
    "term_rules",
    "legend_open",
];

#[cfg(test)]
pub fn json_keys(s: &Settings) -> BTreeSet<String> {
    match serde_json::to_value(s) {
        Ok(serde_json::Value::Object(m)) => m.keys().cloned().collect(),
        _ => BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keys_are_allowlisted() {
        let s = Settings { model: Some(ModelChoice::Registry("demo".into())), ..Default::default() };
        let keys = json_keys(&s);
        let allowed: BTreeSet<String> = ALLOWED_KEYS.iter().map(|k| k.to_string()).collect();
        assert_eq!(keys, allowed, "settings.json keys changed; see ADR-0003 before adding any");
        let json = serde_json::to_string(&Settings::default()).unwrap();
        for forbidden in ["input", "text", "output", "recent", "file", "summary", "mapping", "history"] {
            assert!(!json.contains(&format!("\"{forbidden}")), "suspicious key {forbidden} in {json}");
        }
    }

    #[test]
    fn round_trips_and_tolerates_unknown_or_missing_keys() {
        let mut s = Settings::default();
        s.add_custom_type("Project Code Name");
        s.threshold = 0.7;
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: Settings = serde_json::from_str(r#"{"threshold":0.3,"bogus":1}"#).unwrap();
        assert_eq!(partial.threshold, 0.3);
        assert_eq!(partial.languages, Settings::default().languages);
    }

    #[test]
    fn active_types_follow_groups_types_and_custom() {
        let c = TypeCatalog::builtin();
        let mut s = Settings::default();
        assert_eq!(s.active_types(&c), c.default_types());
        s.set_group_enabled("Contact", false);
        assert!(!s.active_types(&c).contains(&EntityType::new("email")));
        s.set_type_enabled(&EntityType::new("iban"), false);
        assert!(!s.active_types(&c).contains(&EntityType::new("iban")));
        assert!(s.add_custom_type("project code name"));
        assert!(!s.add_custom_type(" Project  Code Name"));
        assert!(s.active_types(&c).contains(&EntityType::new("project code name")));
    }

    #[test]
    fn term_rules_dedupe_and_create_custom_types() {
        let c = TypeCatalog::builtin();
        let mut s = Settings::default();
        assert!(s.add_term_rule(&c, " Falke ", &EntityType::new("project code name")));
        assert!(s.custom_types.contains(&EntityType::new("project code name")));
        assert!(s.add_term_rule(&c, "FALKE", &EntityType::new("person")));
        assert_eq!(s.term_rules, vec![TermRule { term: "FALKE".into(), entity_type: EntityType::new("person") }]);
        assert!(!s.add_term_rule(&c, "   ", &EntityType::new("person")));
        s.remove_term_rule("FALKE");
        assert!(s.term_rules.is_empty());
        // An old settings.json with the removed `strategy` key still loads.
        let old: Settings = serde_json::from_str(r#"{"strategy":"redaction","threshold":0.4}"#).unwrap();
        assert_eq!(old.threshold, 0.4);
        assert!(old.legend_open);
    }
}
