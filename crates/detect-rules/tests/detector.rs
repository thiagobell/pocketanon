//! Whole-detector behaviour: full letters, type and language gating, IBAN/card interplay.

mod common;

use common::*;
use pocketanon_core::{Candidate, DetectRequest, Detector, DetectorKind, EntityType, Span, merge};
use pocketanon_detect_rules::RuleDetector;

fn owned(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

const LETTER_DE: &str = "Sehr geehrter Herr Mustermann,\n\n\
bitte überweisen Sie den Betrag von 1.250,00 € bis zum 15.11.2026 auf das Konto \
IBAN DE89 3704 0044 0532 0130 00 (BIC COBADEFFXXX). Für Rückfragen erreichen Sie mich unter \
+49 228 1234567 oder per E-Mail an max.mustermann@example.com. Weitere Informationen finden \
Sie unter https://www.example.org/hilfe.\n\n\
Antragsteller: Max Mustermann, geb. 12.03.1980, Steuer-ID 86095742719, \
Steuernummer: 123/456/78901, Personalausweisnummer T220001293.\n\
Musterstraße 1, 53757 Sankt Augustin, Tel. (02241) 12 34 56, Fax 02241\u{a0}123457\n\n\
Mit freundlichen Grüßen\nErika Musterfrau";

#[test]
fn german_letter_full_set() {
    let cs = detect_with(LETTER_DE, &all_types(), &["de"]);
    assert_eq!(
        pairs(LETTER_DE, &cs),
        owned(&[
            ("DE89 3704 0044 0532 0130 00", "iban"),
            ("+49 228 1234567", "phone number"),
            ("max.mustermann@example.com", "email"),
            ("https://www.example.org/hilfe", "url"),
            ("12.03.1980", "date of birth"),
            ("86095742719", "tax number"),
            ("123/456/78901", "tax number"),
            ("T220001293", "national id number"),
            ("(02241) 12 34 56", "phone number"),
            ("02241\u{a0}123457", "phone number"),
        ])
    );
}

const LETTER_EN: &str = "Dear Ms Doe,\n\nplease find attached the form for John Doe (date of birth: \
March 12, 1980, passport number C01X00T47, SSN 123-45-6789). You can reach us at (202) 555-0143 or \
user@example.com; card on file: 5555 5555 5555 4444. Meeting on March 3, 2026 at www.example.com/meet.";

#[test]
fn english_letter_full_set() {
    let cs = detect_with(LETTER_EN, &all_types(), &["en"]);
    assert_eq!(
        pairs(LETTER_EN, &cs),
        owned(&[
            ("March 12, 1980", "date of birth"),
            ("C01X00T47", "passport number"),
            ("123-45-6789", "tax number"),
            ("(202) 555-0143", "phone number"),
            ("user@example.com", "email"),
            ("5555 5555 5555 4444", "credit card number"),
            ("www.example.com/meet", "url"),
        ])
    );
}

#[test]
fn types_absent_from_request_are_not_emitted() {
    let only_email = [EntityType::new("email")];
    let cs = detect_with(LETTER_DE, &only_email, &["de", "en"]);
    assert_eq!(pairs(LETTER_DE, &cs), owned(&[("max.mustermann@example.com", "email")]));

    let none: [EntityType; 0] = [];
    assert!(detect_with(LETTER_DE, &none, &["de", "en"]).is_empty());

    // Labels are compared as normalised Entity Types.
    let mixed_case = [EntityType::new(" IBAN "), EntityType::new("Phone  Number")];
    let cs = detect_with(LETTER_DE, &mixed_case, &[]);
    assert_eq!(
        pairs(LETTER_DE, &cs),
        owned(&[("DE89 3704 0044 0532 0130 00", "iban"), ("+49 228 1234567", "phone number")])
    );
}

#[test]
fn german_patterns_do_not_fire_for_english_only() {
    let cs = detect_with(LETTER_DE, &all_types(), &["en"]);
    assert_eq!(
        pairs(LETTER_DE, &cs),
        owned(&[
            ("DE89 3704 0044 0532 0130 00", "iban"),
            ("+49 228 1234567", "phone number"),
            ("max.mustermann@example.com", "email"),
            ("https://www.example.org/hilfe", "url"),
        ])
    );
}

#[test]
fn english_patterns_do_not_fire_for_german_only() {
    let cs = detect_with(LETTER_EN, &all_types(), &["de"]);
    assert_eq!(
        pairs(LETTER_EN, &cs),
        owned(&[
            ("user@example.com", "email"),
            ("5555 5555 5555 4444", "credit card number"),
            ("www.example.com/meet", "url"),
        ])
    );
}

#[test]
fn language_codes_with_region_match() {
    let cs = detect_with("Steuer-ID 86095742719", &all_types(), &["de-DE"]);
    assert_eq!(pairs("Steuer-ID 86095742719", &cs), owned(&[("86095742719", "tax number")]));
}

#[test]
fn iban_digits_are_not_a_credit_card_or_phone() {
    // The tail "3704 0044 0532 0130 00" would otherwise be scanned as card digits, and
    // "0044 0532 0130 00" looks like an international phone number.
    for text in [
        "IBAN DE89 3704 0044 0532 0130 00",
        "IBAN DE89370400440532013000",
        "IBAN DE89 3704 0044 0532 0130 01", // invalid checksum: still not a card or phone
    ] {
        let cs = detect(text);
        assert!(
            cs.iter().all(|c| c.entity_type == EntityType::new("iban")),
            "unexpected candidates in {text:?}: {:?}",
            pairs(text, &cs)
        );
    }
    // Even when only cards are requested.
    let card = [EntityType::new("credit card number")];
    assert!(detect_with("DE89 3704 0044 0532 0130 00", &card, &["de"]).is_empty());
}

#[test]
fn offsets_are_byte_offsets_into_multibyte_text() {
    let text = "Grüße aus Köln – Straße №1: user@example.com · Tel. +49\u{a0}228\u{a0}1234567 ✓";
    let cs = detect(text);
    let start = text.find("user@").unwrap();
    assert_eq!(cs[0].span, Span::new(start, start + "user@example.com".len()));
    assert_eq!(pairs(text, &cs), owned(&[("user@example.com", "email"), ("+49\u{a0}228\u{a0}1234567", "phone number")]));
}

#[test]
fn plain_dates_and_amounts_are_ignored() {
    let text = "Am 12.03.2026 wurden 1.234.567,89 EUR bzw. 0,50 € verbucht; Frist bis 2026-11-30, Az. 4 O 123/26.";
    assert!(detect(text).is_empty(), "{:?}", pairs(text, &detect(text)));
}

#[test]
fn detector_trait_metadata() {
    let d = RuleDetector::default();
    assert_eq!(d.kind(), DetectorKind::Rule);
    assert_eq!(d.name(), "rules");
    assert_eq!(d.max_words(), None);
    let supported = d.supported_types();
    for t in ["email", "phone number", "url", "iban", "credit card number", "passport number", "national id number", "tax number", "date of birth"] {
        assert!(supported.contains(&EntityType::new(t)), "{t}");
    }
    let types = all_types();
    let languages = langs(&["en"]);
    let active = d.active_rules(&DetectRequest { types: &types, languages: &languages });
    assert!(active.contains(&"phone.us"));
    assert!(!active.iter().any(|r| r.contains(".de")));
}

#[test]
fn rule_spans_win_in_merging() {
    // A model candidate that swallowed the trailing period and a word; the rule span wins.
    let text = "IBAN DE89 3704 0044 0532 0130 00.";
    let mut cs = detect(text);
    let start = text.find("DE89").unwrap();
    cs.push(Candidate::new(Span::new(start - 5, text.len()), "customer number", 0.6, DetectorKind::Model));
    let merged = merge(cs);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].text(text), "DE89 3704 0044 0532 0130 00");
    assert_eq!(merged[0].entity_type, EntityType::new("iban"));
}
