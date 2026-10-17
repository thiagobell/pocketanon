//! Builds the synthetic DOCX and RTF acceptance fixtures from `fixtures/letter_de.txt`:
//! header, footer, footnote, comment and a table, with one name split across a bold run.
//!
//! `cargo run -p pocketanon-documents --example make_fixture -- fixtures`
use std::sync::Arc;

use wordcraft_doc::{Block, CharProps, Comment, Document, InlineObject, Paragraph, PartKind, Table, para::NoteKind, para_block};

fn para(runs: &[(&str, CharProps)]) -> Paragraph {
    let mut p = Paragraph::new();
    for (t, props) in runs {
        let at = p.len();
        p.insert_text(at, t, props).unwrap();
    }
    p
}

fn plain() -> CharProps {
    CharProps::default()
}

fn bold() -> CharProps {
    CharProps { bold: Some(true), ..Default::default() }
}

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "fixtures".into());
    let text = std::fs::read_to_string(format!("{out_dir}/letter_de.txt")).expect("fixtures/letter_de.txt");

    let mut doc = Document::new();
    doc.core.creator = "Erika Musterfrau".into();
    let header = doc.add_part(PartKind::Header, vec![para_block(para(&[("Musterfirma GmbH · Akte Max Mustermann", plain())]))]);
    let footer = doc.add_part(PartKind::Footer, vec![para_block(para(&[("Kontakt: erika.musterfrau@example.com · +49 228 7654321", plain())]))]);
    let note = doc.add_part(PartKind::Footnote, vec![para_block(para(&[("Rückfragen an Max Mustermann, Tel. +49 171 1234567.", plain())]))]);
    let comment = doc.add_part(PartKind::Comment, vec![para_block(para(&[("Bitte Erika Musterfrau informieren.", plain())]))]);
    doc.last_section.headers.default = Some(header);
    doc.last_section.footers.default = Some(footer);
    doc.comments.insert(0, Comment { author: "Erika Musterfrau".into(), initials: "EM".into(), part: comment, ..Default::default() });

    let mut body: Vec<Arc<Block>> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut p = if let Some(rest) = line.strip_prefix("Herr Max Mustermann") {
            // split the name across runs: bold "Max" + plain " Mustermann"
            para(&[("Herr ", plain()), ("Max", bold()), (" Mustermann", plain()), (rest, plain())])
        } else {
            para(&[(line, plain())])
        };
        if i == 8 {
            // footnote reference at the end of the first long paragraph
            let at = p.len();
            p.insert_object(at, InlineObject::NoteRef { kind: NoteKind::Footnote, id: note, custom: String::new() }, &plain()).unwrap();
        }
        if line.starts_with("Sehr geehrter") {
            p.insert_object(0, InlineObject::CommentStart { id: 0 }, &plain()).unwrap();
            let at = p.len();
            p.insert_object(at, InlineObject::CommentEnd { id: 0 }, &plain()).unwrap();
        }
        body.push(para_block(p));
    }
    let mut table = Table::new(3, 2, 400.0);
    let cells = [["Kunde", "Max Mustermann"], ["IBAN", "DE89 3704 0044 0532 0130 00"], ["Sachbearbeiterin", "Erika Musterfrau"]];
    for (r, row) in cells.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            table.rows[r].cells[c].blocks = vec![para_block(para(&[(t, plain())]))];
        }
    }
    body.push(Arc::new(Block::Table(table)));
    doc.body = body;
    doc.ensure_nonempty();

    let docx = wordcraft_docx::write(&doc).expect("write docx");
    std::fs::write(format!("{out_dir}/letter_de.docx"), docx).unwrap();
    let rtf = wordcraft_formats::export("rtf", &doc).expect("rtf supported").expect("write rtf");
    std::fs::write(format!("{out_dir}/letter_de.rtf"), rtf).unwrap();
    println!("wrote {out_dir}/letter_de.docx and {out_dir}/letter_de.rtf");
}
