# ASD-STE100 Issue 9 Manual Rewrite Design

**Date:** 2026-07-19

**Status:** Approved

## Objective

Rewrite the public manual in `site/docs/` for strict conformance with ASD-STE100 Simplified Technical English, Issue 9. Preserve the current manual structure and all verified product behavior.

The authoritative language source is ASD-STE100 Issue 9, dated 2025-01-15. The rewrite must apply both its writing rules and its controlled dictionary.

## Approved Approach

Preserve the current information architecture and fully rewrite each page. Keep the ten manual files, routes, screenshots, tables, procedures, cross-page links, and overall topic order.

This approach minimizes structural and factual risk. It also permits a complete language rewrite without unrelated navigation or application changes.

## Scope

Apply the STE rules to all reader-facing prose:

- Headings and link text
- Paragraphs and procedures
- Admonitions and safety instructions
- Table headings and table cells
- Image alternative text
- Troubleshooting instructions
- Terminology definitions
- Download, warranty, and trademark statements

Preserve exact product names, UI labels, filenames, file paths, shortcuts, code values, URLs, and proper nouns. Treat these items as protected text. The sentence around each protected item must conform to STE.

Do not change product behavior or add unsupported instructions. Use the current manual, verified design documents, and visible UI labels as factual sources.

## Controlled Terminology

Use one controlled product glossary for all pages. Product-specific terms are technical nouns or technical verbs when the Issue 9 categories permit them.

The glossary includes terms such as:

- LV1 scene
- Scene fade setting
- Linked setting
- Unlinked setting
- Scope
- Target
- Cut
- Fade
- Cue list
- Cued entry
- Session
- Fader
- Pan control

Each term must have one meaning and one form throughout the manual. The terminology reference page is the public glossary. It must not reproduce the ASD-STE100 dictionary.

Exact UI text remains protected. For example, **Same scene recall finishing** remains unchanged even when normal prose would require different words.

## Language Rules

The rewritten manual must use these Issue 9 constraints:

- Use approved dictionary words only with their approved meanings and parts of speech.
- Use valid technical nouns and technical verbs for product-specific concepts and processes.
- Use no more than 20 words in a procedural sentence.
- Use no more than 25 words in a descriptive sentence.
- Put one instruction in each procedural sentence unless actions occur at the same time.
- Use the imperative form for instructions.
- Use active voice unless the agent is unknown in descriptive text.
- Give one main topic in each sentence.
- Give one topic in each paragraph.
- Use no more than six sentences in a paragraph.
- Use no more than three words in a multi-word noun unless a necessary technical term requires its full form.
- Do not use contractions, semicolons, phrasal verbs, jargon, or unnecessary synonyms.
- Use American English spelling.
- Use consistent terminology and wording.

Warnings and other safety instructions must identify the risk level, start with a clear command or condition, and state the possible result.

## Page Strategy

### Home

`site/docs/index.md` gives a short product description, download instructions, a rehearsal warning, and guide links.

### Getting Started

`site/docs/getting-started.md` gives direct procedures from installation through the first scene recall.

### Application Shell

`site/docs/application-shell.md` describes connection, **SAFE**, status, and session controls.

### Scenes

`site/docs/scenes.md` separates store, scope, recall, link, copy, and paste tasks.

### Cue Lists

`site/docs/cue-lists.md` separates list management, cue preparation, and **GO** operation.

### Settings

`site/docs/settings.md` clearly separates active and inactive settings.

### Logs

`site/docs/logs.md` explains visible logs, diagnostic files, and the information necessary for a problem report.

### Troubleshooting

`site/docs/troubleshooting.md` uses a consistent sequence: condition, cause, corrective action, and verification.

### Reference

`site/docs/reference/terminology.md` contains the controlled product glossary. `site/docs/reference/keyboard-shortcuts.md` contains shortcut facts and related operating limits.

## Verification

Verification must include:

- A sentence-count audit for the 20-word and 25-word limits
- A paragraph audit for the six-sentence limit
- Checks for semicolons, contractions, inconsistent terms, and prohibited constructions
- A vocabulary audit against the Issue 9 dictionary and the project technical-term list
- A factual comparison against the current manual and verified design documents
- A link and site-build check
- A final manual review for meaning, part of speech, sentence type, and technical-term validity

Automated checks can identify possible violations, but they cannot prove full conformance. The final review must examine every reader-facing sentence.

## Exclusions

The rewrite does not change:

- Application code or behavior
- Manual routes or navigation structure
- Screenshots or other media
- UI labels
- Product names, trademarks, or license names
- Download URLs
