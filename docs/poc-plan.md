# PocketAnon proof of concept

Decisions from the planning session on 2026-10-17. Vocabulary is in [CONTEXT.md](../CONTEXT.md); durable decisions are in [docs/adr](./adr).

## Scope

- Text Anonymization and File Anonymization (TXT, DOCX, RTF). No PDF (ADR-0001).
- Detection = Model Detector (GLiNER via `gline-rs` on ONNX Runtime, pure Rust) + Rule Detector (patterns and checksums), merged.
- Replacement Strategy: Indexed Redaction only (plain redaction dropped by the user on 2026-10-17). No pseudonymization.
- Review step with highlights, candidate list, threshold slider, type change, reject, manual span addition (Text workspace first).
- Text Anonymization has no Detect or Apply buttons: detection starts one second after the last keystroke, and the Result follows every Review, Threshold or Strategy change. File Anonymization keeps explicit Detect and Apply & Save, because saving a file should be deliberate.
- Document preview from Wordcraft layout and render crates (ADR-0002).
- Default model: multilingual PII GLiNER, downloaded on first run into the app data dir; local-folder override. Languages are multi-select.
- Settings persist, content never does (ADR-0003).
- UI: egui/eframe, dark theme default, two workspaces (Text, File), a permanent Detection panel titled "What to detect" (languages, type groups, custom types, term rules), a slim settings drawer (model, threshold, theme), Cmd+K options palette, styled after the ArtCraft apps.
- File view: zoom (25–400 %, ⌘+/⌘−/⌘0), findings grouped by page with header/footer items tagged at the bottom of each page, every highlight labelled with its placeholder (`PERSON_1`), right-click menus on highlights and rows (change type, reject, always redact as …).
- Term Rules: select text and right-click to always redact it as a chosen type; stored in settings (ADR-0003 amendment).
- Packaging: `packaging/make-icons.sh` and `packaging/bundle.sh` produce `target/bundle/PocketAnon.app` with the masked-person icon.

## Workspace layout

- `crates/core`: domain types, Detector trait, Merging, Chunking, Replacement Strategies, Summary. No I/O.
- `crates/detect-gliner`: Model Detector, Registry, downloader.
- `crates/detect-rules`: Rule Detector, per-language patterns.
- `crates/documents`: Wordcraft adapter (load, enumerate stories, replace at run level, save, render pages). Only crate depending on Wordcraft.
- `apps/pocketanon`: the eframe application.

## Acceptance

1. German/English letter with names, address, IBAN, email, phone through the Text workspace: review, apply, copy. Under ~2 s per page after model load.
2. DOCX with header, footer, table and footnote: preview renders, highlights shown, result opens in Word with formatting intact and every entity replaced in every story.
3. Same for RTF and TXT.
4. A Custom Type ("project code name") is detected.
5. Relaunch: settings persist, no content trace.

## Test focus (critical parts)

- `core`: Merging (overlaps, nesting, rule-wins-boundaries, higher-confidence-wins-type), Chunking (byte offsets map back exactly, multibyte text, long paragraphs), Indexed Redaction numbering across chunks and repeated entities, Replacement on overlapping/adjacent spans. Property tests for offset round-tripping.
- `detect-rules`: positive and negative fixtures per pattern, IBAN mod-97, card Luhn, German tax number formats with and without spaces.
- `detect-gliner`: integration test gated behind a feature flag (needs the model) on synthetic sentences in DE and EN; tokenizer offset alignment tests.
- `documents`: round-trip fixtures per format; entity straddling run boundaries in DOCX; replacement in header, footer, footnote, comment and table cell; untouched parts byte-compare where the format allows.
- App: smoke test that settings serialize without content fields.

All fixtures are synthetic (Max Mustermann, user@example.com, test IBANs).

## Findings during the build (2026-10-17)

- **The int8 export is unusable; fp16 is the default.** On the German fixture letter the int8 `gliner_multi_pii-v1` export scored "Max Mustermann" at 0.38 and missed "Erika Musterfrau" entirely even at threshold 0.05. The fp16 export (580 MB) scores the same spans at 0.94 to 0.99 and also finds addresses, organisations and the customer number. Cost: about 2 s for the eleven-paragraph letter versus 0.3 s, because every segment is a separate inference call; batching segments into one call is the obvious optimisation. The Review Threshold defaults to 0.3, and the app withholds from the Model Detector the types the Rule Detector owns (email, url, phone number, iban, credit card number).
- **GLiNER includes honorifics** ("Herr Max Mustermann"). The Indexer strips leading titles (Herr, Frau, Mr, Dr, …) from its identity key so the same person keeps one index with or without the title.
- **GLiNER counts punctuation as words**, so the chunker's whitespace word count underestimates; the Model Detector therefore declares a conservative 250-word limit instead of 384.
- **Wordcraft RTF** drops footnotes and comments on import and writes no headers or footers (see ADR-0001).
- **TXT bypasses Wordcraft** for file contents to keep exact line endings and encodings; Wordcraft only renders the preview.
