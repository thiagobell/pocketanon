//! Fixture-driven positive and negative tests per pattern. All data is synthetic.

mod common;

use common::*;

const EMAIL: &str = "email";
const URL: &str = "url";
const PHONE: &str = "phone number";
const IBAN: &str = "iban";
const CARD: &str = "credit card number";
const TAX: &str = "tax number";
const NATIONAL_ID: &str = "national id number";
const PASSPORT: &str = "passport number";
const DOB: &str = "date of birth";

/// (input, entity type, expected matches)
fn check(fixtures: &[(&str, &str, &[&str])]) {
    for (text, ty, expected) in fixtures {
        assert_finds(text, ty, expected);
    }
}

#[test]
fn email_fixtures() {
    check(&[
        ("Schreiben Sie an user@example.com.", EMAIL, &["user@example.com"]),
        ("(max.mustermann+test@mail.example.co.uk)", EMAIL, &["max.mustermann+test@mail.example.co.uk"]),
        ("E-Mail: jürgen.müller@beispiel.de, danke", EMAIL, &["jürgen.müller@beispiel.de"]),
        ("<info@example-firma.de>", EMAIL, &["info@example-firma.de"]),
        ("a@b.c ist zu kurz", EMAIL, &[]),
        ("user@localhost", EMAIL, &[]),
        ("Preis @ 5 Euro", EMAIL, &[]),
        ("user@.example.com", EMAIL, &[]),
    ]);
}

#[test]
fn url_fixtures() {
    check(&[
        ("Siehe https://www.example.org/hilfe.", URL, &["https://www.example.org/hilfe"]),
        ("(siehe www.example.com)", URL, &["www.example.com"]),
        ("http://example.com/a?b=1&c=2, und", URL, &["http://example.com/a?b=1&c=2"]),
        ("https://en.example.org/wiki/Foo_(bar) ok", URL, &["https://en.example.org/wiki/Foo_(bar)"]),
        ("[https://example.com/x]", URL, &["https://example.com/x"]),
        ("Link: https://example.com/pfad/äöü!", URL, &["https://example.com/pfad/äöü"]),
        ("Kein Link: https:// oder www.", URL, &[]),
        ("awww.example.com", URL, &[]),
    ]);
}

#[test]
fn phone_fixtures() {
    check(&[
        ("Tel. +49 228 1234567.", PHONE, &["+49 228 1234567"]),
        ("Tel.: +49 (0) 228 123456", PHONE, &["+49 (0) 228 123456"]),
        ("+49-171-1234567, abends", PHONE, &["+49-171-1234567"]),
        ("0049 228 1234567", PHONE, &["0049 228 1234567"]),
        ("Ruf an: 0228 123456!", PHONE, &["0228 123456"]),
        ("Mobil 0171/1234567", PHONE, &["0171/1234567"]),
        ("Tel. (0228) 12 34 56", PHONE, &["(0228) 12 34 56"]),
        ("(0228 123456)", PHONE, &["0228 123456"]),
        ("Durchwahl 0228 / 12345-67", PHONE, &["0228 / 12345-67"]),
        ("Tel\u{a0}+49\u{a0}228\u{a0}1234567", PHONE, &["+49\u{a0}228\u{a0}1234567"]),
        ("Call +1 202 555 0143 now", PHONE, &["+1 202 555 0143"]),
        ("Call (202) 555-0143 or 202-555-0143.", PHONE, &["(202) 555-0143", "202-555-0143"]),
        // negatives
        ("Termin am 01/03/1980 um 10 Uhr", PHONE, &[]),
        ("Zu kurz: 0228 12", PHONE, &[]),
        ("Betrag 0123 456 789 €", PHONE, &[]),
        ("Datum 01.03.1980", PHONE, &[]),
        ("Rechnung 2024-0001", PHONE, &[]),
        ("Art.-Nr. A0228123456", PHONE, &[]),
    ]);
}

#[test]
fn phone_confidence() {
    assert_eq!(confidence_of("Tel. +49 228 1234567", "+49 228 1234567", PHONE), 0.9);
    assert_eq!(confidence_of("Tel. 0049 228 1234567", "0049 228 1234567", PHONE), 0.9);
    assert_eq!(confidence_of("Tel. 0228 1234567", "0228 1234567", PHONE), 0.8);
}

#[test]
fn iban_fixtures() {
    check(&[
        ("IBAN: DE89 3704 0044 0532 0130 00.", IBAN, &["DE89 3704 0044 0532 0130 00"]),
        ("IBAN DE89370400440532013000, BIC COBADEFFXXX", IBAN, &["DE89370400440532013000"]),
        ("DE89\u{a0}3704\u{a0}0044\u{a0}0532\u{a0}0130\u{a0}00", IBAN, &["DE89\u{a0}3704\u{a0}0044\u{a0}0532\u{a0}0130\u{a0}00"]),
        ("GB82 WEST 1234 5698 7654 32 (UK)", IBAN, &["GB82 WEST 1234 5698 7654 32"]),
        ("AT61 1904 3002 3457 3201 BIC XY", IBAN, &["AT61 1904 3002 3457 3201"]),
        ("Konten: DE89 3704 0044 0532 0130 00 und NL91ABNA0417164300", IBAN, &["DE89 3704 0044 0532 0130 00", "NL91ABNA0417164300"]),
        // negatives: altered digit, altered check digits, truncated, glued to a word
        ("IBAN: DE89 3704 0044 0532 0130 01", IBAN, &[]),
        ("IBAN: DE88 3704 0044 0532 0130 00", IBAN, &[]),
        ("IBAN: DE89 3704 0044 0532 0130", IBAN, &[]),
        ("XDE89370400440532013000", IBAN, &[]),
    ]);
    assert_eq!(confidence_of("DE89 3704 0044 0532 0130 00", "DE89 3704 0044 0532 0130 00", IBAN), 1.0);
}

#[test]
fn credit_card_fixtures() {
    check(&[
        ("Karte 4111 1111 1111 1111.", CARD, &["4111 1111 1111 1111"]),
        ("Karte 4111111111111111", CARD, &["4111111111111111"]),
        ("MC 5555-5555-5555-4444, gültig", CARD, &["5555-5555-5555-4444"]),
        ("Amex 3782 822463 10005", CARD, &["3782 822463 10005"]),
        ("4111\u{a0}1111\u{a0}1111\u{a0}1111", CARD, &["4111\u{a0}1111\u{a0}1111\u{a0}1111"]),
        ("Karte 4111 1111 1111 1111 2026", CARD, &["4111 1111 1111 1111"]),
        // negatives: Luhn fails, wrong prefix, mixed separators, part of a longer number
        ("Karte 4111 1111 1111 1112", CARD, &[]),
        ("Ref 1234 5678 9012 3452", CARD, &[]),
        ("4111-1111 1111-1111", CARD, &[]),
        ("Nr. 94111111111111111", CARD, &[]),
        ("Nr. 12 4111 1111 1111 1111", CARD, &[]), // glued to a longer number: stay conservative
    ]);
    assert_eq!(confidence_of("4111 1111 1111 1111", "4111 1111 1111 1111", CARD), 1.0);
}

#[test]
fn tax_number_de_fixtures() {
    check(&[
        ("Steuer-ID: 86095742719.", TAX, &["86095742719"]),
        ("Identifikationsnummer 86 095 742 719", TAX, &["86 095 742 719"]),
        ("Steuernummer: 123/456/78901", TAX, &["123/456/78901"]),
        ("St.-Nr. 12/345/67890", TAX, &["12/345/67890"]),
        ("StNr 1234567890123,", TAX, &["1234567890123"]),
        ("Steuer-Nr.: 181/815/08155", TAX, &["181/815/08155"]),
        ("Steuernummer 013 815 08153", TAX, &["013 815 08153"]),
        // negatives: wrong check digit, no cue, cue too far away
        ("Steuer-ID: 86095742718", TAX, &[]),
        ("Aktenzeichen 123/456/78901", TAX, &[]),
        ("Steuernummer: siehe den beigefügten Bescheid 123/456/78901", TAX, &[]),
    ]);
    assert_eq!(confidence_of("Steuer-ID 86095742719", "86095742719", TAX), 1.0);
    assert_eq!(confidence_of("Steuernummer 12/345/67890", "12/345/67890", TAX), 0.85);
}

#[test]
fn tax_number_us_fixtures() {
    check(&[
        ("SSN 123-45-6789.", TAX, &["123-45-6789"]),
        ("SSN 000-12-3456", TAX, &[]),
        ("SSN 666-12-3456", TAX, &[]),
        ("SSN 912-34-5678", TAX, &[]),
        ("SSN 123-00-6789", TAX, &[]),
        ("SSN 123-45-0000", TAX, &[]),
        ("Nr 9123-45-6789", TAX, &[]),
    ]);
    assert_eq!(confidence_of("SSN 123-45-6789", "123-45-6789", TAX), 0.9);
}

#[test]
fn national_id_fixtures() {
    check(&[
        ("Personalausweisnummer: T220001293.", NATIONAL_ID, &["T220001293"]),
        ("Ausweis-Nr. L01X00T47", NATIONAL_ID, &["L01X00T47"]),
        // negatives: bad check digit, no cue, forbidden letters
        ("Personalausweisnummer: T220001294", NATIONAL_ID, &[]),
        ("Bestellnummer T220001293", NATIONAL_ID, &[]),
        ("Ausweis: ABCDE1234", NATIONAL_ID, &[]),
    ]);
    assert_eq!(confidence_of("Personalausweis T220001293", "T220001293", NATIONAL_ID), 0.9);
    assert_eq!(confidence_of("Ausweisnummer L01X00T47", "L01X00T47", NATIONAL_ID), 0.8);
}

#[test]
fn passport_fixtures() {
    check(&[
        ("Reisepass Nr. C01X00T47, ausgestellt", PASSPORT, &["C01X00T47"]),
        ("Passnummer: C4J8K2L9M", PASSPORT, &["C4J8K2L9M"]),
        ("passport no. 123456789", PASSPORT, &["123456789"]),
        ("Passport number: AB1234567.", PASSPORT, &["AB1234567"]),
        // negatives
        ("Bestellung C01X00T47", PASSPORT, &[]),
        ("Reisepass: ABCDEFGHI", PASSPORT, &[]),
        ("Reisepass verloren, Anzeige 2026 unter Aktenzeichen C01X00T47", PASSPORT, &[]),
    ]);
    assert_eq!(confidence_of("Reisepass C01X00T47", "C01X00T47", PASSPORT), 0.8);
}

#[test]
fn date_of_birth_fixtures() {
    check(&[
        ("Max Mustermann, geb. 12.03.1980, wohnhaft", DOB, &["12.03.1980"]),
        ("geboren am 12. März 1980 in Bonn", DOB, &["12. März 1980"]),
        ("Geburtsdatum: 1980-03-12", DOB, &["1980-03-12"]),
        ("Geburtsdatum 12/03/1980.", DOB, &["12/03/1980"]),
        ("born on March 12, 1980.", DOB, &["March 12, 1980"]),
        ("Date of birth: 12 March 1980", DOB, &["12 March 1980"]),
        ("DOB 03/12/1980", DOB, &["03/12/1980"]),
        ("geb.\u{a0}1.\u{a0}2.\u{a0}1980", DOB, &["1.\u{a0}2.\u{a0}1980"]),
        // negatives: no cue, implausible, too far from the cue
        ("Termin am 12.03.1980", DOB, &[]),
        ("Rechnung vom March 12, 1980", DOB, &[]),
        ("Geburtsdatum: 45.13.1980", DOB, &[]),
        ("Geburtsdatum siehe unten, das Schreiben ist vom 12.03.2026", DOB, &[]),
    ]);
    assert_eq!(confidence_of("geb. 12.03.1980", "12.03.1980", DOB), 0.9);
}
