# Frontend Visual Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the Storybook AppShell and SceneTab screenshots closer to `designs/01 SCENES.png`.

**Architecture:** Keep the existing component boundaries. First make the AppShell story use connected/populated data, then tune compact layout and neutral console styling in the existing one-file components, then regenerate visual snapshots.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4, Storybook, Playwright visual snapshots.

## Global Constraints

- Work in `/Users/mixxorz/Projects/lv1-scene-fade-utility/.worktrees/frontend-shell-scenes` on branch `frontend-shell-scenes`.
- Keep the existing Tauri `AppViewState` and command contracts unchanged.
- Use `designs/01 SCENES.png` as the visual reference.
- Make `app-appshell--scene-tab` use connected/populated state before comparing visuals.
- Keep all Storybook scene backgrounds neutral black/charcoal; avoid navy or green-tinted page backgrounds.
- Preserve IBM Plex Sans for UI text and IBM Plex Mono for numeric/status text.
- Preserve one React component per file.
- Do not implement Playlists, Events, Sessions, or Settings workflows.
- Regenerate visual snapshots after intentional visual changes.

---

## File Structure

- Modify `ui/src/components/AppShell.stories.tsx`: ensure the SceneTab story passes `connectedAppState`.
- Modify `ui/src/components/SelectedSceneHeader.tsx`: compact the selected scene header to match the reference.
- Modify `ui/src/components/DurationInput.tsx`: allow compact X-Fade styling if needed.
- Modify `ui/src/components/ChannelScopeGrid.tsx`: tighten group spacing and button layout.
- Modify `ui/src/components/ScopeButton.tsx`: make scope buttons denser and reference-like.
- Modify `ui/src/components/SceneListRow.tsx`: improve selected/current row reference styling.
- Modify `ui/src/index.css`: add any missing reusable tokens only if needed.
- Modify visual snapshots under `ui/tests/visual/storybook.visual.spec.ts-snapshots/`.
- Modify `docs/superpowers/specs/2026-06-17-real-frontend-shell-scenes-design.md`: already updated with the polish loop; commit it with the polish work.

---

### Task 1: Make Visual Comparison Populated

**Files:**
- Modify: `ui/src/components/AppShell.stories.tsx`

**Steps:**

- [ ] Set `SceneTab.args.appState` to `connectedAppState` so the full-shell snapshot includes scene rows, selected scene, scope grid, and bottom bar.
- [ ] Run `npm run test:storybook -- AppShell` in `ui/` and expect PASS.
- [ ] Commit with `test: use populated app shell story`.

---

### Task 2: Compact Header And Scope Styling

**Files:**
- Modify: `ui/src/components/SelectedSceneHeader.tsx`
- Modify: `ui/src/components/DurationInput.tsx`
- Modify: `ui/src/components/ScopeButton.tsx`
- Modify: `ui/src/components/ChannelScopeGrid.tsx`
- Modify: `ui/src/components/SceneListRow.tsx`
- Modify: `ui/src/index.css` only if adding reusable tokens is necessary.
- Modify: `docs/superpowers/specs/2026-06-17-real-frontend-shell-scenes-design.md`

**Steps:**

- [ ] Compact `SelectedSceneHeader` into a horizontal reference-like layout with selected scene identity, scene scope, X-Fade duration, and actions in one row on wide screens.
- [ ] Use `X-Fade` wording where the reference uses it.
- [ ] Keep unsupported actions disabled; do not add behavior.
- [ ] Tighten `ScopeButton` dimensions and `ChannelScopeGrid` group spacing.
- [ ] Keep page/shell backgrounds neutral black/charcoal.
- [ ] Run `npm run typecheck` in `ui/` and expect PASS.
- [ ] Run `npm run build` in `ui/` and expect PASS.
- [ ] Commit with `feat: polish console scene visuals`.

---

### Task 3: Regenerate And Verify Visual Snapshots

**Files:**
- Modify: `ui/tests/visual/storybook.visual.spec.ts-snapshots/app-appshell--scene-tab.png`
- Modify: `ui/tests/visual/storybook.visual.spec.ts-snapshots/components-scenetab--stored-scene-selected.png`

**Steps:**

- [ ] Run `npm run build-storybook` in `ui/` and expect PASS.
- [ ] Run `npm run test:visual:update` in `ui/` and expect PASS.
- [ ] Run `npm run test:visual` in `ui/` and expect PASS.
- [ ] Run `git status --short` and confirm only intended files are changed.
- [ ] Commit with `test: update polished console snapshots`.

---

## Self-Review

- Spec coverage: plan covers populated AppShell screenshot, neutral backgrounds, compact selected header, denser channel scope, and snapshot regeneration.
- Placeholder scan: no unresolved placeholders.
- Type consistency: changes stay within existing component exports and app-state contracts.
