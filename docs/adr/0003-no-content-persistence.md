---
status: accepted
---
# The app persists settings only, never content

Between runs the app stores user settings (languages, type groups, custom types, threshold, model choice). It does not store input text, processed file paths, recent-files lists, summaries or the indexed-redaction mapping. A privacy tool that keeps a trail of what it anonymized undermines its own purpose, and a mapping file would turn irreversible redaction into keyed pseudonymization. Convenience features such as "recent files" are deliberately absent; do not add them without revisiting this decision.

## Amendment 2026-10-17: term rules

User-authored **Term Rules** (a literal term mapped to an entity type, e.g. "FALKE" → project code name) are persisted with the settings. They are configuration the user typed in deliberately to make future runs redact more, not a record of what was processed. The term itself may be sensitive (a code name), so the settings file deserves the same care as any config with secrets, but this is a different category from input text, file paths or the redaction mapping, which remain unpersisted.
