# PocketAnon

PocketAnon is a local desktop application that detects personal information in text and documents and replaces it, so the result can be shared without exposing the people it refers to.

## Language

**Anonymization**:
The overall operation the app performs on an input: detect **Entities**, then apply a **Replacement Strategy** to each.
_Avoid_: Scrubbing, sanitizing, cleaning

**Entity**:
A span of input text that has been detected and classified with an **Entity Type**, e.g. "Max Mustermann" classified as PERSON.
_Avoid_: Match, hit, token

**Entity Type**:
The category assigned to an **Entity**, e.g. PERSON, EMAIL, IBAN. Entity types are grouped into **Type Groups**; the active set is configurable and may include **Custom Types**.
_Avoid_: Label, class, category

**Type Group**:
A named cluster of **Entity Types** the user can enable or disable together. The built-in groups are Identity, Contact, Identifiers and Sensitive. Sensitive is disabled by default.

**Custom Type**:
An **Entity Type** the user defines as free text at runtime, e.g. "project code name". Treated exactly like a built-in type once added.
_Avoid_: Custom label, user label

**Replacement Strategy**:
The rule that decides what text replaces a detected **Entity**. The app uses exactly one, **Indexed Redaction**; plain redaction (`[PERSON]` without an index) was dropped on 2026-10-17 because it makes documents unreadable.
_Avoid_: Mode, masking (which is a different, character-level strategy not currently supported)

**Indexed Redaction**:
The only **Replacement Strategy**. Replaces each **Entity** with a placeholder naming its **Entity Type** plus an index that is stable within one input, e.g. `[PERSON_1]`, `[PERSON_2]`. Two occurrences of the same entity text receive the same index. Irreversible.
_Avoid_: Pseudonymization (which substitutes realistic fake values and is out of scope)

## Detection

**Detector**:
A source of **Candidate** entities. The app has two: the **Model Detector** (GLiNER, zero-shot over the active **Entity Types**) and the **Rule Detector** (patterns and checksums for structured types such as IBAN, email, card numbers).
_Avoid_: Recognizer, analyzer, engine

**Candidate**:
A span proposed by one **Detector** with an **Entity Type** and a **Confidence**. Becomes an **Entity** only after **Merging** and **Review**.

**Confidence**:
A score from 0 to 1 attached to a **Candidate**. Candidates below the user's **Threshold** are hidden from **Review**.

**Threshold**:
The minimum **Confidence** a **Candidate** needs to be shown. Adjustable live during **Review**.

**Term Rule**:
A user-defined rule: a literal term (matched case-insensitively, whole words) that is always an **Entity** of a given **Entity Type**, e.g. "FALKE" as *project code name*. Created from a right-click on selected text or in the **Detection Panel**; fed to detection through the **Rule Detector** family. Term rules are settings the user authored, not content.
_Avoid_: Custom rule, blacklist, dictionary entry

**Detection Panel**:
The always-visible panel listing **Languages**, **Type Groups** with their colours, **Custom Types** and **Term Rules**, where all of them are switched on and off. The same panel serves **Text Anonymization** and **File Anonymization**.
_Avoid_: Legend, sidebar, settings (which is the separate drawer for model and appearance)

**Merging**:
The step that resolves overlapping **Candidates** from different **Detectors** into one. A **Rule Detector** candidate wins the span boundaries; the higher **Confidence** wins the **Entity Type**.
_Avoid_: Deduplication, resolution

**Chunk**:
A piece of input small enough for the **Model Detector**. Inputs are split on paragraph boundaries, long paragraphs on sentence boundaries. Chunking is invisible to the user: **Indexed Redaction** numbers entities across the whole input, not per chunk.

**Review**:
The step between detection and replacement where the user sees all **Candidates** above the **Threshold**, can reject them, change their **Entity Type**, or add a span by hand. Accepted candidates become **Entities**.
_Avoid_: Verification, approval, preview (which is the rendered document view inside Review)

## Models and languages

**Model**:
A GLiNER checkpoint the **Model Detector** can load, described by a **Registry** entry: name, download location, expected files and covered **Languages**. Models are downloaded on first use into the app's data directory, or loaded from a user-supplied folder.
_Avoid_: Checkpoint, weights, engine

**Registry**:
The built-in list of known **Models**. Users pick from it or bypass it with a local folder.

**Language**:
A natural language the user expects in their input. Multiple may be selected. The selection filters the **Registry** to suitable **Models** and decides which **Rule Detector** patterns are active, e.g. German tax numbers.
_Avoid_: Locale

## Inputs

**Text Anonymization**:
**Anonymization** of text the user types or pastes directly. The result is shown as text.
_Avoid_: Quick mode, inline mode

**File Anonymization**:
**Anonymization** of an uploaded **Document**, producing a new **Document** in the same **Format** at that format's **Fidelity**.
_Avoid_: Batch mode, upload mode

**Document**:
A file the user submits for **File Anonymization**. Supported **Formats** are TXT, DOCX and RTF. PDF is not supported.

**Format**:
The file type of a **Document**. Each format carries a fixed **Fidelity**.

**Fidelity**:
What a **Format** promises about the output **Document** beyond the replaced **Entities**. TXT is exact. DOCX is formatting-preserving across all text parts, including headers, footers, footnotes and comments. RTF is structurally faithful: paragraphs, character formatting, lists and tables survive; headers, footers, footnotes and comments do not appear in the output at all.
_Avoid_: Quality, mode

## Outputs

**Result**:
The anonymized text (for **Text Anonymization**) or the anonymized **Document** (for **File Anonymization**). A result document is written beside the input under a new name; the input is never overwritten.

**Summary**:
Counts of replaced **Entities** per **Entity Type**, shown in the UI after a run. Never written to disk.
_Avoid_: Report, log, audit

**Mapping**:
The in-memory table from placeholder to original text that **Indexed Redaction** builds while running. Exists only for the duration of a run and is never exported; exporting it would make results reversible.
_Avoid_: Key, lookup table

## Example dialogue

> **Dev:** When the user pastes a letter, what do we hand back?
> **Expert:** First a review: both detectors produce candidates, merging collapses the overlaps, and everything above the threshold is highlighted. What the user accepts becomes an entity, and only then do we apply the replacement strategy.
> **Dev:** And if the same person is named three times?
> **Expert:** With indexed redaction all three become `[PERSON_1]`. With plain redaction they're all `[PERSON]`. Either way we never invent a fake name; that would be pseudonymization, and we don't do that.
