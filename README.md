# PocketAnon

**Proof of concept.** A local desktop app, that uses the GLiNER model to anonymize text and docx files. Your inputs do not leave the machine. 

![PocketAnon icon](assets/icon-256.png)

## What it does

- **Text & Docx**: type text or open a docx file, and have the model find the configured entities for anonymization.
- **Detection** combines a zero-shot NER model ([GLiNER](https://github.com/urchade/GLiNER), multilingual PII variant, run on ONNX Runtime) with pattern rules and checksums for structured data such as IBANs, card numbers, phone numbers, emails and tax IDs. German and English are the defaults; more languages can be enabled.
- **Custom Labels** configure anonymization with your own custom labels
- **Review before anything changes**: reject or retype a finding, add rule-based redactions.
- **Privacy by construction**: settings are persisted, content never is. No recent files, no history, no mapping export.

## Disclaimer

This is a PoC built to explore the idea, not a finished product. Known limits:

- PDF is not supported.
- RTF output loses headers, footers, footnotes and comments.
- The macOS app is only ad-hoc signed, not notarized. See [Install a CI build on macOS](#install-a-ci-build-on-macos).
- Inference runs over MLX on Macs but for small content, it is probably slower than CPU. 

## Install a CI build on macOS

The app is not notarized, so macOS blocks a copy downloaded with a browser. The warning says it could not verify the developer and only offers "Done". Remove the browser's quarantine flag from the disk image before you open it:

```sh
xattr -d com.apple.quarantine ~/Downloads/pocketanon-macos-aarch64.dmg
open ~/Downloads/pocketanon-macos-aarch64.dmg
```

Then drag PocketAnon onto Applications and open it as usual. If a blocked copy is already in Applications, delete it first. Running `xattr` on the installed app usually fails with "Operation not permitted", because macOS protects apps from being modified.

Without a terminal: open the app once, click "Done", then go to System Settings > Privacy & Security and click "Open Anyway" next to the PocketAnon message. The button only shows for about an hour after the blocked launch.

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

Built with [egui](https://github.com/emilk/egui), [gline-rs](https://github.com/fbilhaut/gline-rs), [ONNX Runtime](https://onnxruntime.ai) via [ort](https://github.com/pykeio/ort), and the Wordcraft document crates.

## Author and license

Author: see `authors` in `Cargo.toml` (also embedded in the binary and the app bundle).
