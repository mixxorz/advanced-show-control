# Manual Factual And Tone Revision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Correct documented UI behavior and revise every public manual page to use direct, consistent operator language.

**Architecture:** Treat `scene fade setting` as the single user-facing term for Advanced Show Control data linked to an LV1 scene. Verify UI-enablement behavior from components and backend rejection behavior from scene commands, then apply the terminology and tone revision consistently across all public Markdown pages.

**Tech Stack:** Zensical Markdown, React/TypeScript, Rust scene commands, Vitest.

## Global Constraints

- Work only in `/Users/mixxorz/Projects/lv1-scene-fade-utility/.worktrees/zensical-docs` on `feat/zensical-docs`.
- Define `scene fade setting` once and use it consistently; refer to console objects only as `LV1 scenes`.
- Do not use `Abort All`, `LV1 remains the source of truth`, internal architecture vocabulary, release-artifact phrasing, or browser/DOM vocabulary in public pages.
- SAFE blocks app recalls but does not disable LV1 controls.
- Preserve direct stable v2 Windows and macOS download links, exact control labels, and the `X-Fade` range of `0` or `0.1` through `120` seconds.

---

### Task 1: Correct Operator Behavior And Terminology

**Files:**
- Modify: `site/docs/index.md`
- Modify: `site/docs/getting-started.md`
- Modify: `site/docs/application-shell.md`
- Modify: `site/docs/scenes.md`
- Modify: `site/docs/cue-lists.md`
- Modify: `site/docs/settings.md`
- Modify: `site/docs/logs.md`
- Modify: `site/docs/troubleshooting.md`
- Modify: `site/docs/reference/terminology.md`
- Modify: `site/docs/reference/keyboard-shortcuts.md`

**Interfaces:**
- Consumes: `BottomStatusBar.resolveCuedScene()` and `canGo`, `SelectedSceneActions`, and `validate_recall_scene_request()`.
- Produces: public documentation that separates enabled controls from later recall blocks and uses one operator term.

- [ ] **Step 1: Verify factual source behavior**

Read `ui/src/components/BottomStatusBar.tsx`, `ui/src/components/SelectedSceneActions.tsx`, and `src-tauri/src/scenes/commands.rs`. Confirm that GO requires a cued entry resolving to any scene fade setting, **Copy** is enabled with no handler, and recall rejects an unlinked setting.

- [ ] **Step 2: Rewrite all public Markdown pages**

Define `scene fade setting` in the product introduction and terminology reference. Replace competing terms and implementation phrases. State GO's visible enabled, busy, rejected-unlinked, and corrective behavior precisely. Move inactive v2 settings under a direct inactive heading and rewrite shortcut instructions in operator language.

- [ ] **Step 3: Search for prohibited and inconsistent terms**

Run: `rg -n "app scene configuration|fade overlay|configured application scene|current live channel data|eligible targets|blocked, skipped|disabled recall|request pending|resolves|repeated activation|keydown events|non-modifier|text input|text area|select control|editable content|stored-only|current fade engine|dirty session|placeholder content|no current workflow|current build|source of truth|release artifacts provide|active fade activity|valid application recall|Abort All" site/docs`

Expected: No matches.

- [ ] **Step 4: Verify documentation and behavior sources**

Run: `make docs-build && npm --prefix ui run test`

Expected: Strict documentation build reports `No issues found`; all frontend unit tests pass.

- [ ] **Step 5: Update report and commit**

Append factual corrections, terminology decision, page review, and exact verification output to `/Users/mixxorz/Projects/lv1-scene-fade-utility/.git/worktrees/zensical-docs/sdd/manual-rewrite-report.md`. Commit only tracked worktree changes with `git add site/docs docs/superpowers/plans/2026-07-11-manual-factual-tone-revision.md && git commit -m "docs: correct manual behavior and tone"`.
