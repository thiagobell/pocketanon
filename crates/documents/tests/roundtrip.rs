//! Round trips per format on synthetic fixtures built in code (no binary fixtures).

use pocketanon_core::document::{AnonymizableDocument, Format, Segment, SegmentEdits, SegmentKind};
use pocketanon_core::replace::{Indexer, Strategy, apply_replacements, plan_replacements};
use pocketanon_core::types::{Entity, Span};
use pocketanon_documents::load;
use wordcraft_doc::{Block, CharProps, Comment, Document, InlineObject, Paragraph, PartKind, StoryRef, Table, para::NoteKind, para_block};

const NAME: &str = "Max Mustermann";
const EMAIL: &str = "user@example.com";
const PHONE: &str = "+49 30 1234567";
const COMMENT_AUTHOR: &str = "Erika Musterfrau";

/// What the "detectors" find: every occurrence of these strings.
const ENTITIES: &[(&str, &str)] = &[(NAME, "person"), (EMAIL, "email"), (PHONE, "phone"), ("Erika", "person")];

fn plain() -> CharProps {
    CharProps::default()
}
fn bold() -> CharProps {
    CharProps { bold: Some(true), ..Default::default() }
}

fn para(runs: &[(&str, CharProps)]) -> Paragraph {
    let mut p = Paragraph::new();
    for (t, props) in runs {
        let at = p.len();
        p.insert_text(at, t, props).unwrap();
    }
    p
}

fn part(doc: &mut Document, kind: PartKind, text: &str) -> u32 {
    doc.add_part(kind, vec![para_block(para(&[(text, plain())]))])
}

/// Body with a name split over a bold and a plain run, a footnote reference, a commented range
/// and a 2x2 table; header, footer, footnote and comment stories.
fn fixture() -> Document {
    let mut doc = Document::new();
    doc.core.creator = NAME.into();
    doc.core.last_modified_by = NAME.into();

    let header = part(&mut doc, PartKind::Header, &format!("Akte {NAME}"));
    let footer = part(&mut doc, PartKind::Footer, &format!("Kontakt: {EMAIL}"));
    let note = part(&mut doc, PartKind::Footnote, &format!("Telefon {PHONE} (mobil)"));
    let comment_part = part(&mut doc, PartKind::Comment, &format!("Bitte {NAME} anrufen."));
    doc.last_section.headers.default = Some(header);
    doc.last_section.footers.default = Some(footer);
    doc.comments.insert(0, Comment { author: COMMENT_AUTHOR.into(), initials: "EM".into(), part: comment_part, ..Default::default() });

    let greeting = para(&[("Sehr geehrter ", plain()), ("Max", bold()), (" Mustermann", plain()), (", vielen Dank.", plain())]);

    let mut noted = para(&[("Erreichbar wie angegeben", plain())]);
    let at = noted.len();
    noted.insert_object(at, InlineObject::NoteRef { kind: NoteKind::Footnote, id: note, custom: String::new() }, &plain()).unwrap();
    let at = noted.len();
    noted.insert_text(at, ".", &plain()).unwrap();

    let mut commented = para(&[("Dieser Absatz ", plain())]);
    let at = commented.len();
    commented.insert_object(at, InlineObject::CommentStart { id: 0 }, &plain()).unwrap();
    let at = commented.len();
    commented.insert_text(at, "ist kommentiert", &plain()).unwrap();
    let at = commented.len();
    commented.insert_object(at, InlineObject::CommentEnd { id: 0 }, &plain()).unwrap();
    let at = commented.len();
    commented.insert_text(at, ".", &plain()).unwrap();

    let mut table = Table::new(2, 2, 400.0);
    let cells = [["Name", NAME], ["E-Mail", EMAIL]];
    for (r, row) in cells.iter().enumerate() {
        for (c, text) in row.iter().enumerate() {
            table.rows[r].cells[c].blocks = vec![para_block(para(&[(text, plain())]))];
        }
    }

    doc.body = vec![
        para_block(greeting),
        para_block(noted),
        para_block(commented),
        std::sync::Arc::new(Block::Table(table)),
        para_block(para(&[("Mit freundlichen Grüßen", plain())])),
    ];
    doc.ensure_nonempty();
    doc
}

fn entities_in(text: &str) -> Vec<Entity> {
    let mut out: Vec<Entity> = Vec::new();
    for (needle, t) in ENTITIES {
        for (i, _) in text.match_indices(needle) {
            let span = Span::new(i, i + needle.len());
            if !out.iter().any(|e| e.span.overlaps(&span)) {
                out.push(Entity::new(span, *t));
            }
        }
    }
    out
}

/// Plan with core exactly as the app does; returns the edits and the expected new segment texts.
fn plan(segments: &[Segment]) -> (Vec<SegmentEdits>, Vec<(SegmentKind, String)>) {
    let mut ix = Indexer::new(Strategy::IndexedRedaction);
    let mut edits = Vec::new();
    let mut expected = Vec::new();
    for s in segments {
        let (reps, _) = plan_replacements(&s.text, &entities_in(&s.text), &mut ix);
        expected.push((s.kind, apply_replacements(&s.text, &reps)));
        if !reps.is_empty() {
            edits.push(SegmentEdits { segment: s.id, replacements: reps });
        }
    }
    (edits, expected)
}

fn kinds_and_texts(d: &dyn AnonymizableDocument) -> Vec<(SegmentKind, String)> {
    d.segments().into_iter().map(|s| (s.kind, s.text)).collect()
}

fn assert_no_entities(segs: &[(SegmentKind, String)]) {
    for (kind, text) in segs {
        for (needle, _) in ENTITIES {
            assert!(!text.contains(needle), "{needle:?} survived in {kind} segment {text:?}");
        }
    }
}

fn assert_runs_cover(doc: &Document) {
    let stories = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k)));
    for s in stories {
        for path in doc.para_paths(s) {
            let p = doc.para(s, &path).unwrap();
            assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), p.text.len(), "runs of {:?}", p.text);
        }
    }
}

/// The body paragraph whose text contains `needle`, and the offset.
fn find_body(doc: &Document, needle: &str) -> (Paragraph, usize) {
    for path in doc.para_paths(StoryRef::Body) {
        let p = doc.para(StoryRef::Body, &path).unwrap();
        if let Some(i) = p.text.find(needle) {
            return (p.clone(), i);
        }
    }
    panic!("{needle:?} not in body");
}

#[test]
fn docx_round_trip_anonymizes_every_story() {
    let bytes = wordcraft_docx::write(&fixture()).unwrap();
    let mut d = load(&bytes, Format::Docx).unwrap();
    assert_eq!(d.format(), Format::Docx);

    let segs = d.segments();
    let has = |kind: SegmentKind, needle: &str| segs.iter().any(|s| s.kind == kind && s.text.contains(needle));
    assert!(has(SegmentKind::Body, "Sehr geehrter Max Mustermann, vielen Dank."), "{segs:#?}");
    assert!(has(SegmentKind::Body, NAME) && has(SegmentKind::Body, EMAIL), "table cells are body segments");
    assert!(has(SegmentKind::Header, NAME), "{segs:#?}");
    assert!(has(SegmentKind::Footer, EMAIL), "{segs:#?}");
    assert!(has(SegmentKind::Footnote, PHONE), "{segs:#?}");
    assert!(has(SegmentKind::Comment, NAME), "{segs:#?}");

    let (edits, expected) = plan(&segs);
    d.apply(&edits).unwrap();
    assert_eq!(kinds_and_texts(d.as_ref()), expected, "segments() reflects the edits");

    let out = d.save().unwrap();
    let again = load(&out, Format::Docx).unwrap();
    let reloaded = kinds_and_texts(again.as_ref());
    assert_no_entities(&reloaded);
    // Every placeholder in the right kind of segment, and all other text unchanged.
    assert_eq!(reloaded, expected);
    for (kind, ph) in [
        (SegmentKind::Body, "Sehr geehrter [PERSON_1], vielen Dank."),
        (SegmentKind::Body, "[EMAIL_1]"),
        (SegmentKind::Header, "Akte [PERSON_1]"),
        (SegmentKind::Footer, "Kontakt: [EMAIL_1]"),
        (SegmentKind::Footnote, "Telefon [PHONE_1] (mobil)"),
        (SegmentKind::Comment, "Bitte [PERSON_1] anrufen."),
    ] {
        assert!(reloaded.iter().any(|(k, t)| *k == kind && t.contains(ph)), "{ph:?} not in a {kind} segment: {reloaded:#?}");
    }

    // Formatting at the model level: bold carried onto the placeholder, neighbours untouched.
    let doc = wordcraft_docx::read(&out).unwrap();
    assert_runs_cover(&doc);
    let (p, at) = find_body(&doc, "[PERSON_1]");
    assert_eq!(p.props_of_char(at).bold, Some(true), "first run (bold \"Max\") wins");
    assert_eq!(p.props_of_char(at + "[PERSON_1]".len() - 1).bold, Some(true));
    assert_ne!(p.props_of_char(0).bold, Some(true), "\"Sehr geehrter \" stays plain");
    assert_ne!(p.props_of_char(at + "[PERSON_1]".len()).bold, Some(true), "\", vielen Dank.\" stays plain");
    // Footnote reference and comment range survived.
    let (p, _) = find_body(&doc, "Erreichbar");
    assert!(p.objects.iter().any(|o| matches!(o, InlineObject::NoteRef { .. })));
    let (p, _) = find_body(&doc, "kommentiert");
    assert!(p.objects.iter().any(|o| matches!(o, InlineObject::CommentStart { .. })));

    // Personal metadata outside the text is gone too.
    assert!(!doc.core.creator.contains("Max") && !doc.core.last_modified_by.contains("Max"));
    assert_eq!(doc.comments.len(), 1);
    assert!(doc.comments.values().all(|c| c.author != COMMENT_AUTHOR), "{:?}", doc.comments);
}

#[test]
fn rtf_round_trip_keeps_structure_and_bold() {
    let mut doc = fixture();
    // RTF goes through the flow model: keep to what it carries (paragraphs, runs, tables).
    doc.body.retain(|b| !matches!(&**b, Block::Para(p) if !p.objects.is_empty()));
    let rtf = wordcraft_formats::export("rtf", &doc).unwrap().unwrap();
    let mut d = load(&rtf, Format::Rtf).unwrap();
    let segs = d.segments();
    assert!(segs.iter().any(|s| s.text == "Sehr geehrter Max Mustermann, vielen Dank."), "{segs:#?}");
    assert!(segs.iter().any(|s| s.text == NAME), "table cell: {segs:#?}");

    let (edits, expected) = plan(&segs);
    d.apply(&edits).unwrap();
    let out = d.save().unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(!text.contains("Mustermann") && !text.contains(EMAIL), "{text}");

    let again = load(&out, Format::Rtf).unwrap();
    let reloaded = kinds_and_texts(again.as_ref());
    assert_no_entities(&reloaded);
    let body = |v: &[(SegmentKind, String)]| v.iter().filter(|(k, _)| *k == SegmentKind::Body).cloned().collect::<Vec<_>>();
    assert_eq!(body(&reloaded), body(&expected), "paragraphs and table cells survive");

    let doc = wordcraft_formats::import("rtf", &out).unwrap().unwrap();
    assert_runs_cover(&doc);
    let (p, at) = find_body(&doc, "[PERSON_1]");
    assert_eq!(p.props_of_char(at).bold, Some(true));
    assert_ne!(p.props_of_char(0).bold, Some(true));
    assert!(doc.body.iter().any(|b| matches!(&**b, Block::Table(t) if t.rows.len() == 2 && t.rows[0].cells.len() == 2)));
}

#[test]
fn txt_round_trip_is_exact() {
    let src = "Sehr geehrter Max Mustermann 👋,\r\n\r\nGrüße aus Köln – schreiben Sie an user@example.com.\r\nÄÖÜ äöü ß 🎉\tEnde\r\n";
    let d = load(src.as_bytes(), Format::Txt).unwrap();
    assert_eq!(d.save().unwrap(), src.as_bytes(), "no edits: identical bytes");

    let mut d = load(src.as_bytes(), Format::Txt).unwrap();
    let segs = d.segments();
    assert_eq!(segs.len(), 3, "empty lines are not segments: {segs:#?}");
    let (edits, _) = plan(&segs);
    d.apply(&edits).unwrap();
    let out = String::from_utf8(d.save().unwrap()).unwrap();
    assert_eq!(out, "Sehr geehrter [PERSON_1] 👋,\r\n\r\nGrüße aus Köln – schreiben Sie an [EMAIL_1].\r\nÄÖÜ äöü ß 🎉\tEnde\r\n");

    // LF-only, BOM, no trailing newline, and a legacy encoding stay as they were.
    for src in [&b"a Max Mustermann\nb\n"[..], b"\xEF\xBB\xBFMax Mustermann", b"Gr\xfc\xdfe Max Mustermann\r\n"] {
        let mut d = load(src, Format::Txt).unwrap();
        let (edits, _) = plan(&d.segments());
        d.apply(&edits).unwrap();
        let out = d.save().unwrap();
        let needle: &[u8] = b"Max Mustermann";
        let at = src.windows(needle.len()).position(|w| w == needle).unwrap();
        let mut expected = src[..at].to_vec();
        expected.extend_from_slice(b"[PERSON_1]");
        expected.extend_from_slice(&src[at + needle.len()..]);
        assert_eq!(out, expected);
    }
}

#[test]
fn docx_alignment_with_field_and_bookmark_after_reload() {
    let mut doc = Document::new();
    let mut p = para(&[("Seite ", plain())]);
    let at = p.len();
    p.insert_object(at, InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() }, &plain())
        .unwrap();
    let at = p.len();
    p.insert_text(at, ", Ansprechpartner ", &plain()).unwrap();
    let at = p.len();
    p.insert_object(at, InlineObject::BookmarkStart { name: "kontakt".into() }, &plain()).unwrap();
    let at = p.len();
    p.insert_text(at, NAME, &bold()).unwrap();
    let at = p.len();
    p.insert_object(at, InlineObject::BookmarkEnd { name: "kontakt".into() }, &plain()).unwrap();
    doc.body = vec![para_block(p)];
    let bytes = wordcraft_docx::write(&doc).unwrap();

    let read = wordcraft_docx::read(&bytes).unwrap();
    let (stored, name_at) = find_body(&read, NAME);
    assert!(stored.text.contains('\u{FFFC}'), "objects stay in the stored text: {:?}", stored.text);

    let mut d = load(&bytes, Format::Docx).unwrap();
    let seg = d.segments().into_iter().find(|s| s.text.contains(NAME)).unwrap();
    assert_eq!(seg.text.len(), stored.text.len());
    assert_eq!(seg.text.find(NAME), Some(name_at), "segment offset == Pos.off");
    assert!(!seg.text.contains('\u{FFFC}'));

    let (edits, _) = plan(std::slice::from_ref(&seg));
    d.apply(&edits).unwrap();
    let out = wordcraft_docx::read(&d.save().unwrap()).unwrap();
    let (p, at) = find_body(&out, "[PERSON_1]");
    assert_eq!(p.props_of_char(at).bold, Some(true));
    assert!(p.objects.iter().any(|o| matches!(o, InlineObject::Field { .. })), "the field outside the span survives");
    assert!(!p.text.contains("Mustermann"));
}

#[test]
fn preview_renders_and_locates() {
    let bytes = wordcraft_docx::write(&fixture()).unwrap();
    let d = load(&bytes, Format::Docx).unwrap();
    assert!(d.page_count() >= 1);
    let img = d.render_page(0, 1.0).expect("page 0");
    assert!(img.width > 0 && img.height > 0);
    assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
    let first = &img.rgba[..4];
    assert!(img.rgba.chunks_exact(4).any(|px| px != first), "page is not one flat colour");
    assert!(d.render_page(d.page_count(), 1.0).is_none());

    let seg = d.segments().into_iter().find(|s| s.kind == SegmentKind::Body && s.text.contains(NAME)).unwrap();
    let at = seg.text.find(NAME).unwrap();
    for scale in [1.0f32, 2.0] {
        let placements = d.locate(seg.id, Span::new(at, at + NAME.len()), scale);
        assert!(!placements.is_empty(), "body span is on a page");
        let (w, h) = if scale == 1.0 { (img.width as f32, img.height as f32) } else { (img.width as f32 * 2.0, img.height as f32 * 2.0) };
        for p in &placements {
            assert_eq!(p.page, 0);
            assert!(!p.rects.is_empty());
            for r in &p.rects {
                assert!(r.w > 0.0 && r.h > 0.0, "{r:?}");
                assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w + 1.0 && r.y + r.h <= h + 1.0, "{r:?} outside {w}x{h}");
            }
        }
    }

    // Header text locates too (it repeats; the first page showing it is enough).
    let header = d.segments().into_iter().find(|s| s.kind == SegmentKind::Header).unwrap();
    assert!(!d.locate(header.id, Span::new(0, header.text.len()), 1.0).is_empty());

    // TXT has a preview as well.
    let t = load("Max Mustermann\nzweite Zeile".as_bytes(), Format::Txt).unwrap();
    assert!(t.page_count() >= 1);
    assert!(!t.locate(t.segments()[0].id, Span::new(0, 3), 1.0).is_empty());
}
