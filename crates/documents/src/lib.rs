//! Loading, anonymizing and saving **Documents** (TXT, DOCX, RTF), with page previews.
//!
//! Everything is behind [`AnonymizableDocument`] from `pocketanon-core`; no Wordcraft type is
//! part of this crate's API (ADR-0002), so a Wordcraft upgrade stays inside `adapter`.
//!
//! Fidelity per format (ADR-0001):
//! - TXT is exact: encoding, BOM and every line terminator are kept; only replaced bytes change.
//! - DOCX goes through Wordcraft's reader and writer; replacements are run-aware in every story
//!   (body, tables, headers, footers, footnotes, endnotes, comments, text boxes).
//! - RTF goes through Wordcraft's flow model and is structurally faithful.
//!
//! On save, DOCX and RTF also drop the document's creator and last editor, and DOCX sets Word's
//! "remove personal information" option so comment and revision authors are written as "Author".

mod adapter;
mod text;

use std::path::{Path, PathBuf};

pub use pocketanon_core::document::{AnonymizableDocument, DocumentError, Format};

/// Parse `bytes` as a document of `format`.
pub fn load(bytes: &[u8], format: Format) -> Result<Box<dyn AnonymizableDocument>, DocumentError> {
    Ok(Box::new(adapter::WordcraftDocument::load(bytes, format)?))
}

/// The format named by the file's extension, if it is one we support.
pub fn detect_format(path: &Path) -> Option<Format> {
    Format::from_extension(path.extension()?.to_str()?)
}

/// Where the result for `input` goes: beside it, `.anonymized` before the extension
/// (`letter.docx` → `letter.anonymized.docx`). Never equal to `input`.
pub fn output_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_os_string()).unwrap_or_else(|| "document".into());
    let mut name = stem;
    name.push(".anonymized");
    if let Some(ext) = input.extension() {
        name.push(".");
        name.push(ext);
    }
    input.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_inserts_marker() {
        assert_eq!(output_path(Path::new("/a/letter.docx")), PathBuf::from("/a/letter.anonymized.docx"));
        assert_eq!(output_path(Path::new("notes.TXT")), PathBuf::from("notes.anonymized.TXT"));
        assert_eq!(output_path(Path::new("/a/README")), PathBuf::from("/a/README.anonymized"));
        assert_eq!(output_path(Path::new("x.anonymized.rtf")), PathBuf::from("x.anonymized.anonymized.rtf"));
        for p in ["a.docx", "/tmp/.hidden", "b", "c.anonymized.txt"] {
            assert_ne!(output_path(Path::new(p)), PathBuf::from(p));
        }
    }

    #[test]
    fn detect_by_extension() {
        assert_eq!(detect_format(Path::new("/x/Brief.DOCX")), Some(Format::Docx));
        assert_eq!(detect_format(Path::new("a.rtf")), Some(Format::Rtf));
        assert_eq!(detect_format(Path::new("a.txt")), Some(Format::Txt));
        assert_eq!(detect_format(Path::new("a.pdf")), None);
        assert_eq!(detect_format(Path::new("noext")), None);
    }
}
