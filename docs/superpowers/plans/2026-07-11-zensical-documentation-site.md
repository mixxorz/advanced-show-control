# Zensical Documentation Site Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a lightly branded, screen-oriented Advanced Show Control user manual with Zensical, versioned `stable` and `latest` documentation, and automated GitHub Pages deployment.

**Architecture:** Public documentation lives under `site/`, separate from internal engineering documents under `docs/`. Zensical builds the site, copied visual-regression snapshots provide revision-matched screenshots, and Zensical's `mike` integration writes serialized `stable`, `latest`, and named release versions to `gh-pages`.

**Tech Stack:** Zensical 0.0.50, Zensical-compatible mike commit `2d4ad799442f4592db8ad53b179bfb33db8c69ac`, Markdown, CSS, GitHub Actions, GitHub Pages

## Global Constraints

- The audience is live sound engineers using Advanced Show Control.
- The canonical URL is `https://mitchel.me/advanced-show-control/`.
- `stable` documents the latest `v[0-9]+` release and is the root default; `latest` documents `main`.
- Public documentation lives under `site/`; internal documents under `docs/` are not published.
- Use a formal, precise technical-manual voice.
- Use imperative language for procedures and impersonal declarative language for reference descriptions.
- Use exact UI labels in bold, file names and paths as code, and user-entered values as code.
- Frontend interaction tests are the primary source for user-visible behavior. Use backend behavior only to clarify observable safety, persistence, or runtime details.
- Put safety notes beside the control or procedure they govern; do not add a standalone safety section.
- Document the visible **SAFE** control. Do not document Abort All or other unreachable controls.
- Copy selected visual-regression snapshots into `site/docs/assets/screenshots/`; do not reference test assets directly.
- Do not document the stale Sessions-tab screenshots or present the Events placeholder as functional.
- Every content task must run `zensical build --clean --strict --config-file site/zensical.toml` before commit.

---

## File Structure

- `site/zensical.toml`: canonical URL, navigation, theme, repository, validation, and mike version settings.
- `site/docs/index.md`: landing page.
- `site/docs/getting-started.md`: requirements, installation, connection dialog, first session, and first fade.
- `site/docs/application-shell.md`: navigation, status indicators, **SAFE**, and File menu/session workflows.
- `site/docs/scenes.md`: complete Scenes screen reference and workflows.
- `site/docs/cue-lists.md`: complete Cue Lists screen reference and workflows.
- `site/docs/settings.md`: visible settings and keyboard shortcut behavior.
- `site/docs/logs.md`: visible operational logs and diagnostic collection.
- `site/docs/troubleshooting.md`: symptom index linking to authoritative guides.
- `site/docs/reference/keyboard-shortcuts.md`: fixed and configurable shortcuts.
- `site/docs/reference/terminology.md`: user-facing terms.
- `site/docs/assets/stylesheets/extra.css`: restrained application-derived branding.
- `site/docs/assets/screenshots/*.png`: copied visual-regression snapshots.
- `requirements-docs.txt`: exact Zensical and mike pins shared by local and CI workflows.
- `.github/workflows/docs.yml`: build validation and serialized version deployment.
- `Makefile`: `docs-install`, `docs-build`, and `docs-serve` commands.
- `.gitignore`: generated `site/output/` and `.cache/` content.

### Task 1: Establish The Zensical Site Foundation

**Files:**
- Create: `requirements-docs.txt`
- Create: `site/zensical.toml`
- Create: `site/docs/index.md`
- Create: `site/docs/assets/stylesheets/extra.css`
- Modify: `.gitignore`
- Modify: `Makefile`

**Interfaces:**
- Produces: `make docs-install`, `make docs-build`, `make docs-serve`, and the navigation paths consumed by all later tasks.

- [ ] **Step 1: Add pinned documentation dependencies**

Create `requirements-docs.txt`:

```text
zensical==0.0.50
mike @ git+https://github.com/squidfunk/mike.git@2d4ad799442f4592db8ad53b179bfb33db8c69ac
```

- [ ] **Step 2: Add the initial Zensical configuration and landing page**

Create `site/zensical.toml`:

```toml
[project]
site_name = "Advanced Show Control"
site_url = "https://mitchel.me/advanced-show-control/"
site_description = "User manual for Advanced Show Control"
site_author = "Mitchel Cabuloy"
copyright = "Advanced Show Control is licensed under GPL-3.0-or-later."
docs_dir = "docs"
site_dir = "output"
repo_url = "https://github.com/mixxorz/advanced-show-control"
repo_name = "mixxorz/advanced-show-control"
extra_css = ["assets/stylesheets/extra.css"]
nav = [
  {"Home" = "index.md"},
  {"Getting Started" = "getting-started.md"},
  {"Application Shell" = "application-shell.md"},
  {"Scenes" = "scenes.md"},
  {"Cue Lists" = "cue-lists.md"},
  {"Settings" = "settings.md"},
  {"Logs" = "logs.md"},
  {"Troubleshooting" = "troubleshooting.md"},
  {"Reference" = [
    {"Keyboard Shortcuts" = "reference/keyboard-shortcuts.md"},
    {"Terminology" = "reference/terminology.md"}
  ]}
]

[project.theme]
variant = "modern"
features = [
  "navigation.instant",
  "navigation.instant.progress",
  "navigation.path",
  "navigation.top",
  "toc.follow"
]

[project.extra.version]
provider = "mike"
default = "stable"
alias = true
```

Create `site/docs/index.md` with:

```markdown
# Advanced Show Control

Advanced Show Control adds timed fader fades and cue-list workflows to Waves eMotion LV1 and LV1 Classic.

LV1 remains the source of truth for scenes, routing, processing, mutes, and live mixer state. Advanced Show Control stores fade settings for linked LV1 scenes and moves only the parameters placed in scope.

!!! warning "Current status"
    Advanced Show Control is under active development. Rehearse each session and workflow before show use.

[Install and connect](getting-started.md){ .md-button .md-button--primary }
[Understand the application shell](application-shell.md){ .md-button }
```

Create the remaining configured navigation files with these headings: `# Getting Started`, `# Application Shell`, `# Scenes`, `# Cue Lists`, `# Settings`, `# Logs`, `# Troubleshooting`, `# Keyboard Shortcuts`, and `# Terminology`. These title-only pages make strict navigation validation pass; Tasks 2-5 replace each one with reviewed user content.

- [ ] **Step 3: Add restrained application branding**

Create `site/docs/assets/stylesheets/extra.css`:

```css
:root {
  --asc-navy: #111820;
  --asc-slate: #5f6c78;
  --asc-amber: #d6a94a;
  --asc-green: #69b58a;
}

[data-md-color-scheme="default"] {
  --md-primary-fg-color: var(--asc-navy);
  --md-accent-fg-color: #8a641b;
}

.md-header {
  border-bottom: 2px solid var(--asc-amber);
}

.md-typeset h1 {
  letter-spacing: -0.02em;
}

.md-typeset a:not(.md-button) {
  text-decoration-color: color-mix(in srgb, currentColor 35%, transparent);
  text-underline-offset: 0.15em;
}

.md-typeset .md-button--primary {
  background-color: var(--asc-navy);
  border-color: var(--asc-navy);
}

.md-typeset .admonition.warning,
.md-typeset details.warning {
  border-color: var(--asc-amber);
}

.md-typeset .admonition.success,
.md-typeset details.success {
  border-color: var(--asc-green);
}
```

- [ ] **Step 4: Add local build commands and generated-output ignores**

Add `site/output/` and `site/.cache/` to `.gitignore`. Add `docs-install`, `docs-build`, and `docs-serve` to `.PHONY` and Make help. Use these exact recipes:

```make
docs-install:
	python3 -m pip install -r requirements-docs.txt

docs-build:
	zensical build --clean --strict --config-file site/zensical.toml

docs-serve:
	zensical serve --config-file site/zensical.toml
```

- [ ] **Step 5: Verify the foundation**

Run: `python3 -m pip install -r requirements-docs.txt`

Run: `make docs-build`

Expected: Zensical exits 0 and writes `site/output/index.html` with no strict-mode warnings.

Run: `git diff --check`

Expected: exit 0.

- [ ] **Step 6: Commit the foundation**

```bash
git add requirements-docs.txt site .gitignore Makefile
git commit -m "docs: add Zensical site foundation"
```

### Task 2: Document Getting Started And The Application Shell

**Files:**
- Modify: `site/docs/getting-started.md`
- Modify: `site/docs/application-shell.md`
- Create: `site/docs/assets/screenshots/connection-systems-found.png`
- Create: `site/docs/assets/screenshots/application-shell.png`
- Create: `site/docs/assets/screenshots/safe-active.png`

**Interfaces:**
- Consumes: navigation and image conventions from Task 1.
- Produces: onboarding and shell anchors used by troubleshooting links.

- [ ] **Step 1: Copy current user-visible screenshots**

Copy these files without modifying the source snapshots:

```text
ui/tests/visual/storybook.visual.spec.ts-snapshots/connection-connectionmodal--systems-found.png -> site/docs/assets/screenshots/connection-systems-found.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/app-appshell--scene-tab.png -> site/docs/assets/screenshots/application-shell.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/shell-toptabbar--safe-active.png -> site/docs/assets/screenshots/safe-active.png
```

- [ ] **Step 2: Write Getting Started from frontend behavior**

Use `ui/src/AppRuntime.test.tsx` and `ui/src/components/ConnectionModal.test.tsx` as primary sources. Write these exact sections in `site/docs/getting-started.md`: Requirements; Installation; First Launch; Connect To LV1; Connection States; Create A Session; Configure A First Fade; Current Limitations. Include the connection screenshot after the connection-dialog overview.

The procedure must state that the startup dialog searches for systems, unavailable systems cannot be selected, discovery refreshes while the dialog is open, and a manual connection can be opened from the console control. Do not claim support for platforms or package signing that release artifacts do not provide. State the unsigned and macOS-not-notarized limitation from the release workflow.

- [ ] **Step 3: Write the Application Shell guide**

Use `AppShell.test.tsx`, `TopTabBar.test.tsx`, `BottomStatusBar.test.tsx`, and `AppRuntime.test.tsx` as primary sources. Write these exact sections in `site/docs/application-shell.md`: Screen Overview; Navigation; Connection Control; SAFE; Bottom Status Bar; File Menu And Sessions; Window Title And Unsaved Changes; Reconnection States.

Document only visible controls. Explain that active **SAFE** prevents application-initiated recalls but does not disable LV1 controls. Cover New, Open, Save, and Save As with `.ascs` files. State current dirty-session prompting behavior exactly as asserted by frontend tests; do not describe issue #40's planned behavior as implemented. Include the shell and **SAFE** screenshots.

- [ ] **Step 4: Verify content and links**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx components/ConnectionModal.test.tsx components/TopTabBar.test.tsx components/BottomStatusBar.test.tsx`

Expected: all selected frontend tests pass.

Run: `make docs-build`

Expected: strict build passes and all three screenshots are copied into output.

- [ ] **Step 5: Commit onboarding and shell documentation**

```bash
git add site/docs/getting-started.md site/docs/application-shell.md site/docs/assets/screenshots
git commit -m "docs: add onboarding and application shell guides"
```

### Task 3: Document The Scenes Screen

**Files:**
- Modify: `site/docs/scenes.md`
- Create: `site/docs/assets/screenshots/scenes-selected.png`
- Create: `site/docs/assets/screenshots/scenes-unlinked.png`
- Create: `site/docs/assets/screenshots/scenes-duplicate-warning.png`
- Create: `site/docs/assets/screenshots/channel-scope.png`

**Interfaces:**
- Produces: `scenes.md` anchors for troubleshooting and terminology.

- [ ] **Step 1: Copy representative Scenes screenshots**

Copy:

```text
ui/tests/visual/storybook.visual.spec.ts-snapshots/scenes-scenetab--stored-scene-selected.png -> site/docs/assets/screenshots/scenes-selected.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/scenes-scenetab--link-scene-controls.png -> site/docs/assets/screenshots/scenes-unlinked.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/scenes-scenetab--duplicate-scene-warning.png -> site/docs/assets/screenshots/scenes-duplicate-warning.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/scenes-channel-scope-channelscopegrid--populated.png -> site/docs/assets/screenshots/channel-scope.png
```

- [ ] **Step 2: Write the control reference**

Use `SceneEditor.test.tsx`, `SceneListRow.test.tsx`, `SceneTab.stories.tsx`, and channel-scope stories as primary sources. Write: Screen Overview; Scene Library; Scene States; Selected Scene; Fade Duration; Parameter Scope; Channel Scope; Selected Scene Actions.

Use tables for scene-state indicators and controls. State that duration `0` is a cut and nonzero duration is constrained to the UI's `0.1` through `120` second range. Identify **Copy** as unavailable and **Paste** as disabled if that remains true in the inspected frontend revision.

- [ ] **Step 3: Write procedures and contextual safety notes**

Write: Link A Scene; Store Targets; Recall A Scene; Relink A Missing Scene; Delete An Unlinked Configuration; Duplicate Scene Names; Empty And Disconnected States.

Place exact-scene matching, blocked recall, current-live-value fade start, manual override, and disconnect notes beside Recall or the state where they apply. Use frontend-visible outcomes first; consult backend tests only where the UI tests do not define the resulting visible state. Do not mention Abort All.

- [ ] **Step 4: Verify Scenes behavior and site output**

Run: `npm --prefix ui run test -- components/SceneEditor.test.tsx components/SceneListRow.test.tsx`

Expected: all selected frontend tests pass.

Run: `make docs-build`

Expected: strict build passes and all Scenes images resolve.

- [ ] **Step 5: Commit the Scenes guide**

```bash
git add site/docs/scenes.md site/docs/assets/screenshots
git commit -m "docs: add Scenes screen guide"
```

### Task 4: Document Cue Lists

**Files:**
- Modify: `site/docs/cue-lists.md`
- Create: `site/docs/assets/screenshots/cue-list.png`
- Create: `site/docs/assets/screenshots/manage-cue-lists.png`
- Create: `site/docs/assets/screenshots/missing-cue-scene.png`

**Interfaces:**
- Produces: `cue-lists.md` anchors used by troubleshooting and shortcut reference.

- [ ] **Step 1: Copy representative Cue Lists screenshots**

Copy:

```text
ui/tests/visual/storybook.visual.spec.ts-snapshots/cue-lists-cueliststab--active-list-with-duplicates.png -> site/docs/assets/screenshots/cue-list.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/cue-lists-cueliststab--manage-modal-open.png -> site/docs/assets/screenshots/manage-cue-lists.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/cue-lists-cueliststab--missing-cued-scene.png -> site/docs/assets/screenshots/missing-cue-scene.png
```

- [ ] **Step 2: Write the Cue Lists screen reference**

Use `CueListsTab.test.tsx`, `CueListManageModal.test.tsx`, `BottomStatusBar.test.tsx`, and `AppRuntime.test.tsx` as primary sources. Write: Screen Overview; Active Cue List; Scene Library; Cue Entries; Selected And Cued States; Manage Cue Lists; GO.

Describe dragging scenes into a list, dragging entries to reorder, selecting then cueing, double-click cueing, immediate removal, creating/renaming/reordering/deleting lists, and deletion confirmation.

- [ ] **Step 3: Write recall, keyboard, and missing-state behavior**

Write: Build A Cue List; Prepare A Cue; Recall With GO; Keyboard Operation; Missing Scenes; Blocked And Disabled States.

State that GO requires a valid active list, cued entry, and linked scene configuration; duplicate pending requests are blocked; CUE and GO do not fire while focus is in dialog text input; and missing references remain visible rather than being silently discarded. Put **SAFE** and recall-safety notes beside GO.

- [ ] **Step 4: Verify Cue Lists behavior and site output**

Run: `npm --prefix ui run test -- components/CueListsTab.test.tsx components/CueListManageModal.test.tsx components/BottomStatusBar.test.tsx`

Expected: all selected frontend tests pass.

Run: `make docs-build`

Expected: strict build passes and all Cue Lists images resolve.

- [ ] **Step 5: Commit the Cue Lists guide**

```bash
git add site/docs/cue-lists.md site/docs/assets/screenshots
git commit -m "docs: add Cue Lists screen guide"
```

### Task 5: Document Settings, Logs, Troubleshooting, And Reference

**Files:**
- Modify: `site/docs/settings.md`
- Modify: `site/docs/logs.md`
- Modify: `site/docs/troubleshooting.md`
- Modify: `site/docs/reference/keyboard-shortcuts.md`
- Modify: `site/docs/reference/terminology.md`
- Create: `site/docs/assets/screenshots/settings.png`
- Create: `site/docs/assets/screenshots/logs.png`

**Interfaces:**
- Consumes: stable section anchors from Tasks 2-4.
- Produces: complete first-version user content.

- [ ] **Step 1: Copy Settings and Logs screenshots**

Copy:

```text
ui/tests/visual/storybook.visual.spec.ts-snapshots/settings-settingstab--default.png -> site/docs/assets/screenshots/settings.png
ui/tests/visual/storybook.visual.spec.ts-snapshots/logs-consolelogstab--populated.png -> site/docs/assets/screenshots/logs.png
```

- [ ] **Step 2: Write Settings from frontend tests**

Use `SettingsTab.test.tsx`, `keyboard.test.tsx`, and `shortcutFormat.test.ts` as primary sources. Cover Auto-load Last Show, Auto-save, Time Display, Fader Override Sensitivity, Extensive Diagnostics, GO Shortcut, and CUE Shortcut. Identify settings that are displayed but not wired to runtime behavior. Explain shortcut capture, Escape cancellation, modifier-only rejection, Tab capture, and conflicts with the other shortcut or fixed file accelerators.

- [ ] **Step 3: Write Logs and diagnostics guidance**

Use `ConsoleLogsTab.tsx`, its stories, and frontend log projection behavior as the user-visible source. Explain timestamp, severity, message, empty state, and operational-versus-diagnostic logs. List macOS diagnostic paths for `com.advancedshowcontrol.app` and state what to include in an issue report without exposing full show files or sensitive console state by default.

- [ ] **Step 4: Write troubleshooting and reference pages**

Create symptom entries for: no LV1 systems found; connection fails or reconnects; scene is unlinked; duplicate scene name warning; Recall is disabled or blocked; fade does not start; GO is disabled; cue displays Missing scene; session cannot be opened or saved; no visible log explains a failure. Each entry must link to the authoritative guide section and provide only the immediate diagnostic checks.

Keyboard reference must separate fixed file shortcuts from configurable GO/CUE shortcuts. Terminology must define LV1 scene, app scene configuration, linked/unlinked scene, scope, target, cut, fade, cue list, selected cue, cued entry, GO, **SAFE**, session, and `.ascs`.

- [ ] **Step 5: Verify settings behavior and the complete site**

Run: `npm --prefix ui run test -- components/SettingsTab.test.tsx keyboard.test.tsx shortcutFormat.test.ts`

Expected: all selected frontend tests pass.

Run: `make docs-build`

Expected: strict build passes with no missing navigation entries, links, or assets.

Run: `git diff --check`

Expected: exit 0.

- [ ] **Step 6: Commit supporting guides**

```bash
git add site/docs/settings.md site/docs/logs.md site/docs/troubleshooting.md site/docs/reference site/docs/assets/screenshots
git commit -m "docs: add settings logs and reference guides"
```

### Task 6: Add Versioned GitHub Pages Validation And Deployment

**Files:**
- Create: `.github/workflows/docs.yml`
- Modify: `README.md`

**Interfaces:**
- Consumes: `requirements-docs.txt`, `site/zensical.toml`, and complete site content.
- Produces: PR validation, `latest` deployment from `main`, and named `stable` deployments from `v[0-9]+` tags.

- [ ] **Step 1: Add build validation to the docs workflow**

Create `.github/workflows/docs.yml` triggered by `pull_request`, pushes to `main`, pushes of `v[0-9]+` tags, and `workflow_dispatch`. Grant `contents: write`. Add workflow concurrency:

```yaml
concurrency:
  group: documentation-pages
  cancel-in-progress: false
```

The validation job must check out the triggering revision, set up Python 3.13, install `requirements-docs.txt`, and run `zensical build --clean --strict --config-file site/zensical.toml`.

- [ ] **Step 2: Add serialized `latest` deployment**

Add a deploy job that runs only for `refs/heads/main`, depends on validation, uses full history (`fetch-depth: 0`), configures the Git author as `github-actions[bot]`, and runs:

```bash
mike deploy --config-file site/zensical.toml --push --update-aliases development latest
```

Do not update `stable` from `main`, nightly releases, or pull requests.

- [ ] **Step 3: Add serialized stable-release deployment**

For refs beginning with `refs/tags/v`, validate `GITHUB_REF_NAME` against `^v[0-9]+$`, then run:

```bash
mike deploy --config-file site/zensical.toml --push --update-aliases "${GITHUB_REF_NAME}" stable
mike set-default --config-file site/zensical.toml --push stable
```

This job must deploy from the immutable tag checkout and must not move `latest`.

- [ ] **Step 4: Document local site commands and publication URLs**

Add a `Documentation` section to `README.md` with `make docs-install`, `make docs-build`, `make docs-serve`, the canonical URL, and the meaning of `stable` and `latest`. Do not move internal development setup from the README into the public site.

- [ ] **Step 5: Validate workflow and repository checks**

Run: `npx --yes prettier@3.8.4 --write .github/workflows/docs.yml`

Run: `actionlint .github/workflows/*.yml`

Expected: exit 0.

Run: `make docs-build`

Expected: strict build passes.

Run: `make check`

Expected: all standard Rust and frontend checks pass.

Run: `git diff --check`

Expected: exit 0.

- [ ] **Step 6: Commit automation**

```bash
git add .github/workflows/docs.yml README.md
git commit -m "ci: publish versioned documentation"
```

### Task 7: Configure And Verify GitHub Pages

**Files:**
- No repository file changes expected unless GitHub records the custom domain as a `CNAME` file on `gh-pages`.

**Interfaces:**
- Consumes: `.github/workflows/docs.yml` and repository administration access.
- Produces: the public site at the canonical URL.

- [ ] **Step 1: Enable branch-based GitHub Pages publication**

Configure repository Pages to serve the root of `gh-pages`. Ensure the workflow token can write that branch and that branch-protection rules permit the GitHub Actions bot deployment.

- [ ] **Step 2: Verify inherited custom-domain routing**

Confirm that the account-level GitHub Pages site already serves `mitchel.me` and that GitHub routes this repository project site at `/advanced-show-control/`. Do not add a repository-level `CNAME` containing a path; Pages custom-domain records accept host names, not path prefixes. If the account-level custom domain is not configured, configure `mitchel.me` on the account's user Pages repository first, then recheck this project URL over HTTPS.

- [ ] **Step 3: Bootstrap both aliases**

Run the docs workflow for `main` to create `latest`. Re-run or manually dispatch the workflow at the latest stable `v[0-9]+` tag to create the named release and `stable` default.

- [ ] **Step 4: Inspect published behavior**

Open and verify:

```text
https://mitchel.me/advanced-show-control/
https://mitchel.me/advanced-show-control/stable/
https://mitchel.me/advanced-show-control/latest/
https://mitchel.me/advanced-show-control/v2/
```

Expected: the root resolves to `stable`; the selector offers `stable`, `latest`, and `v2`; screenshots, search, navigation, and internal links work under each version; `latest` is visibly development documentation.

- [ ] **Step 5: Record external setup outcome**

Add a comment to issues #34 and #24 with the public URL, successful workflow run URL, versions inspected, and any repository-setting limitation. Close each issue only when its acceptance criteria are satisfied.
