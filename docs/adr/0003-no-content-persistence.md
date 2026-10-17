---
status: accepted
---
# The app persists settings only, never content

Between runs the app stores user settings (languages, type groups, custom types, threshold, model choice). It does not store input text, processed file paths, recent-files lists, summaries or the indexed-redaction mapping. A privacy tool that keeps a trail of what it anonymized undermines its own purpose, and a mapping file would turn irreversible redaction into keyed pseudonymization. Convenience features such as "recent files" are deliberately absent; do not add them without revisiting this decision.
