# ASD-STE100 Issue 9 Manual Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite all public manual prose for strict ASD-STE100 Simplified Technical English Issue 9 conformance without changing product facts or manual structure.

**Architecture:** Keep the ten existing Zensical Markdown pages and their current responsibilities. First establish the controlled product glossary, then rewrite related page groups against that glossary, and finish with a sentence-by-sentence dictionary and structure audit.

**Tech Stack:** Zensical Markdown, ASD-STE100 Simplified Technical English Issue 9, Ruby for local candidate word-count checks, and the strict Zensical documentation build.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-19-asd-ste100-manual-rewrite-design.md`.
- Use ASD-STE100 Issue 9, dated 2025-01-15, as the authoritative language source.
- Read the local authoritative copy at `/Users/mixxorz/Downloads/ASD-STE100_ISSUE9.pdf`.
- Preserve the ten current manual files, routes, screenshots, tables, procedures, cross-page links, and overall topic order.
- Preserve exact product names, UI labels, filenames, file paths, shortcuts, code values, URLs, and proper nouns.
- Treat protected text as fixed text. Apply STE rules to the sentence around it.
- Do not change application code, behavior, screenshots, media, navigation, product names, trademarks, license names, or download URLs.
- Use no more than 20 words in a procedural sentence and no more than 25 words in a descriptive sentence.
- Use no more than six sentences in one paragraph.
- Use one command in each procedural sentence unless actions occur at the same time.
- Use imperative verbs in procedures and active voice in descriptions unless the agent is unknown.
- Use approved words only with their approved meanings and parts of speech.
- Use the controlled project glossary for valid product-specific technical nouns and technical verbs.
- Do not use contractions, semicolons, phrasal verbs, jargon, or unnecessary synonyms.
- Use American English spelling and consistent terminology.
- Documentation-only verification applies. No Rust unit, actor, or smoke test is applicable because this work does not change application behavior.
- No frontend unit, Storybook, or visual test is applicable because this work does not change the UI or screenshots.

## Audit Method

For every changed sentence:

1. Classify it as procedural or descriptive.
2. Count protected text, quoted text, abbreviations, numbers with units, and hyphenated words as specified by Issue 9 rules 8.4 through 8.7.
3. Confirm the applicable 20-word or 25-word limit.
4. Check each normal prose word in the Issue 9 dictionary.
5. Check each product-specific word against `site/docs/reference/terminology.md` and the Issue 9 technical-term categories.
6. Confirm the approved part of speech and meaning.
7. Confirm that the sentence has one main topic.

Use this command only to find candidate sentence-length violations. Manually correct its counts for Markdown and the Issue 9 counting rules:

```bash
ruby -e 'ARGV.each { |path| File.readlines(path).each_with_index { |line, i| next if line.lstrip.start_with?("```", "|", "!["); line.scan(/[^.!?]+[.!?]+/).each { |sentence| count = sentence.scan(/[[:alnum:]]+(?:[-'\''+][[:alnum:]]+)*/).length; puts "#{path}:#{i + 1}:#{count}:#{sentence.strip}" if count > 20 } } }' site/docs/*.md site/docs/reference/*.md
```

Expected final output: only descriptive sentences with manually verified counts of 21 through 25 words, or no output.

Use these exact cross-manual checks after each task:

```bash
rg -n ";" site/docs --glob "*.md"
rg -ni "\b(can't|cannot've|don't|doesn't|isn't|aren't|wasn't|weren't|won't|wouldn't|shouldn't|couldn't|it's|you're|we're|they're)\b" site/docs --glob "*.md"
```

Expected final output: no matches from either command. `CANNOT` is an approved word, but contractions with `not` are not permitted.

---

### Task 1: Establish The Controlled Product Glossary

**Files:**
- Modify: `site/docs/reference/terminology.md`
- Modify: `site/docs/reference/keyboard-shortcuts.md`

**Interfaces:**
- Consumes: the controlled terminology in the approved design and exact shortcut behavior in the current manual.
- Produces: the authoritative public definitions and term forms used by Tasks 2 through 5.

- [ ] **Step 1: Record the current language failures**

Review both reference pages sentence by sentence. Record each unapproved word, incorrect part of speech, sentence over 25 words, multi-topic sentence, contraction, phrasal verb, and inconsistent term in working notes outside the repository.

Expected: the current pages do not pass the complete Issue 9 dictionary and sentence audit.

- [ ] **Step 2: Rewrite the terminology reference**

Keep the existing table and define these exact terms once: `LV1 scene`, `scene fade setting`, `linked setting`, `unlinked setting`, `scope`, `target`, `cut`, `fade`, `cue list`, `cued entry`, `GO`, `SAFE`, and `session`.

Use one meaning for each term. State that `scope` identifies the channels and controls that can move. State that a `fade` starts at current control positions and ends at stored targets. Preserve `.ascs`, **FADER**, **PAN**, **GO**, **SAFE**, **Store**, and **Recall** as protected text.

- [ ] **Step 3: Rewrite the keyboard shortcut reference**

Keep the fixed shortcut table and the **GO** and **CUE** table. Preserve `CmdOrCtrl`, `CmdOrCtrl+N`, `CmdOrCtrl+O`, `CmdOrCtrl+S`, `CmdOrCtrl+Shift+S`, `Space`, `C`, `Escape`, and `Tab` exactly.

Give one command in each instruction. State these limits in separate descriptive sentences: text entry blocks shortcuts, dialogs block shortcuts, held keys do not repeat actions, and shortcuts do not bypass safety checks.

- [ ] **Step 4: Audit the two reference pages**

Apply the seven-point audit method to every heading, paragraph, table cell, and link label in both files. Confirm that all normal prose words are approved and all project terms are valid technical terms.

Run the two cross-manual checks. Existing failures elsewhere are permitted at this task boundary, but the two changed files must have no match.

- [ ] **Step 5: Build the documentation site**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue.

- [ ] **Step 6: Commit the controlled references**

```bash
git add site/docs/reference/terminology.md site/docs/reference/keyboard-shortcuts.md
git commit -m "docs: define STE manual terminology"
```

### Task 2: Rewrite The Home And First-Use Procedure

**Files:**
- Modify: `site/docs/index.md`
- Modify: `site/docs/getting-started.md`

**Interfaces:**
- Consumes: the exact glossary forms from Task 1 and the existing download and first-fade facts.
- Produces: an STE product introduction and a complete first-use procedure.

- [ ] **Step 1: Record the current language failures**

Review both pages with the seven-point audit method. Pay special attention to promotional wording, long descriptive sentences, combined commands, passive constructions, and the warning about empty scope.

Expected: the current pages do not pass the complete Issue 9 dictionary and sentence audit.

- [ ] **Step 2: Rewrite the home page**

Keep the current product title, screenshot, download buttons, guide links, disclaimer facts, trademark statements, GPL link, and warranty statement.

Describe only these product facts:

- Advanced Show Control adds timed fader and pan moves to LV1 scenes.
- The user selects scope, stores target values, and sets fade time.
- A recall moves scoped controls from current positions to stored targets.
- Cue lists put scene fade settings in show order without changing LV1 scene order.
- Version 2 downloads are not signed, and the macOS download is not notarized.
- The user must rehearse each fade and cue-list sequence on the show system.

- [ ] **Step 3: Rewrite the installation and connection procedures**

Keep the five numbered sections and the connection screenshot. Put one command in each numbered step. Preserve both stable v2 download URLs and the exact labels **Available**, **Unavailable**, **Connected**, and **Connect to LV1**.

State that the computer and LV1 system must use the same network. State that Advanced Show Control searches for LV1 systems when the application starts.

- [ ] **Step 4: Rewrite the session and first-fade procedures**

Preserve `.ascs`, **File > New Session**, **File > Save Session**, **Scenes**, **Scope**, **FADER**, **PAN**, **Store**, **X-Fade**, and **Recall**.

Keep these facts explicit:

- A new scene fade setting has an empty scope.
- **FADER** and **PAN** are off in a new setting.
- Empty scope at **Store** adds all current channels.
- **Store** keeps the current **FADER** and **PAN** selections.
- `0` makes a cut.
- Timed values are `0.1` through `120` seconds.
- Recall requires **Connected**, exact scene identity, and **SAFE** off.
- A fade starts at current positions.
- Manual fader movement gives control of that fader to the user.

Format the scope risk as a warning with a command, the condition, and the possible result.

- [ ] **Step 5: Audit both pages**

Apply the seven-point audit method to all reader-facing text, including image alternative text, button labels, admonition titles, and guide-link descriptions.

Run the candidate sentence-length command and both cross-manual checks. Manually verify every candidate in these two files.

- [ ] **Step 6: Build and commit**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue.

```bash
git add site/docs/index.md site/docs/getting-started.md
git commit -m "docs: rewrite STE getting started guide"
```

### Task 3: Rewrite The Application Shell And Scene Workflows

**Files:**
- Modify: `site/docs/application-shell.md`
- Modify: `site/docs/scenes.md`

**Interfaces:**
- Consumes: glossary terms from Task 1 and first-use terminology from Task 2.
- Produces: STE descriptions of persistent controls and all scene fade setting tasks.

- [ ] **Step 1: Record the current language failures**

Audit both pages before editing. Identify long status descriptions, combined instructions, passive constructions, ambiguous pronouns, and inconsistent forms of scene-related terms.

Expected: the current pages do not pass the complete Issue 9 dictionary and sentence audit.

- [ ] **Step 2: Rewrite the application shell page**

Keep the screenshots, top-bar section, **SAFE** section, bottom status table, and session shortcut table.

Preserve these facts:

- The tabs are **Scenes**, **Cue Lists**, **Logs**, **Settings**, and **Events**.
- Event automation is not available in v2.
- Connection states are **Connected**, **Connecting**, and **Offline**.
- The full-screen **Reconnecting...** message blocks scene fade starts.
- **SAFE** blocks **Recall** and **GO** but does not stop an active fade.
- **Mode** shows **Safe** while **SAFE** is on.
- `---` identifies an unavailable scene field.
- **Ready** does not guarantee that **GO** is available.
- Version 2 does not warn about unsaved changes before session replacement or closure.

- [ ] **Step 3: Rewrite scene creation, store, scope, and fade-time sections**

Keep all current screenshots and table rows. Use separate procedural steps for scene selection, LV1 value changes, channel selection, control selection, store, fade time, and save.

Preserve empty-scope behavior, target replacement behavior, all scope controls, immediate cut behavior, accepted timed values, accepted trailing `s`, and invalid-value restoration.

- [ ] **Step 4: Rewrite recall, link, delete, copy, and paste sections**

Preserve exact scene number-and-name validation, LV1-first recall order, current-position fade starts, manual override behavior, disconnect behavior, and retained unlinked data.

Keep these copy and paste facts:

- **Copy** keeps fade time, control selections, target data, and channel scope.
- Copy can use a linked or unlinked source.
- **Paste** requires a linked destination.
- **Paste** keeps the destination scene identity.
- An identical paste does not mark the session as changed.
- Session replacement clears copied settings.

- [ ] **Step 5: Audit both pages**

Apply the seven-point audit method to every reader-facing item. Confirm one consistent distinction between an LV1 scene and a scene fade setting.

Run the candidate sentence-length command and both cross-manual checks. Manually verify every candidate in these two files.

- [ ] **Step 6: Build and commit**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue.

```bash
git add site/docs/application-shell.md site/docs/scenes.md
git commit -m "docs: rewrite STE scene workflows"
```

### Task 4: Rewrite Cue-List And Settings Workflows

**Files:**
- Modify: `site/docs/cue-lists.md`
- Modify: `site/docs/settings.md`

**Interfaces:**
- Consumes: controlled scene, cue, session, and shortcut terms from Tasks 1 through 3.
- Produces: STE cue-list procedures and settings descriptions.

- [ ] **Step 1: Record the current language failures**

Audit both pages before editing. Identify commands joined by conjunctions, long setting descriptions, complex verb constructions, phrasal verbs, and unclear references.

Expected: the current pages do not pass the complete Issue 9 dictionary and sentence audit.

- [ ] **Step 2: Rewrite cue-list creation and management**

Keep the two screenshots and all current headings. Put creation, activation, addition, and ordering in separate commands.

Preserve these facts:

- Cue lists do not change LV1 scene order.
- One scene fade setting can occur more than one time.
- Removing an entry does not remove its scene fade setting or LV1 scene.
- Selecting an entry does not cue it.
- **Cue** or a double-click prepares an entry.

- [ ] **Step 3: Rewrite GO behavior and entry states**

Preserve automatic advance after successful **GO**, `---` after the last entry, all unavailable conditions, unlinked recall refusal, and missing-scene behavior.

Keep **Selected**, **Cued**, **Current**, and **Missing scene** as exact state labels. Preserve `C` and `Space` defaults and all shortcut limits.

- [ ] **Step 4: Rewrite active and inactive settings**

Keep the settings screenshot and the active/inactive split. Preserve all same-scene settings facts, including the `500 ms` default, `0 ms` through `5000 ms` range, and `100 ms` steps.

Preserve the effect of enabled and disabled same-scene finishing. Preserve duplicate-notification behavior and all unchanged safety checks.

Keep **Extensive diagnostics** behavior, shortcut capture behavior, and the four inactive v2 settings. Keep `1` through `10` for inactive **Fader override sensitivity**.

- [ ] **Step 5: Audit both pages**

Apply the seven-point audit method to every reader-facing item. Check that each setting description has one topic and that each shortcut instruction has one command.

Run the candidate sentence-length command and both cross-manual checks. Manually verify every candidate in these two files.

- [ ] **Step 6: Build and commit**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue.

```bash
git add site/docs/cue-lists.md site/docs/settings.md
git commit -m "docs: rewrite STE cue and settings guides"
```

### Task 5: Rewrite Logs And Troubleshooting

**Files:**
- Modify: `site/docs/logs.md`
- Modify: `site/docs/troubleshooting.md`

**Interfaces:**
- Consumes: all controlled terminology and factual operating limits from Tasks 1 through 4.
- Produces: STE diagnostic descriptions and condition-action troubleshooting procedures.

- [ ] **Step 1: Record the current language failures**

Audit both pages before editing. Identify unapproved diagnostic vocabulary, combined corrective commands, unclear conditions, passive constructions, and paragraphs with more than one fault topic.

Expected: the current pages do not pass the complete Issue 9 dictionary and sentence audit.

- [ ] **Step 2: Rewrite the logs page**

Keep the screenshot and diagnostic path. Preserve the meanings of `INFO`, `WARNING`, `ERROR`, and `DEBUG` as protected severity labels.

State that **Logs** excludes `DEBUG`. State that **Extensive diagnostics** adds `DEBUG` to diagnostic files and can cause files to grow quickly.

Put each requested problem-report item in a vertical list. Keep the restriction on sharing complete session and diagnostic files.

- [ ] **Step 3: Rewrite each troubleshooting section**

Use this sequence for each fault: condition, cause, corrective command, and verification. Give one command in each sentence.

Preserve all current fault facts for discovery, reconnect, unlinked settings, recall, **GO**, missing scenes, session files, visible logs, and inactive settings. Preserve every existing cross-page link.

- [ ] **Step 4: Audit both pages**

Apply the seven-point audit method to every reader-facing item. Confirm that every corrective instruction uses the imperative form and has no more than 20 words.

Run the candidate sentence-length command and both cross-manual checks. Manually verify every candidate in these two files.

- [ ] **Step 5: Build and commit**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue.

```bash
git add site/docs/logs.md site/docs/troubleshooting.md
git commit -m "docs: rewrite STE troubleshooting guide"
```

### Task 6: Complete The Cross-Manual Conformance Review

**Files:**
- Modify if the audit finds a defect: `site/docs/index.md`
- Modify if the audit finds a defect: `site/docs/getting-started.md`
- Modify if the audit finds a defect: `site/docs/application-shell.md`
- Modify if the audit finds a defect: `site/docs/scenes.md`
- Modify if the audit finds a defect: `site/docs/cue-lists.md`
- Modify if the audit finds a defect: `site/docs/settings.md`
- Modify if the audit finds a defect: `site/docs/logs.md`
- Modify if the audit finds a defect: `site/docs/troubleshooting.md`
- Modify if the audit finds a defect: `site/docs/reference/terminology.md`
- Modify if the audit finds a defect: `site/docs/reference/keyboard-shortcuts.md`

**Interfaces:**
- Consumes: all ten rewritten pages and the approved design.
- Produces: one factually consistent manual that passes the complete Issue 9 review and strict site build.

- [ ] **Step 1: Audit every sentence and term**

Apply the seven-point audit method to every heading, paragraph, procedure, admonition, table cell, image alternative, and link label in all ten pages.

For each normal prose word, verify its dictionary entry, approved meaning, and approved part of speech. For each project term, verify its glossary form and technical-term category. Rewrite each violation instead of substituting one word when the sentence construction must change.

- [ ] **Step 2: Audit document structures**

Confirm that each procedure sentence has no more than 20 words. Confirm that each descriptive sentence has no more than 25 words. Confirm that each paragraph has one topic and no more than six sentences.

Confirm that multi-word nouns have no more than three words unless the glossary defines the necessary full term. Confirm that vertical lists have parallel grammatical structures.

- [ ] **Step 3: Audit prohibited constructions and terminology**

Run the candidate sentence-length command and both cross-manual checks from the Audit Method section.

Run:

```bash
rg -ni "app scene configuration|configured application scene|fade overlay|dirty session|source of truth|Abort All" site/docs --glob "*.md"
```

Expected: no output.

Search for every glossary term and confirm that competing synonyms do not identify the same concept.

- [ ] **Step 4: Verify protected text and facts**

Run `git diff f6f52b3 -- site/docs` to compare the rewritten pages with the pre-rewrite versions. Also compare the result with the facts in `docs/superpowers/specs/2026-07-11-public-manual-rewrite-design.md`.

Confirm all UI labels, numbers, ranges, defaults, file extensions, paths, shortcuts, product names, trademarks, license names, and download URLs. Confirm that no page claims unavailable v2 behavior.

- [ ] **Step 5: Run final documentation verification**

Run: `make docs-build`

Expected: the command exits with status 0 and Zensical reports no strict-validation issue or broken link.

Run: `git diff --check`

Expected: no output.

- [ ] **Step 6: Review the rendered site**

Open the generated pages under `site/output/`. Check all ten pages at desktop and narrow viewport widths. Confirm readable tables, visible admonitions, correct images, working internal links, and unchanged navigation.

- [ ] **Step 7: Commit audit corrections**

If the audit changed files, stage only the corrected manual files and commit:

```bash
git add site/docs
git commit -m "docs: complete STE manual audit"
```

If the audit changed no files, do not create an empty commit.
