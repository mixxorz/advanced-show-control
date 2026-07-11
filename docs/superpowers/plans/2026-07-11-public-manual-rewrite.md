# Public Manual Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the full Advanced Show Control public manual as a practical, safety-aware guide for LV1 engineers.

**Architecture:** Preserve the existing Zensical navigation, Markdown paths, and screenshots. Rewrite the public pages in three groups: entry path, screen guides, and concise reference material. Validate behavior against the current frontend components and tests, then validate rendered documentation and relevant frontend tests.

**Tech Stack:** Zensical Markdown site, React/TypeScript frontend, Vitest, GitHub Releases.

## Global Constraints

- Work only in `/Users/mixxorz/Projects/lv1-scene-fade-utility/.worktrees/zensical-docs` on `feat/zensical-docs`.
- Preserve exact UI labels, `X-Fade` limits, defaults, SAFE behavior, and current stored-only settings behavior.
- Do not mention an Abort All control.
- Use the stable v2 release assets, not nightly assets.
- Keep current screenshots and navigation unless a content change requires no new visual structure.
- Apply the binding technical-manual tone guide to every public Markdown page.

---

### Task 1: Rewrite The Entry Path

**Files:**
- Modify: `site/docs/index.md`
- Modify: `site/docs/getting-started.md`

**Interfaces:**
- Consumes: stable v2 asset URLs from GitHub Release `v2`; existing Zensical Markdown button syntax.
- Produces: a download path, product introduction, and a complete first-success procedure.

- [ ] **Step 1: Record the release facts**

Run: `gh release view v2 --json url,name,tagName,publishedAt,isPrerelease,assets`

Expected: Stable `v2` with Windows x64 ZIP and universal macOS DMG asset URLs.

- [ ] **Step 2: Rewrite the home page**

Write an introduction that defines the product as a scene-fade and cue-list overlay for Waves eMotion LV1 and LV1 Classic. Explain who should use it, how it fits alongside normal LV1 scene work, and what remains under LV1 control. Include the existing representative full-app screenshot and links labelled Download, Quick Start, and User Guide.

- [ ] **Step 3: Rewrite the first-success tutorial**

Sequence download, installation, connection, new session creation and save, one stored scoped fade, and recall observation. Include Windows and macOS asset links, natural unsigned/notarization guidance, exact controls, and rehearsal warning.

- [ ] **Step 4: Build the documentation**

Run: `make docs-build`

Expected: Documentation build exits successfully with no broken-page errors.

- [ ] **Step 5: Commit the entry path**

Run: `git add site/docs/index.md site/docs/getting-started.md && git commit -m "docs: rewrite manual entry path"`

Expected: A focused documentation commit.

### Task 2: Rewrite Screen Guides

**Files:**
- Modify: `site/docs/application-shell.md`
- Modify: `site/docs/scenes.md`
- Modify: `site/docs/cue-lists.md`
- Modify: `site/docs/settings.md`
- Modify: `site/docs/logs.md`

**Interfaces:**
- Consumes: UI state and visible labels from `ui/src/components/` and their Vitest coverage.
- Produces: page flow of purpose, common workflow, interface overview, controls, and edge cases.

- [ ] **Step 1: Rewrite shell and scene guidance**

Define the operator's workflow before the control reference. Explain SAFE, connection state, sessions, scope, target storage, exact identity validation, fade duration, manual override, and disconnect behavior in direct user language.

- [ ] **Step 2: Rewrite cue-list guidance**

Define cue lists and their relationship to app scene configurations before explaining list construction, cue preparation, GO, auto-next, missing scenes, keyboard operation, and blocked states.

- [ ] **Step 3: Rewrite settings and logs guidance**

Describe the desired outcome of each setting, distinguish active behavior from stored-only preferences, define shortcut capture and conflicts, and explain how the operational Logs screen differs from diagnostic files.

- [ ] **Step 4: Run the relevant frontend tests**

Run: `npm --prefix ui run test -- --run ui/src/components/AppShell.test.tsx ui/src/components/ConnectionModal.test.tsx ui/src/components/SceneEditor.test.tsx ui/src/components/CueListsTab.test.tsx ui/src/components/CueListManageModal.test.tsx ui/src/components/SettingsTab.test.tsx ui/src/components/BottomStatusBar.test.tsx ui/src/keyboard.test.tsx`

Expected: All selected tests pass.

- [ ] **Step 5: Commit the screen guides**

Run: `git add site/docs/application-shell.md site/docs/scenes.md site/docs/cue-lists.md site/docs/settings.md site/docs/logs.md && git commit -m "docs: rewrite screen guides"`

Expected: A focused documentation commit.

### Task 3: Rewrite Reference And Troubleshooting

**Files:**
- Modify: `site/docs/troubleshooting.md`
- Modify: `site/docs/reference/terminology.md`
- Modify: `site/docs/reference/keyboard-shortcuts.md`

**Interfaces:**
- Consumes: terminology and behavior established by the rewritten guides.
- Produces: concise, actionable reference pages that use the same terms and safety model.

- [ ] **Step 1: Rewrite troubleshooting**

For each problem, state the visible condition, likely operational consequence, and the corrective action. Link to the relevant full procedure without restating internal implementation details.

- [ ] **Step 2: Rewrite reference pages**

Define each term before using its operating advice. Keep shortcut constraints, fixed commands, and SAFE restrictions exact and concise.

- [ ] **Step 3: Complete editorial review and verification**

Review every Markdown file under `site/docs/` against the tone guide. Run `make docs-build` and `npm --prefix ui run test`.

Expected: Build and frontend test suite pass; every public page uses direct, operator-focused language and accurate control names.

- [ ] **Step 4: Write the delivery report and commit**

Write `sdd/manual-rewrite-report.md` with changed pages, editorial choices, factual sources, v2 download URLs, exact verification results, commit hashes, and remaining concerns. Commit the reference pages and report with `git add site/docs/troubleshooting.md site/docs/reference/terminology.md site/docs/reference/keyboard-shortcuts.md sdd/manual-rewrite-report.md && git commit -m "docs: complete manual rewrite"`.

Expected: A final documentation commit and a detailed delivery record.
