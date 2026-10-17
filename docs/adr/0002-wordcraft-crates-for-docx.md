---
status: accepted
---
# Wordcraft crates for DOCX handling and document preview

DOCX reading, editing, writing and page rendering are built on the Wordcraft workspace crates (`wordcraft-doc`, `wordcraft-docx`, `wordcraft-formats`, `wordcraft-layout`, `wordcraft-render`, MIT OR Apache-2.0, pulled as pinned git dependencies) instead of our own zip-plus-XML editing. Their document model already separates body, headers, footers, notes and comments into stories of paragraphs and runs, which is exactly the surface anonymization must cover, and their layout and rasteriser give the review step a rendered page preview with no extra work.

## Considered options

- Own run-aware XML editing with `docx-rs` or raw `quick-xml`: smaller dependency, but no preview and a long tail of OOXML edge cases.
- Rendering preview separately (e.g. convert to PDF and rasterise): double parsing, two sources of truth for positions.

## Consequences

- Wordcraft is alpha and not on crates.io; we pin a commit and expect API churn on upgrade. All Wordcraft usage lives behind one adapter module so churn is contained.
- Toolchain floor is Rust 1.95 / edition 2024 and build times are noticeably longer than a plain egui app.
- `wordcraft-formats` also handles RTF and TXT through a flow model, so all three supported formats share one in-memory document model and one preview path.
