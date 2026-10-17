//! The one place that touches Wordcraft (ADR-0002). Nothing here is public outside the crate.
//!
//! # Segments and positions
//!
//! Every paragraph of the document gets a slot in `locs` at load time, in document order: the
//! body (table cells descended), then every part grouped by kind (headers, footers, footnotes,
//! endnotes, comments, text boxes; ascending part id within a kind). A [`SegmentId`] is the index
//! of that slot, so ids stay stable even though empty paragraphs are not reported as segments.
//! Edits never add or remove paragraphs, so the slots stay valid for the document's lifetime.
//!
//! A segment's text has exactly the bytes of the paragraph's stored `Paragraph::text`, with two
//! substitutions of equal byte length:
//!
//! - every U+FFFC object placeholder (images, shapes, fields, note references, and zero-width
//!   markers such as bookmarks, comment ranges and field start/end; 3 bytes) becomes three
//!   spaces;
//! - page and column breaks (U+000C, U+000E; 1 byte) become `\n`.
//!
//! So a byte offset `o` in the segment is `Pos { story, path, off: o }` in Wordcraft, and every
//! char boundary of the segment is a char boundary of the paragraph.
//!
//! TXT is the exception: its authoritative text is [`PlainText`] (exact bytes), and the Wordcraft
//! document is only a preview copy with one paragraph per line.

use std::sync::{Arc, Mutex};

use pocketanon_core::document::{AnonymizableDocument, DocumentError, Format, PageImage, Placement, Rect, Segment, SegmentEdits, SegmentId, SegmentKind};
use pocketanon_core::replace::{Replacement, apply_replacements};
use pocketanon_core::types::Span;
use wordcraft_doc::para::{COLUMN_BREAK, OBJ, PAGE_BREAK};
use wordcraft_doc::{CharProps, Document, Paragraph, PartKind, Path, Pos, StoryRef};
use wordcraft_layout::{DocLayout, LayoutCache, LayoutOptions};
use wordcraft_render::RenderOptions;

use crate::text::PlainText;

/// OBJ is three bytes in UTF-8; the segment shows it as this many spaces.
const OBJ_SPACES: &str = "   ";
const _: () = assert!(OBJ.len_utf8() == OBJ_SPACES.len());

#[derive(Clone, Debug)]
struct Loc {
    story: StoryRef,
    path: Path,
    kind: SegmentKind,
}

#[derive(Default)]
struct View {
    cache: LayoutCache,
    layout: Option<Arc<DocLayout>>,
}

pub(crate) struct WordcraftDocument {
    format: Format,
    /// DOCX/RTF: the document itself. TXT: a preview copy of `plain`.
    doc: Document,
    locs: Vec<Loc>,
    /// TXT only: the authoritative text.
    plain: Option<PlainText>,
    view: Mutex<View>,
}

fn read_err(e: impl std::fmt::Display) -> DocumentError {
    DocumentError::Read(e.to_string())
}

fn write_err(e: impl std::fmt::Display) -> DocumentError {
    DocumentError::Write(e.to_string())
}

impl WordcraftDocument {
    pub fn load(bytes: &[u8], format: Format) -> Result<Self, DocumentError> {
        let (doc, plain) = match format {
            Format::Docx => (wordcraft_docx::read(bytes).map_err(read_err)?, None),
            Format::Rtf => (wordcraft_formats::import("rtf", bytes).ok_or(DocumentError::Unsupported)?.map_err(read_err)?, None),
            Format::Txt => {
                let plain = PlainText::decode(bytes);
                (preview_of(&plain), Some(plain))
            }
        };
        let locs = index(&doc);
        Ok(WordcraftDocument { format, doc, locs, plain, view: Mutex::new(View::default()) })
    }

    fn loc(&self, id: SegmentId) -> Result<&Loc, DocumentError> {
        self.locs.get(id.0).ok_or(DocumentError::UnknownSegment(id))
    }

    /// The segment text for slot `id` (including empty ones).
    fn text_of(&self, id: SegmentId) -> Result<String, DocumentError> {
        if let Some(plain) = &self.plain {
            return plain.lines.get(id.0).cloned().ok_or(DocumentError::UnknownSegment(id));
        }
        let loc = self.loc(id)?;
        let p = self.doc.para(loc.story, &loc.path).ok_or(DocumentError::UnknownSegment(id))?;
        Ok(segment_text(p))
    }

    /// A segment offset as an offset into the paragraph that layout sees.
    fn para_offset(&self, id: SegmentId, off: usize) -> usize {
        match &self.plain {
            // The preview's paragraphs drop U+FFFC (Wordcraft reserves it); shift past those.
            Some(plain) => plain.lines.get(id.0).map(|l| off - l[..off.min(l.len())].matches(OBJ).count() * OBJ.len_utf8()).unwrap_or(off),
            None => off,
        }
    }

    fn layout(&self) -> Arc<DocLayout> {
        let mut v = self.view.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(l) = &v.layout {
            return l.clone();
        }
        let l = Arc::new(wordcraft_layout::layout(&self.doc, &mut v.cache, &LayoutOptions::default()));
        v.layout = Some(l.clone());
        l
    }

    fn invalidate(&mut self) {
        let v = self.view.get_mut().unwrap_or_else(|e| e.into_inner());
        v.layout = None;
    }
}

/// The paragraph text with objects and breaks swapped for same-length plain characters.
fn segment_text(p: &Paragraph) -> String {
    let mut out = String::with_capacity(p.text.len());
    for c in p.text.chars() {
        match c {
            OBJ => out.push_str(OBJ_SPACES),
            PAGE_BREAK | COLUMN_BREAK => out.push('\n'),
            c => out.push(c),
        }
    }
    out
}

fn kind_of(k: PartKind) -> SegmentKind {
    match k {
        PartKind::Header => SegmentKind::Header,
        PartKind::Footer => SegmentKind::Footer,
        PartKind::Footnote => SegmentKind::Footnote,
        PartKind::Endnote => SegmentKind::Endnote,
        PartKind::Comment => SegmentKind::Comment,
        PartKind::TextBox => SegmentKind::TextBox,
    }
}

fn rank(k: PartKind) -> u8 {
    match k {
        PartKind::Header => 0,
        PartKind::Footer => 1,
        PartKind::Footnote => 2,
        PartKind::Endnote => 3,
        PartKind::Comment => 4,
        PartKind::TextBox => 5,
    }
}

/// Every paragraph, in segment order.
fn index(doc: &Document) -> Vec<Loc> {
    let mut locs: Vec<Loc> =
        doc.para_paths(StoryRef::Body).into_iter().map(|path| Loc { story: StoryRef::Body, path, kind: SegmentKind::Body }).collect();
    let mut parts: Vec<(u8, u32, PartKind)> = doc.parts.iter().map(|(id, p)| (rank(p.kind), *id, p.kind)).collect();
    parts.sort_unstable_by_key(|(r, id, _)| (*r, *id));
    for (_, id, kind) in parts {
        let story = StoryRef::Part(id);
        locs.extend(doc.para_paths(story).into_iter().map(|path| Loc { story, path, kind: kind_of(kind) }));
    }
    locs
}

fn preview_of(plain: &PlainText) -> Document {
    // `from_text` splits on `\n`; the lines hold none, so paragraph i is line i.
    Document::from_text(&plain.lines.join("\n"))
}

/// Remove personal information that is not paragraph text from the copy being saved: the
/// document's creator and last editor, and (DOCX, via Word's own setting) comment and revision
/// authors and initials.
fn scrub_metadata(doc: &mut Document) {
    doc.settings.remove_personal_info = true;
    doc.core.creator.clear();
    doc.core.last_modified_by.clear();
}

/// All edits per segment, merged, sorted and checked against the current segment texts.
fn validate(edits: &[SegmentEdits], text_of: impl Fn(SegmentId) -> Result<String, DocumentError>) -> Result<Vec<(SegmentId, Vec<Replacement>)>, DocumentError> {
    let mut by_seg: std::collections::BTreeMap<SegmentId, Vec<Replacement>> = Default::default();
    for e in edits {
        by_seg.entry(e.segment).or_default().extend(e.replacements.iter().cloned());
    }
    let mut out = Vec::with_capacity(by_seg.len());
    for (id, mut reps) in by_seg {
        let text = text_of(id)?;
        reps.sort_by_key(|r| (r.span.start, r.span.end));
        let mut cursor = 0;
        for r in &reps {
            if !r.span.is_valid_in(&text) || r.span.start < cursor {
                return Err(DocumentError::Write(format!("replacement {:?} is out of bounds, off a char boundary or overlapping in segment {}", r.span, id.0)));
            }
            cursor = r.span.end;
        }
        out.push((id, reps));
    }
    Ok(out)
}

/// Replace `span` of `p` by `with`, run-aware: the placeholder takes the formatting of the first
/// real character of the span ("first run wins"). Inline objects inside the span (pictures,
/// fields, note references) are deleted with it; zero-width markers (bookmarks, comment ranges,
/// field and content-control boundaries, permission ranges) stay, right after the placeholder,
/// so the structures they delimit stay balanced.
fn replace_in(p: &mut Paragraph, span: Span, with: &str) -> Result<(), wordcraft_doc::DocError> {
    let mut keep = Vec::new();
    let mut props: Option<CharProps> = None;
    for (i, c) in p.text[span.range()].char_indices() {
        let off = span.start + i;
        if c == OBJ && p.object_at(off).is_some_and(|o| o.is_marker()) {
            keep.push(off);
        } else if props.is_none() {
            props = Some(p.props_of_char(off).clone());
        }
    }
    let props = props.unwrap_or_else(|| p.props_of_char(span.start).clone());
    p.delete_except(span.start, span.end, &keep)?;
    p.insert_text(span.start, with, &props)?;
    Ok(())
}

impl AnonymizableDocument for WordcraftDocument {
    fn format(&self) -> Format {
        self.format
    }

    fn segments(&self) -> Vec<Segment> {
        (0..self.locs.len())
            .filter_map(|i| {
                let id = SegmentId(i);
                let text = self.text_of(id).ok()?;
                if text.trim().is_empty() {
                    return None;
                }
                Some(Segment { id, kind: self.locs[i].kind, text })
            })
            .collect()
    }

    fn apply(&mut self, edits: &[SegmentEdits]) -> Result<(), DocumentError> {
        // Everything is checked first, so a bad edit leaves the document untouched.
        let plan = validate(edits, |id| self.text_of(id))?;
        if plan.is_empty() {
            return Ok(());
        }
        if let Some(plain) = &mut self.plain {
            for (id, reps) in &plan {
                if let Some(line) = plain.lines.get_mut(id.0) {
                    *line = apply_replacements(line, reps);
                }
            }
            self.doc = preview_of(plain);
        } else {
            for (id, reps) in &plan {
                let loc = self.locs.get(id.0).ok_or(DocumentError::UnknownSegment(*id))?.clone();
                let p = self.doc.para_mut(loc.story, &loc.path).map_err(write_err)?;
                // Back to front: earlier offsets stay valid.
                for r in reps.iter().rev() {
                    replace_in(p, r.span, &r.with).map_err(write_err)?;
                }
            }
        }
        self.invalidate();
        Ok(())
    }

    fn save(&self) -> Result<Vec<u8>, DocumentError> {
        if let Some(plain) = &self.plain {
            return Ok(plain.encode());
        }
        let mut doc = self.doc.clone();
        scrub_metadata(&mut doc);
        match self.format {
            Format::Docx => wordcraft_docx::write(&doc).map_err(write_err),
            Format::Rtf => wordcraft_formats::export("rtf", &doc).ok_or(DocumentError::Unsupported)?.map_err(write_err),
            Format::Txt => Err(DocumentError::Unsupported),
        }
    }

    fn page_count(&self) -> usize {
        self.layout().pages.len()
    }

    fn render_page(&self, page: usize, scale: f32) -> Option<PageImage> {
        let layout = self.layout();
        let page = layout.pages.get(page)?;
        let r = wordcraft_render::render_page(&self.doc, page, scale, &RenderOptions::default());
        // `Rendered` is premultiplied; the contract is straight RGBA8.
        Some(PageImage { width: r.width, height: r.height, rgba: r.to_straight() })
    }

    fn locate(&self, segment: SegmentId, span: Span, scale: f32) -> Vec<Placement> {
        let Ok(loc) = self.loc(segment) else { return Vec::new() };
        let Ok(text) = self.text_of(segment) else { return Vec::new() };
        if !span.is_valid_in(&text) {
            return Vec::new();
        }
        let a = Pos::new(loc.story, loc.path.clone(), self.para_offset(segment, span.start));
        let b = Pos::new(loc.story, loc.path.clone(), self.para_offset(segment, span.end));
        let layout = self.layout();
        let mut out: Vec<Placement> = Vec::new();
        for (page, r) in layout.selection_rects(&self.doc, &a, &b, 0) {
            let rect = Rect { x: r.x * scale, y: r.y * scale, w: r.w * scale, h: r.h * scale };
            match out.iter_mut().find(|p| p.page == page) {
                Some(p) => p.rects.push(rect),
                None => out.push(Placement { page, rects: vec![rect] }),
            }
        }
        out
    }
}

#[cfg(test)]
impl WordcraftDocument {
    fn from_doc(doc: Document, format: Format) -> Self {
        let locs = index(&doc);
        WordcraftDocument { format, doc, locs, plain: None, view: Mutex::new(View::default()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketanon_core::types::EntityType;
    use wordcraft_doc::{InlineObject, para_block};

    fn plain() -> CharProps {
        CharProps::default()
    }
    fn bold() -> CharProps {
        CharProps { bold: Some(true), ..Default::default() }
    }
    fn italic() -> CharProps {
        CharProps { italic: Some(true), ..Default::default() }
    }

    fn para(runs: &[(&str, CharProps)]) -> Paragraph {
        let mut p = Paragraph::new();
        for (t, props) in runs {
            let at = p.len();
            p.insert_text(at, t, props).unwrap();
        }
        p
    }

    fn one_para(p: Paragraph) -> WordcraftDocument {
        let mut d = Document::new();
        d.body = vec![para_block(p)];
        WordcraftDocument::from_doc(d, Format::Docx)
    }

    fn rep(text: &str, needle: &str, with: &str) -> Replacement {
        let s = text.find(needle).unwrap();
        Replacement { span: Span::new(s, s + needle.len()), entity_type: EntityType::new("person"), with: with.into() }
    }

    fn body_para(d: &WordcraftDocument) -> &Paragraph {
        d.doc.para(StoryRef::Body, &Path::top(0)).unwrap()
    }

    fn runs(p: &Paragraph) -> Vec<(String, Option<bool>, Option<bool>)> {
        p.run_ranges().map(|(r, c)| (p.text[r].to_string(), c.bold, c.italic)).collect()
    }

    fn assert_runs_cover(p: &Paragraph) {
        assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), p.text.len(), "run lengths must sum to the text length");
    }

    #[test]
    fn straddling_replacement_keeps_neighbouring_formatting() {
        let mut d = one_para(para(&[("Dear ", plain()), ("Max", bold()), (" Mustermann", plain()), (", thanks", italic())]));
        let seg = d.segments().remove(0);
        d.apply(&[SegmentEdits { segment: seg.id, replacements: vec![rep(&seg.text, "Max Mustermann", "[PERSON_1]")] }]).unwrap();
        let p = body_para(&d);
        assert_eq!(p.text, "Dear [PERSON_1], thanks");
        assert_runs_cover(p);
        assert_eq!(
            runs(p),
            vec![("Dear ".into(), None, None), ("[PERSON_1]".into(), Some(true), None), (", thanks".into(), None, Some(true))]
        );
        assert_eq!(d.segments()[0].text, "Dear [PERSON_1], thanks");
    }

    #[test]
    fn replacement_starting_mid_run_takes_that_runs_formatting() {
        // "Johannes" starts in the plain run and ends inside the bold one.
        let mut d = one_para(para(&[("Hallo Johann", plain()), ("es Max", bold()), (" Ende", italic())]));
        let seg = d.segments().remove(0);
        d.apply(&[SegmentEdits { segment: seg.id, replacements: vec![rep(&seg.text, "Johannes", "[PERSON_1]")] }]).unwrap();
        let p = body_para(&d);
        assert_eq!(p.text, "Hallo [PERSON_1] Max Ende");
        assert_runs_cover(p);
        assert_eq!(
            runs(p),
            vec![("Hallo [PERSON_1]".into(), None, None), (" Max".into(), Some(true), None), (" Ende".into(), None, Some(true))]
        );
    }

    #[test]
    fn several_replacements_apply_back_to_front() {
        let mut d = one_para(para(&[("Max Mustermann", bold()), (" 👋 schrieb an ", plain()), ("user@example.com", italic()), (" und Max Mustermann 🎉.", plain())]));
        let seg = d.segments().remove(0);
        let t = &seg.text;
        let second = t.rfind("Max Mustermann").unwrap();
        let reps = vec![
            rep(t, "Max Mustermann", "[PERSON_1]"),
            rep(t, "user@example.com", "[EMAIL_1]"),
            Replacement { span: Span::new(second, second + 14), entity_type: EntityType::new("person"), with: "[PERSON_1]".into() },
        ];
        d.apply(&[SegmentEdits { segment: seg.id, replacements: reps.clone() }]).unwrap();
        assert_eq!(d.segments()[0].text, apply_replacements(t, &reps));
        let p = body_para(&d);
        assert_runs_cover(p);
        assert_eq!(p.props_of_char(0).bold, Some(true));
        let e = p.text.find("[EMAIL_1]").unwrap();
        assert_eq!(p.props_of_char(e).italic, Some(true));
    }

    #[test]
    fn segment_offsets_align_with_inline_objects() {
        // Text, a field (PAGE), a name, a bookmark around the name, a picture-like field after.
        let mut p = para(&[("Seite ", plain())]);
        let at = p.len();
        p.insert_object(at, InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() }, &plain())
            .unwrap();
        let at = p.len();
        p.insert_text(at, " – Kontakt: ", &plain()).unwrap();
        let at = p.len();
        p.insert_object(at, InlineObject::BookmarkStart { name: "who".into() }, &plain()).unwrap();
        let at = p.len();
        p.insert_text(at, "Max Mustermann", &bold()).unwrap();
        let at = p.len();
        p.insert_object(at, InlineObject::BookmarkEnd { name: "who".into() }, &plain()).unwrap();
        let at = p.len();
        p.insert_text(at, " Ende", &plain()).unwrap();
        let stored = p.text.clone();
        let mut d = one_para(p);

        let seg = d.segments().remove(0);
        // Same byte length, same char boundaries, and every non-object byte identical.
        assert_eq!(seg.text.len(), stored.len());
        assert_eq!(seg.text, stored.replace(OBJ, OBJ_SPACES));
        for (i, _) in stored.char_indices() {
            assert!(seg.text.is_char_boundary(i));
        }
        let name = seg.text.find("Max Mustermann").unwrap();
        assert_eq!(Some(name), stored.find("Max Mustermann"));

        // Replace the name, and separately a span that covers the PAGE field and the bookmark start.
        let field_at = stored.find(OBJ).unwrap();
        let reps = vec![
            Replacement { span: Span::new(field_at - 1, field_at + OBJ.len_utf8()), entity_type: EntityType::new("x"), with: "[X]".into() },
            rep(&seg.text, "Max Mustermann", "[PERSON_1]"),
        ];
        d.apply(&[SegmentEdits { segment: seg.id, replacements: reps }]).unwrap();
        let p = body_para(&d);
        assert_runs_cover(p);
        // The field inside the replaced span is gone; the bookmark markers are not.
        assert_eq!(p.objects.len(), 2);
        assert!(p.objects.iter().all(|o| o.is_marker()));
        assert_eq!(segment_text(p), format!("Seite[X] – Kontakt: {OBJ_SPACES}[PERSON_1]{OBJ_SPACES} Ende"));
        let ph = p.text.find("[PERSON_1]").unwrap();
        assert_eq!(p.props_of_char(ph).bold, Some(true));
    }

    #[test]
    fn replacement_covering_a_marker_keeps_it_after_the_placeholder() {
        let mut p = para(&[("Max", bold())]);
        p.insert_object(3, InlineObject::CommentStart { id: 0 }, &plain()).unwrap();
        let at = p.len();
        p.insert_text(at, " Mustermann", &plain()).unwrap();
        let mut d = one_para(p);
        let seg = d.segments().remove(0);
        d.apply(&[SegmentEdits { segment: seg.id, replacements: vec![Replacement { span: Span::new(0, seg.text.len()), entity_type: EntityType::new("person"), with: "[PERSON_1]".into() }] }])
            .unwrap();
        let p = body_para(&d);
        assert_eq!(p.text, format!("[PERSON_1]{OBJ}"));
        assert!(matches!(p.objects.as_slice(), [InlineObject::CommentStart { id: 0 }]));
        assert_eq!(p.props_of_char(0).bold, Some(true), "first real character's run wins, not the marker's");
        assert_runs_cover(p);
    }

    #[test]
    fn invalid_edits_change_nothing() {
        let mut d = one_para(para(&[("Grüße von Max", plain())]));
        let seg = d.segments().remove(0);
        let ok = rep(&seg.text, "Max", "[P]");
        let bad = Replacement { span: Span::new(3, 4), entity_type: EntityType::new("x"), with: "[X]".into() }; // inside 'ü'
        assert!(d.apply(&[SegmentEdits { segment: seg.id, replacements: vec![ok.clone(), bad] }]).is_err());
        assert!(matches!(d.apply(&[SegmentEdits { segment: SegmentId(99), replacements: vec![ok.clone()] }]), Err(DocumentError::UnknownSegment(SegmentId(99)))));
        let overlapping = Replacement { span: Span::new(ok.span.start - 1, ok.span.end), ..ok.clone() };
        assert!(d.apply(&[SegmentEdits { segment: seg.id, replacements: vec![ok.clone(), overlapping] }]).is_err());
        assert_eq!(d.segments()[0].text, "Grüße von Max");
    }

    #[test]
    fn empty_paragraphs_are_skipped_but_ids_stay_stable() {
        let mut doc = Document::new();
        doc.body = vec![para_block(para(&[("eins", plain())])), para_block(Paragraph::new()), para_block(para(&[("drei", plain())]))];
        let d = WordcraftDocument::from_doc(doc, Format::Docx);
        let segs = d.segments();
        assert_eq!(segs.iter().map(|s| (s.id.0, s.text.as_str())).collect::<Vec<_>>(), vec![(0, "eins"), (2, "drei")]);
    }
}
