# PocketAnon

**Proof of concept.** A local desktop app, written in Rust, that finds personal information in text and documents and replaces it with placeholders. Nothing leaves your machine.

![PocketAnon icon](assets/icon-256.png)

## What it does

- **Text**: paste or type, and a second after you stop the text is analysed. Detected entities are highlighted, listed for review, and the anonymized version appears beside the input, ready to copy.
- **Files**: open a TXT, DOCX or RTF. Pages are rendered with every finding boxed and labelled. Apply writes a new file in the same format next to the original, with headers, footers, footnotes, comments and tables covered. The original is never overwritten.
- **Detection** combines a zero-shot NER model ([GLiNER](https://github.com/urchade/GLiNER), multilingual PII variant, run on ONNX Runtime) with pattern rules and checksums for structured data such as IBANs, card numbers, phone numbers, emails and tax IDs. German and English are the defaults; more languages can be enabled.
- **Review before anything changes**: adjust the confidence threshold, reject or retype a finding, or select text and right-click to make it a permanent redaction rule.
- **Indexed placeholders**: the same person is always `[PERSON_1]`, the next one `[PERSON_2]`, so the result stays readable. There is no way to reverse it; the mapping is never written anywhere.
- **Detection panel**: languages, entity types with their colours, custom types and your own term rules, all switchable in place.
- **Privacy by construction**: settings are persisted, content never is. No recent files, no history, no mapping export.

## Status

This is a PoC built to explore the idea, not a finished product. Known limits:

- PDF is not supported.
- RTF output loses headers, footers, footnotes and comments.
- Inference runs on the CPU; a one-page document takes about two to three seconds on an Apple Silicon laptop.
- The app bundle is unsigned.

## Build and run

Requires Rust 1.95 and, for the icon script, `librsvg`. The first start downloads the model (about 580 MB) from Hugging Face into the user data directory.

```sh
cargo run -p pocketanon --release          # run from the terminal
./packaging/bundle.sh                      # or build target/bundle/PocketAnon.app
```

Headless check of the whole pipeline on a sample file:

```sh
cargo run -p pocketanon --example anonymize -- fixtures/letter_de.docx
```

## Repository layout

| Path | Purpose |
|---|---|
| `crates/core` | Domain model: entities, detectors, chunking, merging, replacement, review |
| `crates/detect-gliner` | GLiNER model detector, model registry and downloader |
| `crates/detect-rules` | Pattern and checksum rules, user term rules |
| `crates/documents` | Document adapter on the [Wordcraft](https://github.com/storytold/wordcraft) crates: load, edit, save, render |
| `apps/pocketanon` | The egui desktop app |
| `fixtures/` | Synthetic sample inputs; no real data |
| `docs/adr/` | Decisions and their reasons |

Vocabulary used across code and docs is defined in [`CONTEXT.md`](CONTEXT.md).

## Credits

Built with [egui](https://github.com/emilk/egui), [gline-rs](https://github.com/fbilhaut/gline-rs), [ONNX Runtime](https://onnxruntime.ai) via [ort](https://github.com/pykeio/ort), and the Wordcraft document crates. The look follows the [ArtCraft](https://getartcraft.com) apps.

## Author and license

Author: see `authors` in `Cargo.toml` (also embedded in the binary and the app bundle).

MIT OR Apache-2.0.
