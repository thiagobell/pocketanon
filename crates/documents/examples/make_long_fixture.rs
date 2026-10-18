//! Builds a synthetic ~10-page DOCX (`fixtures/report_de.docx`) for performance checks:
//! headings, short list items, long paragraphs and a table per section. Synthetic names only.
//!
//! `cargo run -p pocketanon-documents --example make_long_fixture -- fixtures [sections]`
use std::sync::Arc;

use wordcraft_doc::{Block, CharProps, Document, Paragraph, PartKind, Table, para_block};

fn para(t: &str) -> Arc<Block> {
    let mut p = Paragraph::new();
    p.insert_text(0, t, &CharProps::default()).unwrap();
    para_block(p)
}

const NAMES: &[&str] = &["Max Mustermann", "Erika Musterfrau", "John Smith", "Jane Doe", "Hans Beispiel", "Maria Muster"];
const CITIES: &[&str] = &["Bonn", "Berlin", "London", "Hamburg", "Köln", "München"];

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "fixtures".into());
    let sections: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(12);
    let mut doc = Document::new();
    doc.core.creator = "Erika Musterfrau".into();
    let header = doc.add_part(PartKind::Header, vec![para("Musterfirma GmbH · Jahresbericht · vertraulich")]);
    let footer = doc.add_part(PartKind::Footer, vec![para("Kontakt: info@example.com · +49 228 7654321")]);
    doc.last_section.headers.default = Some(header);
    doc.last_section.footers.default = Some(footer);

    let mut body: Vec<Arc<Block>> = Vec::new();
    for s in 0..sections {
        let n1 = NAMES[s % NAMES.len()];
        let n2 = NAMES[(s + 1) % NAMES.len()];
        let city = CITIES[s % CITIES.len()];
        body.push(para(&format!("{}. Abschnitt: Projekt {city}", s + 1)));
        body.push(para(&format!("Verantwortlich: {n1}")));
        body.push(para(&format!("Stellvertretung: {n2}, Tel. +49 171 {:07}", 1000000 + s)));
        body.push(para(&format!("E-Mail: user{s}@example.com")));
        body.push(para(""));
        for p in 0..3 {
            body.push(para(&format!(
                "Im Berichtszeitraum hat {n1} gemeinsam mit {n2} das Projekt in {city} betreut. \
                 Die Abstimmung mit der Niederlassung in {} erfolgte wöchentlich; Rückfragen beantwortete \
                 {n2} unter {n2_mail}. Der Kunde, vertreten durch Herrn {n1}, geboren am 0{}.0{}.1980, \
                 wohnhaft in der Musterstraße {}, 5311{} {city}, bestätigte den Abschluss der Phase {p}. \
                 Zahlungen liefen über die IBAN DE89 3704 0044 0532 0130 00 bei der Beispielbank. \
                 Our partner office, managed by {n2}, forwarded the final documents to the tax office in {city} \
                 on time, and the steering committee approved the budget for the next quarter without changes. \
                 Weitere Details finden sich im Anhang sowie im Protokoll vom 12. März 2024.",
                CITIES[(s + 1) % CITIES.len()],
                (s % 9) + 1,
                (p % 9) + 1,
                s + 1,
                s % 10,
                n2_mail = format!("kontakt{s}@example.com"),
            )));
            body.push(para(""));
        }
        let mut table = Table::new(3, 2, 400.0);
        let cells = [["Kunde", n1], ["Ort", city], ["Sachbearbeitung", n2]];
        for (r, row) in cells.iter().enumerate() {
            for (c, t) in row.iter().enumerate() {
                table.rows[r].cells[c].blocks = vec![para(t)];
            }
        }
        body.push(Arc::new(Block::Table(table)));
        body.push(para(""));
    }
    doc.body = body;
    doc.ensure_nonempty();
    let docx = wordcraft_docx::write(&doc).expect("write docx");
    std::fs::write(format!("{out_dir}/report_de.docx"), docx).unwrap();
    println!("wrote {out_dir}/report_de.docx ({sections} sections)");
}
