//! The built-in list of known Models.

use pocketanon_core::Language;

/// An ONNX export precision offered by a Model repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quantization {
    /// Dynamic int8 quantization (`onnx/model_quantized.onnx`). Smallest; for
    /// gliner_multi_pii-v1 it loses most of its recall.
    Int8,
    /// Half precision (`onnx/model_fp16.onnx`).
    Fp16,
    /// Full precision (`onnx/model.onnx`).
    Fp32,
}

impl Quantization {
    /// Preference order when no variant is requested. fp32 first: ONNX Runtime on CPU runs
    /// fp16 graphs through casts and was measured about 1.5 times slower than fp32; int8 last
    /// (see docs/poc-plan.md).
    pub const ALL: [Quantization; 3] = [Quantization::Fp32, Quantization::Fp16, Quantization::Int8];

    /// Path of the ONNX file relative to the model directory.
    pub fn onnx_file(self) -> &'static str {
        match self {
            Quantization::Int8 => "onnx/model_quantized.onnx",
            Quantization::Fp16 => "onnx/model_fp16.onnx",
            Quantization::Fp32 => "onnx/model.onnx",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quantization::Int8 => "int8",
            Quantization::Fp16 => "fp16",
            Quantization::Fp32 => "fp32",
        }
    }
}

impl std::fmt::Display for Quantization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A file to download, relative to the repository root and the model directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    pub path: &'static str,
    /// Size in bytes as published; informational (downloads verify against Content-Length).
    pub approx_bytes: u64,
}

/// One downloadable ONNX variant of a Model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariantSpec {
    pub quantization: Quantization,
    pub approx_bytes: u64,
}

/// A Registry entry describing one Model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSpec {
    /// Stable identifier, used as directory name and persisted in settings.
    pub id: &'static str,
    pub display_name: &'static str,
    /// Hugging Face repository, e.g. `onnx-community/gliner_multi_pii-v1`.
    pub repo: &'static str,
    /// Files needed besides the ONNX graph.
    pub support_files: &'static [ModelFile],
    pub variants: &'static [VariantSpec],
    pub default_quantization: Quantization,
    /// ISO 639-1 codes of the covered Languages.
    pub languages: &'static [&'static str],
    pub description: &'static str,
}

impl ModelSpec {
    pub fn languages(&self) -> Vec<Language> {
        self.languages.iter().map(Language::new).collect()
    }

    pub fn covers(&self, lang: &Language) -> bool {
        self.languages.contains(&lang.code())
    }

    pub fn variant(&self, q: Quantization) -> Option<&VariantSpec> {
        self.variants.iter().find(|v| v.quantization == q)
    }

    /// Every file to download for `q`: support files, then the ONNX graph.
    pub fn files_for(&self, q: Quantization) -> Option<Vec<ModelFile>> {
        let v = self.variant(q)?;
        let mut files = self.support_files.to_vec();
        files.push(ModelFile { path: q.onnx_file(), approx_bytes: v.approx_bytes });
        Some(files)
    }

    /// Approximate total download size for `q`.
    pub fn approx_bytes(&self, q: Quantization) -> Option<u64> {
        Some(self.files_for(q)?.iter().map(|f| f.approx_bytes).sum())
    }

    /// `https://huggingface.co/<repo>/resolve/main/<file>`
    pub fn url(&self, file: &str) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.repo, file)
    }
}

const GLINER_MULTI_PII_V1: ModelSpec = ModelSpec {
    id: "gliner_multi_pii_v1",
    display_name: "GLiNER multi PII v1 (multilingual)",
    repo: "onnx-community/gliner_multi_pii-v1",
    support_files: &[
        ModelFile { path: "tokenizer.json", approx_bytes: 16_331_948 },
        ModelFile { path: "gliner_config.json", approx_bytes: 732 },
    ],
    variants: &[
        VariantSpec { quantization: Quantization::Int8, approx_bytes: 349_120_924 },
        VariantSpec { quantization: Quantization::Fp16, approx_bytes: 579_717_643 },
        VariantSpec { quantization: Quantization::Fp32, approx_bytes: 1_157_129_714 },
    ],
    default_quantization: Quantization::Fp32,
    languages: &["de", "en", "fr", "es", "it", "pt", "nl", "pl"],
    description: "mDeBERTa-based GLiNER fine-tuned for personal information; multilingual.",
};

const GLINER_SMALL_V2_1: ModelSpec = ModelSpec {
    id: "gliner_small_v2_1",
    display_name: "GLiNER small v2.1 (English)",
    repo: "onnx-community/gliner_small-v2.1",
    support_files: &[
        ModelFile { path: "tokenizer.json", approx_bytes: 8_657_198 },
        ModelFile { path: "gliner_config.json", approx_bytes: 731 },
    ],
    variants: &[
        VariantSpec { quantization: Quantization::Int8, approx_bytes: 183_403_734 },
        VariantSpec { quantization: Quantization::Fp16, approx_bytes: 306_253_040 },
        VariantSpec { quantization: Quantization::Fp32, approx_bytes: 611_293_061 },
    ],
    default_quantization: Quantization::Fp32,
    languages: &["en"],
    description: "Smaller, faster general-purpose GLiNER; English only.",
};

/// The built-in list of known Models. The first entry is the default.
#[derive(Clone, Debug)]
pub struct Registry {
    models: Vec<ModelSpec>,
}

impl Registry {
    pub fn builtin() -> Self {
        Registry { models: vec![GLINER_MULTI_PII_V1, GLINER_SMALL_V2_1] }
    }

    pub fn models(&self) -> &[ModelSpec] {
        &self.models
    }

    pub fn default_model(&self) -> &ModelSpec {
        &self.models[0]
    }

    pub fn get(&self, id: &str) -> Option<&ModelSpec> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Models covering **all** requested Languages, default first, otherwise in Registry order.
    /// An empty request returns every Model.
    pub fn for_languages(&self, langs: &[Language]) -> Vec<&ModelSpec> {
        self.models.iter().filter(|m| langs.iter().all(|l| m.covers(l))).collect()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Registry::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_multilingual_pii() {
        let r = Registry::builtin();
        let d = r.default_model();
        assert_eq!(d.id, "gliner_multi_pii_v1");
        assert_eq!(d.default_quantization, Quantization::Fp32);
        assert_eq!(
            d.url("onnx/model_quantized.onnx"),
            "https://huggingface.co/onnx-community/gliner_multi_pii-v1/resolve/main/onnx/model_quantized.onnx"
        );
        let files: Vec<_> = d.files_for(Quantization::Int8).unwrap().iter().map(|f| f.path).collect();
        assert_eq!(files, ["tokenizer.json", "gliner_config.json", "onnx/model_quantized.onnx"]);
        assert!(d.approx_bytes(Quantization::Int8).unwrap() > 300_000_000);
    }

    #[test]
    fn filters_by_languages() {
        let r = Registry::builtin();
        let ids = |langs: &[Language]| r.for_languages(langs).iter().map(|m| m.id).collect::<Vec<_>>();
        assert_eq!(ids(&[Language::english()]), ["gliner_multi_pii_v1", "gliner_small_v2_1"]);
        assert_eq!(ids(&[Language::german()]), ["gliner_multi_pii_v1"]);
        assert_eq!(ids(&[Language::german(), Language::english()]), ["gliner_multi_pii_v1"]);
        assert_eq!(ids(&[]), ["gliner_multi_pii_v1", "gliner_small_v2_1"]);
        assert!(ids(&[Language::new("ja")]).is_empty());
        assert_eq!(ids(&[Language::new(" EN ")]).len(), 2);
    }

    #[test]
    fn lookup() {
        let r = Registry::builtin();
        assert!(r.get("gliner_small_v2_1").is_some());
        assert!(r.get("nope").is_none());
        assert_eq!(r.get("gliner_small_v2_1").unwrap().languages(), vec![Language::english()]);
    }
}
