# Scene Tab Component Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `SceneTab` into focused, storyable components while preserving behavior and matching `designs/01 SCENES.png`.

**Architecture:** Keep `SceneTab` as the only hook-driven container. Move all visible pieces into prop-driven files under `ui/src/components/scene-tab/`, with shared button primitives under `ui/src/components/ui/`. Update Storybook fixtures to use the expected LV1 topology so visual tests represent the real layout.

**Tech Stack:** React 19, TypeScript, Storybook, Vitest Storybook tests, Playwright visual snapshots, Tailwind CSS v4.

## Global Constraints

- Preserve existing Tauri `AppViewState` and command contracts.
- Do not change scene recall behavior, fade safety logic, backend data ownership, or command semantics.
- `SceneTab` keeps local-only `duplicateSceneNames()`; do not create a helper file for it.
- Each React component lives in its own file.
- Each component gets its own Storybook story file.
- Inputs render as one continuous `1-80` grid, not banks.
- Default Storybook topology: Inputs `1-80`, Groups `1-16`, Auxes `1-24`, Masters `LR`, `C`, `Mono`, Matrix `1-8`, Link/DCAs `1-16`.
- Hidden/control group `24` is not part of the default visual fixture.
- Each task implements exactly one component, plus that component's story when applicable.
- Every component task ends by comparing its Storybook/Playwright visual output against `designs/01 SCENES.png` and iterating until it matches the relevant portion of the design.
- Commit after each task.

---

## File Structure

- Create `ui/src/components/ui/Button.tsx`: generic action button.
- Create `ui/src/components/ui/Button.stories.tsx`: action button states.
- Create `ui/src/components/ui/ToggleButton.tsx`: generic pressed/unpressed button.
- Create `ui/src/components/ui/ToggleButton.stories.tsx`: toggle states.
- Create `ui/src/components/scene-tab/SceneListRow.tsx`: one scene list row.
- Create `ui/src/components/scene-tab/SceneListRow.stories.tsx`: selected/current/cued row states.
- Create `ui/src/components/scene-tab/SceneStatusLegend.tsx`: scene status legend.
- Create `ui/src/components/scene-tab/SceneStatusLegend.stories.tsx`: legend story.
- Create `ui/src/components/scene-tab/SceneList.tsx`: left scene list panel.
- Create `ui/src/components/scene-tab/SceneList.stories.tsx`: populated, empty, duplicate warning stories.
- Create `ui/src/components/scene-tab/CrossfadeInput.tsx`: X-fade input.
- Create `ui/src/components/scene-tab/CrossfadeInput.stories.tsx`: normal and immediate stories.
- Create `ui/src/components/scene-tab/SceneScopeControls.tsx`: FADER/PAN toggle pair.
- Create `ui/src/components/scene-tab/SceneScopeControls.stories.tsx`: toggle combinations.
- Create `ui/src/components/scene-tab/SelectedSceneToolbar.tsx`: selected scene toolbar.
- Create `ui/src/components/scene-tab/SelectedSceneToolbar.stories.tsx`: toolbar story.
- Create `ui/src/components/scene-tab/ChannelScopeButton.tsx`: one channel scope button.
- Create `ui/src/components/scene-tab/ChannelScopeButton.stories.tsx`: scoped/unscoped stories.
- Create `ui/src/components/scene-tab/ChannelScopeSection.tsx`: one channel section.
- Create `ui/src/components/scene-tab/ChannelScopeSection.stories.tsx`: inputs, masters, and Link/DCA examples.
- Create `ui/src/components/scene-tab/ChannelScopePanel.tsx`: channel scope panel.
- Create `ui/src/components/scene-tab/ChannelScopePanel.stories.tsx`: full topology and empty stored-state stories.
- Modify `ui/src/components/SceneTab.tsx`: compose extracted components and keep `duplicateSceneNames()` local.
- Modify `ui/src/components/SceneTab.stories.tsx`: root integration stories with full topology.
- Modify `ui/src/storybook/mockAppState.ts`: default full topology fixture.
- Modify `ui/tests/visual/storybook.visual.spec.ts`: include the root SceneTab story and add component stories if useful for stable visual baselines.

---

### Task 1: Shared Button Component

**Files:**
- Create: `ui/src/components/ui/Button.tsx`
- Create: `ui/src/components/ui/Button.stories.tsx`

**Interfaces:**
- Produces: `Button(props: { children: ReactNode; disabled?: boolean; onClick?: () => void; variant?: "primary" | "secondary" | "ghost"; className?: string }): JSX.Element`
- Consumed by later scene action buttons and All/None controls.

- [ ] **Step 1: Create the component and story**

Use a compact console-style bordered button. Primary is orange, secondary is dark bordered, ghost is transparent/dim, disabled is visibly unavailable.

- [ ] **Step 2: Run Storybook test for Button**

Run: `npm run test:storybook -- Button` in `ui/`

Expected: PASS or no matching tests if Storybook test filtering finds no play test.

- [ ] **Step 3: Visual review loop**

Open `Components/UI/Button` in Storybook or capture it with Playwright. Compare primary/secondary/disabled states to the Cue/Recall/Store/Copy/Paste and All/None buttons in `designs/01 SCENES.png`. Adjust spacing, border, color, and disabled treatment until it matches the design language.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/ui/Button.tsx ui/src/components/ui/Button.stories.tsx && git commit -m "feat: add shared scene button"`

---

### Task 2: Shared ToggleButton Component

**Files:**
- Create: `ui/src/components/ui/ToggleButton.tsx`
- Create: `ui/src/components/ui/ToggleButton.stories.tsx`

**Interfaces:**
- Consumes: React `ReactNode`.
- Produces: `ToggleButton(props: { children: ReactNode; pressed: boolean; disabled?: boolean; onClick?: () => void; title?: string; className?: string }): JSX.Element`
- Consumed by FADER/PAN controls and channel scope buttons.

- [ ] **Step 1: Create the component and story**

Use orange filled active state and dark inactive state with compact console borders.

- [ ] **Step 2: Run Storybook test for ToggleButton**

Run: `npm run test:storybook -- ToggleButton` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare active/inactive toggle states to FADER/PAN and channel buttons in `designs/01 SCENES.png`. Iterate until active orange, inactive dark, borders, and label weight match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/ui/ToggleButton.tsx ui/src/components/ui/ToggleButton.stories.tsx && git commit -m "feat: add shared toggle button"`

---

### Task 3: SceneListRow Component

**Files:**
- Create: `ui/src/components/scene-tab/SceneListRow.tsx`
- Create: `ui/src/components/scene-tab/SceneListRow.stories.tsx`

**Interfaces:**
- Consumes: `SceneConfig`, `formatSceneNumber`, `formatSceneDurationSummary`.
- Produces: `SceneListRow(props: { scene: SceneConfig; selected: boolean; current: boolean; cued: boolean; onSelect: () => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render a grid row with scene number, scene name, and X-fade. Show selected with orange border/background, current with green marker/text, cued with blue marker/text, and selected plus cued with combined treatment.

- [ ] **Step 2: Run Storybook test for SceneListRow**

Run: `npm run test:storybook -- SceneListRow` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare the row story to the left Scene List rows in `designs/01 SCENES.png`, especially rows `003` and `004`. Iterate until marker, row height, text alignment, and selected orange treatment match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/SceneListRow.tsx ui/src/components/scene-tab/SceneListRow.stories.tsx && git commit -m "feat: add scene list row"`

---

### Task 4: SceneStatusLegend Component

**Files:**
- Create: `ui/src/components/scene-tab/SceneStatusLegend.tsx`
- Create: `ui/src/components/scene-tab/SceneStatusLegend.stories.tsx`

**Interfaces:**
- Produces: `SceneStatusLegend(): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render four legend items: `ACTIVE`, `CUED (NEXT)`, `SELECTED`, `SELECTED & CUED`.

- [ ] **Step 2: Run Storybook test for SceneStatusLegend**

Run: `npm run test:storybook -- SceneStatusLegend` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare legend swatches and labels to the bottom-left legend in `designs/01 SCENES.png`. Iterate until colors and spacing match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/SceneStatusLegend.tsx ui/src/components/scene-tab/SceneStatusLegend.stories.tsx && git commit -m "feat: add scene status legend"`

---

### Task 5: SceneList Component

**Files:**
- Create: `ui/src/components/scene-tab/SceneList.tsx`
- Create: `ui/src/components/scene-tab/SceneList.stories.tsx`

**Interfaces:**
- Consumes: `SceneListRow`, `SceneStatusLegend`, `SceneConfig`.
- Produces: `SceneList(props: { scenes: SceneConfig[]; selectedSceneId: string | null; currentScene: { index: number; name: string } | null; cuedSceneId?: string | null; duplicateNames: string[]; onSelectScene: (sceneId: string) => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render panel title `SCENE LIST`, table headers, duplicate warning when provided, empty state when no scenes exist, rows, and legend.

- [ ] **Step 2: Run Storybook test for SceneList**

Run: `npm run test:storybook -- SceneList` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare the populated story against the full left rail in `designs/01 SCENES.png`. Iterate until panel proportions, header spacing, row density, and legend placement match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/SceneList.tsx ui/src/components/scene-tab/SceneList.stories.tsx && git commit -m "feat: add scene list panel"`

---

### Task 6: CrossfadeInput Component

**Files:**
- Create: `ui/src/components/scene-tab/CrossfadeInput.tsx`
- Create: `ui/src/components/scene-tab/CrossfadeInput.stories.tsx`
- Modify: `ui/src/components/DurationInput.tsx` only if shared logic is moved or deleted after replacement.

**Interfaces:**
- Consumes: existing `DurationInput` behavior.
- Produces: `CrossfadeInput(props: { sceneId: string; durationMs: number; setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean> }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Port the draft/commit behavior from `DurationInput` and render as X-fade seconds with compact up/down controls matching the design.

- [ ] **Step 2: Run Storybook test for CrossfadeInput**

Run: `npm run test:storybook -- CrossfadeInput` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare the story to the X-FADE control in the selected toolbar in `designs/01 SCENES.png`. Iterate until width, orange numeric value, border, and stepper placement match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/CrossfadeInput.tsx ui/src/components/scene-tab/CrossfadeInput.stories.tsx ui/src/components/DurationInput.tsx && git commit -m "feat: add crossfade input"`

---

### Task 7: SceneScopeControls Component

**Files:**
- Create: `ui/src/components/scene-tab/SceneScopeControls.tsx`
- Create: `ui/src/components/scene-tab/SceneScopeControls.stories.tsx`

**Interfaces:**
- Consumes: `ToggleButton`.
- Produces: `SceneScopeControls(props: { fadersEnabled: boolean; panEnabled: boolean; onToggleFaders: () => void; onTogglePan: () => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render label `SCENE SCOPE` and FADER/PAN toggle buttons.

- [ ] **Step 2: Run Storybook test for SceneScopeControls**

Run: `npm run test:storybook -- SceneScopeControls` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare to the `SCENE SCOPE` area in `designs/01 SCENES.png`. Iterate until label, toggle size, gap, and active state match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/SceneScopeControls.tsx ui/src/components/scene-tab/SceneScopeControls.stories.tsx && git commit -m "feat: add scene scope controls"`

---

### Task 8: SelectedSceneToolbar Component

**Files:**
- Create: `ui/src/components/scene-tab/SelectedSceneToolbar.tsx`
- Create: `ui/src/components/scene-tab/SelectedSceneToolbar.stories.tsx`

**Interfaces:**
- Consumes: `Button`, `CrossfadeInput`, `SceneScopeControls`, `SceneConfig`.
- Produces: `SelectedSceneToolbar(props: { scene: SceneConfig; setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>; onToggleFaders: () => void; onTogglePan: () => void; onStore: () => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render selected scene number/name, scene scope controls, X-fade input, Store action, and disabled placeholder action buttons for unsupported Cue/Recall/Copy/Paste behavior.

- [ ] **Step 2: Run Storybook test for SelectedSceneToolbar**

Run: `npm run test:storybook -- SelectedSceneToolbar` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare to the full top selected-scene strip in `designs/01 SCENES.png`. Iterate until vertical dividers, spacing, typography, and action button alignment match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/SelectedSceneToolbar.tsx ui/src/components/scene-tab/SelectedSceneToolbar.stories.tsx && git commit -m "feat: add selected scene toolbar"`

---

### Task 9: ChannelScopeButton Component

**Files:**
- Create: `ui/src/components/scene-tab/ChannelScopeButton.tsx`
- Create: `ui/src/components/scene-tab/ChannelScopeButton.stories.tsx`

**Interfaces:**
- Consumes: `ToggleButton`.
- Produces: `ChannelScopeButton(props: { label: string; scoped: boolean; title: string; onToggle: () => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render one compact numeric/label toggle.

- [ ] **Step 2: Run Storybook test for ChannelScopeButton**

Run: `npm run test:storybook -- ChannelScopeButton` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare scoped/unscoped buttons to the channel grid buttons in `designs/01 SCENES.png`. Iterate until dimensions, orange fill, inactive fill, and numeric text match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/ChannelScopeButton.tsx ui/src/components/scene-tab/ChannelScopeButton.stories.tsx && git commit -m "feat: add channel scope button"`

---

### Task 10: ChannelScopeSection Component

**Files:**
- Create: `ui/src/components/scene-tab/ChannelScopeSection.tsx`
- Create: `ui/src/components/scene-tab/ChannelScopeSection.stories.tsx`

**Interfaces:**
- Consumes: `ChannelScopeButton`, `ChannelConfig`, `ChannelSummary`, existing format helpers.
- Produces: `ChannelScopeSection(props: { title: string; configs: ChannelConfig[]; channels: ChannelSummary[]; scopedKeys: Set<string>; sceneId: string; onSetChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Render a titled section and a wrapping grid. Inputs story must show `1-80` in one continuous grid. Masters story must show `LR`, `C`, `Mono`.

- [ ] **Step 2: Run Storybook test for ChannelScopeSection**

Run: `npm run test:storybook -- ChannelScopeSection` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare inputs, groups/auxes, masters, matrix, and Link/DCA section stories to the channel scope area in `designs/01 SCENES.png`. Iterate until section title, panel border, and grid density match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/ChannelScopeSection.tsx ui/src/components/scene-tab/ChannelScopeSection.stories.tsx && git commit -m "feat: add channel scope section"`

---

### Task 11: ChannelScopePanel Component

**Files:**
- Create: `ui/src/components/scene-tab/ChannelScopePanel.tsx`
- Create: `ui/src/components/scene-tab/ChannelScopePanel.stories.tsx`

**Interfaces:**
- Consumes: `Button`, `ChannelScopeSection`, `AppViewState["channels"]`, `SceneConfig`.
- Produces: `ChannelScopePanel(props: { channels: AppViewState["channels"]; scene: SceneConfig; setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void; setAllChannelsScoped: (sceneId: string, scoped: boolean) => void }): JSX.Element`

- [ ] **Step 1: Create the component and story**

Group configs by existing `channelDisplayGroup()` and sort by `channelDisplayGroupOrder()`. Render header `CHANNEL SCOPE`, All/None actions, empty stored-state message, and sections.

- [ ] **Step 2: Run Storybook test for ChannelScopePanel**

Run: `npm run test:storybook -- ChannelScopePanel` in `ui/`

Expected: PASS or no matching tests if no play test is selected.

- [ ] **Step 3: Visual review loop**

Compare full topology story to the entire channel scope region in `designs/01 SCENES.png`. Ensure inputs are one continuous `1-80` grid and groups match the required topology. Iterate until layout and density match.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/components/scene-tab/ChannelScopePanel.tsx ui/src/components/scene-tab/ChannelScopePanel.stories.tsx && git commit -m "feat: add channel scope panel"`

---

### Task 12: Full Topology Story Fixture

**Files:**
- Modify: `ui/src/storybook/mockAppState.ts`

**Interfaces:**
- Produces: default `connectedAppState` with Inputs `1-80`, Groups `1-16`, Auxes `1-24`, Masters `LR/C/Mono`, Matrix `1-8`, Link/DCAs `1-16`.
- Consumed by all SceneTab integration stories.

- [ ] **Step 1: Update fixture helpers**

Create helper functions in `mockAppState.ts` to generate channel summaries and scene channel configs for the expected topology. Use zero-based internal channel numbers with one-based display labels from `format.ts`.

- [ ] **Step 2: Run Storybook tests**

Run: `npm run test:storybook -- SceneTab` in `ui/`

Expected: existing SceneTab assertions pass or are updated in Task 13.

- [ ] **Step 3: Visual review loop**

Open any component story consuming `connectedAppState` and verify topology matches the design and user-provided counts. Iterate until the fixture produces all expected groups.

- [ ] **Step 4: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add ui/src/storybook/mockAppState.ts && git commit -m "test: expand scene story topology"`

---

### Task 13: SceneTab Container Composition

**Files:**
- Modify: `ui/src/components/SceneTab.tsx`
- Modify: `ui/src/components/SceneTab.stories.tsx`

**Interfaces:**
- Consumes: `SceneList`, `SelectedSceneToolbar`, `ChannelScopePanel`.
- Produces: existing named export `SceneTab()`.

- [ ] **Step 1: Refactor SceneTab**

Keep `duplicateSceneNames()` local. Use hooks only in `SceneTab`. Pass app state and command callbacks into child components.

- [ ] **Step 2: Update SceneTab stories**

Keep `StoredSceneSelected`, `DuplicateSceneWarning`, `ChorusSelected`, and `NoScenes`. Ensure `StoredSceneSelected` asserts `SCENE LIST`, selected scene heading, and at least one full-topology group label.

- [ ] **Step 3: Run Storybook tests**

Run: `npm run test:storybook -- SceneTab` in `ui/`

Expected: PASS.

- [ ] **Step 4: Visual review loop**

Compare `Components/SceneTab/StoredSceneSelected` against `designs/01 SCENES.png`. Iterate until the composed SceneTab matches the design as closely as practical for the current app shell.

- [ ] **Step 5: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 6: Commit**

Run: `git add ui/src/components/SceneTab.tsx ui/src/components/SceneTab.stories.tsx && git commit -m "feat: split scene tab components"`

---

### Task 14: Visual Test Coverage

**Files:**
- Modify: `ui/tests/visual/storybook.visual.spec.ts`
- Modify: `ui/tests/visual/storybook.visual.spec.ts-snapshots/*` if snapshots intentionally change.

**Interfaces:**
- Consumes: Storybook story ids from all prior tasks.
- Produces: visual regression coverage for the composed SceneTab and selected component stories.

- [ ] **Step 1: Update visual story list**

Keep `components-scenetab--stored-scene-selected`. Add stable component story ids only if they materially protect against regressions without making the visual suite too slow.

- [ ] **Step 2: Build Storybook**

Run: `npm run build-storybook` in `ui/`

Expected: PASS.

- [ ] **Step 3: Update visual snapshots**

Run: `npm run test:visual:update` in `ui/`

Expected: PASS and intentional snapshots update.

- [ ] **Step 4: Compare snapshots to design**

Open the updated `components-scenetab--stored-scene-selected.png` snapshot and compare it to `designs/01 SCENES.png`. Iterate on component styling until the full SceneTab snapshot matches the design reference.

- [ ] **Step 5: Run visual tests**

Run: `npm run test:visual` in `ui/`

Expected: PASS.

- [ ] **Step 6: Commit**

Run: `git add ui/tests/visual/storybook.visual.spec.ts ui/tests/visual/storybook.visual.spec.ts-snapshots && git commit -m "test: update scene tab visual coverage"`

---

### Task 15: Final Verification

**Files:**
- Modify docs only if implementation diverges from `docs/superpowers/specs/2026-06-17-scene-tab-component-split-design.md`.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: verified SceneTab split ready for review.

- [ ] **Step 1: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 2: Run frontend build**

Run: `npm run build` in `ui/`

Expected: PASS.

- [ ] **Step 3: Run Storybook tests**

Run: `npm run test:storybook` in `ui/`

Expected: PASS.

- [ ] **Step 4: Run visual tests**

Run: `npm run test:visual` in `ui/`

Expected: PASS.

- [ ] **Step 5: Inspect git status**

Run: `git status --short`

Expected: clean, or only intentional docs drift that must be committed.

---

## Self-Review

- Spec coverage: The plan covers every component from the approved spec, keeps `duplicateSceneNames()` local, includes the full topology fixture, keeps inputs as one continuous `1-80` grid, and includes per-component visual review loops.
- Placeholder scan: No TBD/TODO placeholders remain. Unsupported Cue/Recall/Copy/Paste actions are explicitly disabled placeholders because commands do not exist in this scope.
- Type consistency: Component signatures use existing `SceneConfig`, `ChannelConfig`, `AppViewState["channels"]`, and command callback shapes from `SceneTab.tsx`.
