//! Production services: the Wordcraft document adapter, the Rule Detector and the GLiNER
//! Model Detector with its Registry and downloader.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pocketanon_core::{
    AnonymizableDocument, Candidate, DetectRequest, Detector, DetectorError, DetectorKind, DocumentError, EntityType,
    Format,
};
use pocketanon_detect_gliner::{GlinerDetector, GlinerOptions, ModelStore, Quantization, Registry};

use super::{DocumentLoader, DownloadProgress, ModelInfo, ModelManager, ModelStatus};

/// `pocketanon-documents` behind the app's loader seam.
pub struct WordcraftLoader;

impl DocumentLoader for WordcraftLoader {
    fn detect_format(&self, path: &Path) -> Option<Format> {
        pocketanon_documents::detect_format(path)
    }
    fn load(&self, bytes: &[u8], format: Format) -> Result<Box<dyn AnonymizableDocument>, DocumentError> {
        pocketanon_documents::load(bytes, format)
    }
    fn output_path(&self, input: &Path) -> PathBuf {
        pocketanon_documents::output_path(input)
    }
}

/// Entity types the Rule Detector already covers with checksums or strict patterns. They are
/// withheld from the Model Detector: fewer labels raise GLiNER's confidence on the rest, and the
/// model mislabels these anyway (see docs/poc-plan.md, findings).
const RULE_OWNED: &[&str] = &["email", "url", "phone number", "iban", "credit card number"];

/// Wraps a Model Detector so it is only asked about the types it is good at.
pub struct ModelTypeFilter<D: Detector> {
    inner: D,
}

impl<D: Detector> ModelTypeFilter<D> {
    pub fn new(inner: D) -> Self {
        ModelTypeFilter { inner }
    }

    pub fn filtered_types(types: &[EntityType]) -> Vec<EntityType> {
        let owned: Vec<EntityType> = RULE_OWNED.iter().map(EntityType::new).collect();
        types.iter().filter(|t| !owned.contains(t)).cloned().collect()
    }
}

impl<D: Detector> Detector for ModelTypeFilter<D> {
    fn kind(&self) -> DetectorKind {
        self.inner.kind()
    }
    fn name(&self) -> String {
        self.inner.name()
    }
    fn max_words(&self) -> Option<usize> {
        self.inner.max_words()
    }
    fn detect(&self, text: &str, req: &DetectRequest<'_>) -> Result<Vec<Candidate>, DetectorError> {
        let types = Self::filtered_types(req.types);
        if types.is_empty() {
            return Ok(Vec::new());
        }
        self.inner.detect(text, &DetectRequest { types: &types, languages: req.languages })
    }
    fn detect_batch(&self, texts: &[&str], req: &DetectRequest<'_>) -> Result<Vec<Vec<Candidate>>, DetectorError> {
        let types = Self::filtered_types(req.types);
        if types.is_empty() {
            return Ok(vec![Vec::new(); texts.len()]);
        }
        self.inner.detect_batch(texts, &DetectRequest { types: &types, languages: req.languages })
    }
}

/// The GLiNER Registry and ModelStore behind the app's model seam.
pub struct GlinerModelManager {
    registry: Registry,
    store: ModelStore,
    downloading: Mutex<Vec<String>>,
}

impl GlinerModelManager {
    pub fn new() -> Result<Self, String> {
        Ok(GlinerModelManager {
            registry: Registry::builtin(),
            store: ModelStore::with_default_dir().map_err(|e| e.to_string())?,
            downloading: Mutex::new(Vec::new()),
        })
    }

    fn spec(&self, id: &str) -> Result<&pocketanon_detect_gliner::ModelSpec, String> {
        self.registry.get(id).ok_or_else(|| format!("Unknown model {id}"))
    }

    fn load_dir(&self, dir: &Path, name: Option<String>, quantization: Option<Quantization>) -> Result<Box<dyn Detector>, String> {
        // Prefer the requested variant, but an installed one beats a missing one.
        let quantization = quantization.filter(|q| dir.join(q.onnx_file()).is_file());
        let options = GlinerOptions { name, quantization, ..GlinerOptions::default() };
        let det = GlinerDetector::load(dir, options).map_err(|e| e.to_string())?;
        Ok(Box::new(ModelTypeFilter::new(det)))
    }
}

impl ModelManager for GlinerModelManager {
    fn models(&self) -> Vec<ModelInfo> {
        self.registry
            .models()
            .iter()
            .map(|m| ModelInfo {
                id: m.id.to_string(),
                display_name: m.display_name.to_string(),
                languages: m.languages(),
                size_bytes: m.approx_bytes(m.default_quantization).unwrap_or(0),
            })
            .collect()
    }

    fn status(&self, id: &str) -> ModelStatus {
        if self.downloading.lock().map(|d| d.iter().any(|x| x == id)).unwrap_or(false) {
            return ModelStatus::Downloading;
        }
        match self.spec(id).map(|s| self.store.status(s)) {
            Ok(pocketanon_detect_gliner::ModelStatus::Installed(_)) => ModelStatus::Installed,
            _ => ModelStatus::Missing,
        }
    }

    fn download(&self, id: &str, progress: Box<dyn Fn(DownloadProgress) + Send>) -> Result<(), String> {
        let spec = self.spec(id)?;
        if let Ok(mut d) = self.downloading.lock() {
            d.push(id.to_string());
        }
        // The store wants a `Sync` callback; the app hands us a `Send` one. Serialise calls.
        let progress = Mutex::new(progress);
        let result = self
            .store
            .download(spec, spec.default_quantization, &|p| {
                if let Ok(cb) = progress.lock() {
                    cb(DownloadProgress { received: p.received, total: p.total });
                }
            })
            .map(|_| ())
            .map_err(|e| e.to_string());
        if let Ok(mut d) = self.downloading.lock() {
            d.retain(|x| x != id);
        }
        result
    }

    fn load_detector(&self, id: &str) -> Result<Box<dyn Detector>, String> {
        let spec = self.spec(id)?;
        match self.store.status(spec) {
            pocketanon_detect_gliner::ModelStatus::Installed(dir) => self.load_dir(&dir, Some(spec.id.to_string()), Some(spec.default_quantization)),
            _ => Err(format!("{} is not downloaded yet", spec.display_name)),
        }
    }

    fn load_from_folder(&self, folder: &Path) -> Result<Box<dyn Detector>, String> {
        let dir = ModelStore::local_dir(folder).map_err(|e| e.to_string())?;
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
        self.load_dir(&dir, name, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketanon_core::{FnDetector, TypeCatalog};

    #[test]
    fn filter_withholds_rule_owned_types() {
        let all = TypeCatalog::builtin().all_types();
        let kept = ModelTypeFilter::<FnDetector<fn(&str, &DetectRequest<'_>) -> Vec<Candidate>>>::filtered_types(&all);
        assert!(kept.contains(&EntityType::new("person")));
        assert!(kept.contains(&EntityType::new("tax number")));
        assert!(!kept.contains(&EntityType::new("email")));
        assert!(!kept.contains(&EntityType::new("iban")));
        assert_eq!(kept.len(), all.len() - RULE_OWNED.len());
    }

    #[test]
    fn filter_passes_reduced_request_to_inner() {
        let seen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        let inner = FnDetector::new(DetectorKind::Model, "m", move |_, req| {
            s2.lock().unwrap().extend(req.types.iter().map(|t| t.as_str().to_string()));
            vec![]
        });
        let f = ModelTypeFilter::new(inner);
        let types = [EntityType::new("person"), EntityType::new("email")];
        f.detect("x", &DetectRequest { types: &types, languages: &[] }).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["person".to_string()]);
    }
}
