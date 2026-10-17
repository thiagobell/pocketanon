---
status: accepted
---
# Per-format fidelity for file anonymization

File anonymization promises a different fidelity per input format rather than a uniform "same file, only entities changed" guarantee. TXT round-trips exactly. DOCX preserves formatting by editing paragraph text run-aware across all text parts (body, headers, footers, footnotes, comments). RTF is read and written through the Wordcraft flow model (see ADR-0002), which keeps paragraphs, character formatting, lists, alignment and tables but drops anything the flow model does not represent, so RTF output is structurally faithful rather than byte-faithful. In practice Wordcraft's RTF import drops footnotes and comments and its export writes no headers or footers, so those parts are absent from RTF output (nothing leaks, but content is lost); the UI says so. PDF is excluded entirely, because drawing boxes over text is not redaction (the text stays extractable) and true content-stream redaction is a project of its own.

## Consequences

- TXT bypasses Wordcraft for the file contents (Wordcraft's text export normalises line endings and encoding); Wordcraft only renders the preview.
- On save, DOCX and RTF clear the author and last-modified-by metadata and DOCX sets Word's remove-personal-info flag, because an anonymized document with the author's name in its properties would defeat the purpose.

- The UI must state the fidelity level per format so users are not surprised by RTF details that did not survive the flow model.
- Adding PDF later means real content-stream rewriting, not an overlay.
