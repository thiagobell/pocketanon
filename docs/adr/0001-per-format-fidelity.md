---
status: accepted
---
# Per-format fidelity for file anonymization

File anonymization promises a different fidelity per input format rather than a uniform "same file, only entities changed" guarantee. TXT round-trips exactly. DOCX preserves formatting by editing paragraph text run-aware across all text parts (body, headers, footers, footnotes, comments). RTF is read and written through the Wordcraft flow model (see ADR-0002), which keeps paragraphs, character formatting, lists, alignment and tables but drops anything the flow model does not represent, so RTF output is structurally faithful rather than byte-faithful. PDF is excluded entirely, because drawing boxes over text is not redaction (the text stays extractable) and true content-stream redaction is a project of its own.

## Consequences

- The UI must state the fidelity level per format so users are not surprised by RTF details that did not survive the flow model.
- Adding PDF later means real content-stream rewriting, not an overlay.
