# PocketAnon — notes for Claude

- Read `CONTEXT.md` first and use its vocabulary (Entity, Candidate, Detector, Merging, Review, Replacement Strategy, Fidelity, Segment). Don't invent synonyms.
- Decisions with rationale are in `docs/adr/`. ADR-0003 is a hard rule: never persist content, file paths, histories or the redaction mapping.
- `crates/core` has no I/O and no dependency on models, files or UI. Keep it that way; everything format- or model-specific lives in the other crates behind core's traits.
- All span offsets are **byte offsets** into the exact `&str` analysed, on char boundaries. Test with umlauts and emoji.
- Wordcraft types must not leak out of `crates/documents` (ADR-0002).
- Test data is synthetic only (Max Mustermann, user@example.com, test IBANs). Never add real personal data to fixtures or tests.
- Build per crate (`cargo test -p <crate>`); the documents crate is slow to compile.
