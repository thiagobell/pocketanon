//! The contract between the UI and a loaded document, independent of file format.
//!
//! A document is a list of [`Segment`]s, each a run of plain text that detectors analyse and
//! whose byte offsets replacements refer to. A segment is typically one paragraph (body,
//! header, footer, footnote, comment, table cell). Rendering is optional: a format may return
//! no pages, in which case the UI falls back to showing segment text.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::replace::Replacement;
use crate::types::Span;

/// Supported input formats and the fidelity each promises (ADR-0001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Txt,
    Docx,
    Rtf,
}

impl Format {
    pub fn from_extension(ext: &str) -> Option<Format> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "txt" | "text" => Some(Format::Txt),
            "docx" => Some(Format::Docx),
            "rtf" => Some(Format::Rtf),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Format::Txt => "txt",
            Format::Docx => "docx",
            Format::Rtf => "rtf",
        }
    }

    pub fn all() -> [Format; 3] {
        [Format::Txt, Format::Docx, Format::Rtf]
    }

    /// One sentence for the UI, per `CONTEXT.md` *Fidelity*.
    pub fn fidelity(&self) -> &'static str {
        match self {
            Format::Txt => "Exact: only the detected entities change.",
            Format::Docx => "Formatting preserved, including headers, footers, footnotes, comments and tables.",
            Format::Rtf => "Structure preserved: paragraphs, character formatting, lists and tables survive. Headers, footers, footnotes and comments are dropped from the output.",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Txt => "TXT",
            Format::Docx => "DOCX",
            Format::Rtf => "RTF",
        })
    }
}

/// Where in the document a segment lives; for labelling in the review list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Body,
    Header,
    Footer,
    Footnote,
    Endnote,
    Comment,
    TextBox,
    Other,
}

impl fmt::Display for SegmentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SegmentKind::Body => "body",
            SegmentKind::Header => "header",
            SegmentKind::Footer => "footer",
            SegmentKind::Footnote => "footnote",
            SegmentKind::Endnote => "endnote",
            SegmentKind::Comment => "comment",
            SegmentKind::TextBox => "text box",
            SegmentKind::Other => "other",
        })
    }
}

/// Stable identifier of a segment within one loaded document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SegmentId(pub usize);

/// A run of analysable text. `text` is exactly what detectors see; all spans for this segment
/// are byte offsets into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub id: SegmentId,
    pub kind: SegmentKind,
    pub text: String,
}

/// A rectangle in page pixels at the render scale (origin top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A rendered page: tightly packed RGBA8, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Where a span of a segment appears on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub page: usize,
    pub rects: Vec<Rect>,
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("unsupported format")]
    Unsupported,
    #[error("could not read document: {0}")]
    Read(String),
    #[error("could not write document: {0}")]
    Write(String),
    #[error("unknown segment {0:?}")]
    UnknownSegment(SegmentId),
}

/// Edits for one segment, in document order (as produced by [`crate::replace::plan_replacements`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentEdits {
    pub segment: SegmentId,
    pub replacements: Vec<Replacement>,
}

/// A loaded document that can be analysed, edited and saved in its own format.
pub trait AnonymizableDocument: Send {
    fn format(&self) -> Format;

    /// All analysable segments in document order. Must be stable until `apply` is called.
    fn segments(&self) -> Vec<Segment>;

    /// Apply replacements. Offsets refer to the segment texts as returned by the last
    /// `segments()` call. After this, `segments()` reflects the new text.
    fn apply(&mut self, edits: &[SegmentEdits]) -> Result<(), DocumentError>;

    /// Serialise the current state in the document's format.
    fn save(&self) -> Result<Vec<u8>, DocumentError>;

    /// Number of rendered pages, or 0 when this format has no page preview.
    fn page_count(&self) -> usize {
        0
    }

    /// Render one page at `scale` device pixels per point (1.0 = 72 dpi).
    fn render_page(&self, _page: usize, _scale: f32) -> Option<PageImage> {
        None
    }

    /// Where a span of a segment lands on the rendered pages at `scale`; empty when unknown.
    fn locate(&self, _segment: SegmentId, _span: Span, _scale: f32) -> Vec<Placement> {
        Vec::new()
    }

    /// The text position under a point on a rendered page (page pixels at `scale`, origin
    /// top-left): the segment and a byte offset into its text, on a char boundary. `None` when
    /// the point is not over analysable text or the format has no pages. Together with
    /// [`AnonymizableDocument::locate`] this lets a UI select text on rendered pages.
    fn hit_test(&self, _page: usize, _x: f32, _y: f32, _scale: f32) -> Option<TextHit> {
        None
    }
}

/// A text position under a point on a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextHit {
    pub segment: SegmentId,
    /// Byte offset into that segment's text, on a char boundary.
    pub offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_extension() {
        assert_eq!(Format::from_extension(".DOCX"), Some(Format::Docx));
        assert_eq!(Format::from_extension("txt"), Some(Format::Txt));
        assert_eq!(Format::from_extension("pdf"), None);
    }
}
