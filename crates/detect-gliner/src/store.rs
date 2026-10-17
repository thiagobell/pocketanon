//! The models directory: where Models live, whether they are installed, and how they get there.
//!
//! Only model files are written here. Nothing about user content is ever stored (ADR-0003).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::GlinerError;
use crate::registry::{ModelSpec, Quantization};

/// Installation state of a Model in a [`ModelStore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelStatus {
    /// All support files and at least one ONNX variant are present; holds the model directory.
    Installed(PathBuf),
    /// Nothing on disk.
    Missing,
    /// Some files (or leftover `*.part` downloads) exist, but the Model is not usable yet.
    Partial,
}

/// Reported repeatedly while a file downloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    /// File path relative to the model directory, e.g. `onnx/model_quantized.onnx`.
    pub file: String,
    pub received: u64,
    /// From Content-Length, when the server sent one.
    pub total: Option<u64>,
}

/// Bytes between two progress callbacks.
const PROGRESS_EVERY: u64 = 1 << 20;

/// Manages `<base>/<model id>/` directories.
#[derive(Clone, Debug)]
pub struct ModelStore {
    base: PathBuf,
}

impl ModelStore {
    /// A store rooted at `base`; each Model lives in `base/<model id>/`.
    pub fn new(base: impl Into<PathBuf>) -> Self {
        ModelStore { base: base.into() }
    }

    /// The platform default: `<data dir>/models`, e.g.
    /// `~/Library/Application Support/de.PocketAnon.PocketAnon/models` on macOS.
    pub fn default_base_dir() -> Result<PathBuf, GlinerError> {
        let dirs = directories::ProjectDirs::from("de", "PocketAnon", "PocketAnon").ok_or(GlinerError::NoDataDir)?;
        Ok(dirs.data_dir().join("models"))
    }

    /// A store at [`ModelStore::default_base_dir`].
    pub fn with_default_dir() -> Result<Self, GlinerError> {
        Ok(ModelStore::new(Self::default_base_dir()?))
    }

    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    pub fn model_dir(&self, spec: &ModelSpec) -> PathBuf {
        self.base.join(spec.id)
    }

    /// Quantizations whose ONNX file is fully present.
    pub fn installed_variants(&self, spec: &ModelSpec) -> Vec<Quantization> {
        let dir = self.model_dir(spec);
        spec.variants
            .iter()
            .map(|v| v.quantization)
            .filter(|q| dir.join(q.onnx_file()).is_file())
            .collect()
    }

    pub fn status(&self, spec: &ModelSpec) -> ModelStatus {
        let dir = self.model_dir(spec);
        if !dir.is_dir() {
            return ModelStatus::Missing;
        }
        let support_ok = spec.support_files.iter().all(|f| dir.join(f.path).is_file());
        if support_ok && !self.installed_variants(spec).is_empty() {
            return ModelStatus::Installed(dir);
        }
        if dir_has_any_file(&dir) { ModelStatus::Partial } else { ModelStatus::Missing }
    }

    /// Downloads the support files and the ONNX file for `variant` into the model directory and
    /// returns it. Files already present are skipped. Each file is written to `<file>.part` and
    /// renamed once its size matches Content-Length. Blocking; run it on a worker thread.
    pub fn download(
        &self,
        spec: &ModelSpec,
        variant: Quantization,
        progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<PathBuf, GlinerError> {
        let files = spec.files_for(variant).ok_or_else(|| GlinerError::UnknownVariant {
            model: spec.id.to_string(),
            variant: variant.to_string(),
        })?;
        let dir = self.model_dir(spec);
        fs::create_dir_all(&dir)?;
        let agent = agent();
        for f in files {
            let dest = dir.join(f.path);
            if dest.is_file() {
                continue;
            }
            download_file(&agent, &spec.url(f.path), &dest, f.path, progress)?;
        }
        Ok(dir)
    }

    /// Validates a user-supplied model folder: it must contain `tokenizer.json` and an ONNX file
    /// (`onnx/*.onnx` or `*.onnx`). Returns the canonical path.
    pub fn local_dir(path: impl AsRef<Path>) -> Result<PathBuf, GlinerError> {
        let path = path.as_ref();
        if !path.join("tokenizer.json").is_file() {
            return Err(GlinerError::MissingFile(path.to_path_buf(), "tokenizer.json".into()));
        }
        if find_onnx_file(path, None).is_none() {
            return Err(GlinerError::MissingFile(path.to_path_buf(), "an .onnx model file".into()));
        }
        Ok(path.canonicalize()?)
    }
}

/// Finds the ONNX graph in a model directory. Tries `preferred` first, then the known variant
/// names (int8, fp16, fp32), then any `*.onnx` in `onnx/` or the directory itself (sorted).
pub fn find_onnx_file(dir: &Path, preferred: Option<Quantization>) -> Option<PathBuf> {
    let known = preferred.into_iter().chain(Quantization::ALL);
    for q in known {
        let p = dir.join(q.onnx_file());
        if p.is_file() {
            return Some(p);
        }
    }
    for sub in [dir.join("onnx"), dir.to_path_buf()] {
        let Ok(rd) = fs::read_dir(&sub) else { continue };
        let mut found: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "onnx"))
            .collect();
        found.sort();
        if let Some(p) = found.into_iter().next() {
            return Some(p);
        }
    }
    None
}

fn dir_has_any_file(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    rd.filter_map(Result::ok).any(|e| {
        let p = e.path();
        p.is_file() || (p.is_dir() && dir_has_any_file(&p))
    })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("PocketAnon/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent()
}

fn download_file(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    name: &str,
    progress: &(dyn Fn(DownloadProgress) + Sync),
) -> Result<(), GlinerError> {
    let err = |e: &dyn std::fmt::Display| GlinerError::Download { url: url.to_string(), message: e.to_string() };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_path(dest);
    let resp = agent.get(url).call().map_err(|e| err(&e))?;
    let total = resp.body().content_length();
    let mut reader = resp.into_body().into_reader();
    let mut out = fs::File::create(&part)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    let mut last_report = 0u64;
    progress(DownloadProgress { file: name.to_string(), received, total });
    let result = (|| -> Result<(), GlinerError> {
        loop {
            let n = reader.read(&mut buf).map_err(|e| err(&e))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            received += n as u64;
            if received - last_report >= PROGRESS_EVERY {
                last_report = received;
                progress(DownloadProgress { file: name.to_string(), received, total });
            }
        }
        out.flush()?;
        out.sync_all()?;
        if let Some(expected) = total
            && expected != received
        {
            return Err(GlinerError::SizeMismatch { file: name.to_string(), expected, received });
        }
        Ok(())
    })();
    drop(out);
    if let Err(e) = result {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    progress(DownloadProgress { file: name.to_string(), received, total });
    fs::rename(&part, dest)?;
    Ok(())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn layout_and_status() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path());
        let reg = Registry::builtin();
        let spec = reg.default_model();
        let dir = store.model_dir(spec);
        assert_eq!(dir, tmp.path().join("gliner_multi_pii_v1"));

        assert_eq!(store.status(spec), ModelStatus::Missing);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(store.status(spec), ModelStatus::Missing, "empty dir counts as missing");

        touch(&dir.join("onnx/model_quantized.onnx.part"));
        assert_eq!(store.status(spec), ModelStatus::Partial);

        touch(&dir.join("tokenizer.json"));
        touch(&dir.join("gliner_config.json"));
        assert_eq!(store.status(spec), ModelStatus::Partial, "no complete onnx file yet");

        touch(&dir.join("onnx/model_fp16.onnx"));
        assert_eq!(store.status(spec), ModelStatus::Installed(dir.clone()));
        assert_eq!(store.installed_variants(spec), vec![Quantization::Fp16]);

        fs::remove_file(dir.join("gliner_config.json")).unwrap();
        assert_eq!(store.status(spec), ModelStatus::Partial);
    }

    #[test]
    fn default_base_dir_ends_in_models() {
        let d = ModelStore::default_base_dir().unwrap();
        assert!(d.ends_with("models"));
        assert!(d.to_string_lossy().contains("PocketAnon"));
    }

    #[test]
    fn local_dir_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        assert!(matches!(ModelStore::local_dir(p), Err(GlinerError::MissingFile(_, f)) if f == "tokenizer.json"));
        touch(&p.join("tokenizer.json"));
        assert!(ModelStore::local_dir(p).is_err());
        touch(&p.join("model.onnx"));
        assert!(ModelStore::local_dir(p).is_ok());
        assert_eq!(find_onnx_file(p, None), Some(p.join("model.onnx")));
    }

    #[test]
    fn onnx_preference_order() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        touch(&p.join("onnx/model.onnx"));
        touch(&p.join("onnx/model_quantized.onnx"));
        touch(&p.join("onnx/model_q4.onnx"));
        assert_eq!(find_onnx_file(p, None), Some(p.join("onnx/model_quantized.onnx")));
        assert_eq!(find_onnx_file(p, Some(Quantization::Fp32)), Some(p.join("onnx/model.onnx")));
        fs::remove_file(p.join("onnx/model.onnx")).unwrap();
        fs::remove_file(p.join("onnx/model_quantized.onnx")).unwrap();
        assert_eq!(find_onnx_file(p, Some(Quantization::Fp32)), Some(p.join("onnx/model_q4.onnx")));
    }

    #[test]
    fn part_suffix() {
        assert_eq!(part_path(Path::new("a/onnx/model.onnx")), PathBuf::from("a/onnx/model.onnx.part"));
    }
}
