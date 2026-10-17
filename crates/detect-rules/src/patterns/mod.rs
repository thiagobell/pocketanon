//! Built-in patterns, one module per category.
//!
//! Each [`Rule`] produces candidates for exactly one Entity Type and is optionally bound to one
//! Language. Language-independent rules always run.

use pocketanon_core::Span;

use crate::scan::Found;

pub(crate) mod contact;
pub(crate) mod dates;
pub(crate) mod financial;
pub(crate) mod identity;
pub(crate) mod tax;

/// Per-call state shared by all rules.
pub(crate) struct Ctx<'t> {
    pub text: &'t str,
    /// Spans that look like an IBAN, valid or not. Card and phone rules skip anything overlapping
    /// them, so IBAN digit tails are never reported as something else.
    pub iban_like: Vec<Span>,
}

impl Ctx<'_> {
    pub fn in_iban(&self, start: usize, end: usize) -> bool {
        let s = Span::new(start, end);
        self.iban_like.iter().any(|i| i.overlaps(&s))
    }
}

pub(crate) struct Rule {
    /// Stable identifier, e.g. `"phone.de"`.
    pub id: &'static str,
    /// Built-in Entity Type label from the catalog.
    pub entity_type: &'static str,
    /// ISO 639-1 code this rule needs, or `None` for language-independent rules.
    pub language: Option<&'static str>,
    pub find: fn(&Ctx<'_>, &mut Vec<Found>),
}

pub(crate) const EMAIL: &str = "email";
pub(crate) const PHONE: &str = "phone number";
pub(crate) const URL: &str = "url";
pub(crate) const IBAN: &str = "iban";
pub(crate) const CARD: &str = "credit card number";
pub(crate) const PASSPORT: &str = "passport number";
pub(crate) const NATIONAL_ID: &str = "national id number";
pub(crate) const TAX: &str = "tax number";
pub(crate) const DOB: &str = "date of birth";

pub(crate) static RULES: &[Rule] = &[
    Rule { id: "email", entity_type: EMAIL, language: None, find: contact::email },
    Rule { id: "url", entity_type: URL, language: None, find: contact::url },
    Rule { id: "phone.international", entity_type: PHONE, language: None, find: contact::phone_international },
    Rule { id: "phone.de", entity_type: PHONE, language: Some("de"), find: contact::phone_de },
    Rule { id: "phone.us", entity_type: PHONE, language: Some("en"), find: contact::phone_us },
    Rule { id: "iban", entity_type: IBAN, language: None, find: financial::iban },
    Rule { id: "credit_card", entity_type: CARD, language: None, find: financial::credit_card },
    Rule { id: "tax.de.steuer_id", entity_type: TAX, language: Some("de"), find: tax::steuer_id },
    Rule { id: "tax.de.steuernummer", entity_type: TAX, language: Some("de"), find: tax::steuernummer },
    Rule { id: "tax.us.ssn", entity_type: TAX, language: Some("en"), find: tax::ssn },
    Rule { id: "national_id.de", entity_type: NATIONAL_ID, language: Some("de"), find: identity::personalausweis },
    Rule { id: "passport.de", entity_type: PASSPORT, language: Some("de"), find: identity::passport_de },
    Rule { id: "passport.en", entity_type: PASSPORT, language: Some("en"), find: identity::passport_en },
    Rule { id: "date_of_birth.de", entity_type: DOB, language: Some("de"), find: dates::date_of_birth_de },
    Rule { id: "date_of_birth.en", entity_type: DOB, language: Some("en"), find: dates::date_of_birth_en },
];
