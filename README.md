# PocketAnon

A local, pure-Rust desktop app that anonymizes text and documents. Detects personal information with a GLiNER model (ONNX Runtime) plus pattern rules, lets you review the findings, and replaces them with placeholders such as `[PERSON_1]`.

- **Text Anonymization**: paste text, review detections, copy the result.
- **File Anonymization**: TXT, DOCX and RTF in, same format out, with formatting preserved (see `docs/adr/0001-per-format-fidelity.md`).
- Runs entirely on your machine. Settings persist; content never does.

## Build

Requires Rust 1.95 (see `rust-toolchain.toml`).

```sh
cargo run -p pocketanon --release
```

On first start, pick a model in Settings and download it (about 580 MB for the default multilingual PII model in fp16; the int8 variant is much smaller but loses most of its recall). Downloads go to the platform data directory and nowhere else.

## Layout

| Path | Purpose |
|---|---|
| `crates/core` | Domain: entities, detectors, chunking, merging, replacement, review, document contract |
| `crates/detect-gliner` | Model Detector on `gline-rs` / ONNX Runtime, model registry and downloader |
| `crates/detect-rules` | Rule Detector: regex patterns and checksums |
| `crates/documents` | Wordcraft-based adapter: load, segment, edit, save, render |
| `apps/pocketanon` | The egui desktop app |
| `fixtures/` | Synthetic sample inputs (no real data); `make_fixture` regenerates the DOCX and RTF |

Headless end-to-end run over a file, useful for checking a change without the UI:

```sh
cargo run -p pocketanon --example anonymize -- fixtures/letter_de.docx
```

Vocabulary lives in `CONTEXT.md`; decisions in `docs/adr/`; the PoC plan in `docs/poc-plan.md`.
