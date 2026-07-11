# Empty Scene Defaults and Scene Settings Copy/Paste Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make scene fade configuration opt-in by default and add an atomic, session-local Copy/Paste workflow for scene settings, with smoke coverage and matching public documentation.

**Architecture:** `ScenesState` owns both scene configuration and an ephemeral settings clipboard. Explicit mailbox commands copy an owned settings snapshot and atomically paste it onto a linked destination; `ScenesEvent` projects only clipboard availability, while normal persisted-edit events drive dirty tracking. Empty defaults are enforced in domain and show-file DTO defaults so omitted legacy values fail safe.

**Tech Stack:** Rust, Tokio actors, Tauri 2, serde, React 19, TypeScript, Vitest, Storybook, Playwright visual tests, Zensical.

## Global Constraints

- Copy/Paste must not recall LV1 scenes, send parameter writes, start or abort fades, or alter active-fade ownership.
- Paste must preserve destination `internal_scene_id`, `scene_index`, and `scene_name` and mutate settings atomically only after validation.
- Copy is valid for linked and unlinked sources; Paste requires an existing linked destination.
- New/Open session replacement clears the clipboard; clipboard data is never serialized.
- Omitted legacy scope fields import as `faders: false, pan: false`; explicit saved values remain unchanged.
- Actor behavior tests use mailboxes and `AppEventBus`, not private-state mutation.
- Smoke assertions cover projected configuration only; existing smoke tests retain responsibility for live fader movement.

---

### Task 1: Fail-Safe Scene and Show-File Defaults

**Files:**
- Modify: `src-tauri/src/scenes/types.rs`
- Modify: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/scenes/scene_alignment.rs`
- Test: tests colocated in those Rust modules

**Interfaces:**
- Produces: `SceneScopeToggles::default() == { faders: false, pan: false }`
- Produces: omitted `ShowFileSceneScopeToggles` fields deserialize to `false`

- [ ] **Step 1: Write failing pure unit tests for the new defaults**

Add assertions equivalent to:

```rust
#[test]
fn scene_scope_defaults_to_empty() {
    assert_eq!(
        SceneScopeToggles::default(),
        SceneScopeToggles { faders: false, pan: false }
    );
}

#[test]
fn omitted_show_file_scope_defaults_to_empty() {
    let file: ShowFile = serde_json::from_value(show_file_json_without_scope()).unwrap();
    assert_eq!(file.scene_configs[0].scope_toggles.faders, false);
    assert_eq!(file.scene_configs[0].scope_toggles.pan, false);
}
```

Cover a missing whole `scopeToggles` object, missing individual fields, explicit `true`, and explicit `false`. Expand `missing_config_gets_new_uuid_and_default_duration` to assert empty channel collections and both toggles disabled.

- [ ] **Step 2: Run the focused tests and confirm the old defaults fail**

Run: `cargo nextest run -p advanced-show-control 'scenes::types|show_file|scene_alignment'`

Expected: assertions expecting disabled fader scope fail because both current defaults enable faders.

- [ ] **Step 3: Change both Rust defaults minimally**

Use the same values in `SceneScopeToggles` and `ShowFileSceneScopeToggles`:

```rust
Self {
    faders: false,
    pan: false,
}
```

Keep `#[serde(default)]` on the DTO fields so omissions use this fail-safe value. Do not add migration compatibility code.

- [ ] **Step 4: Run the focused tests**

Run: `cargo nextest run -p advanced-show-control 'scenes::types|show_file|scene_alignment'`

Expected: PASS, including explicit-value round trips.

- [ ] **Step 5: Commit the defaults**

```bash
git add src-tauri/src/scenes/types.rs src-tauri/src/show/show_file.rs src-tauri/src/show_file.rs src-tauri/src/scenes/scene_alignment.rs
git commit -m "fix: default scene settings to empty scope"
```

### Task 2: Empty-Configuration Recall and Actor Alignment Safety

**Files:**
- Modify: `src-tauri/src/scenes/policy.rs`
- Modify: `src-tauri/src/scenes/actor.rs`

**Interfaces:**
- Consumes: empty defaults from Task 1
- Produces: default configuration yields `RecallPolicyDecision::Skip` and no fade mailbox command

- [ ] **Step 1: Add a pure policy regression test**

Construct an exact-identity scene with `SceneScopeToggles::default()`, empty `channel_configs`, and empty `scoped_channels`; assert:

```rust
assert_eq!(
    decide_scene_recall(input),
    RecallPolicyDecision::Skip {
        reason: "no applicable targets".to_string(),
    }
);
```

- [ ] **Step 2: Add actor tests through mailbox and event bus**

Expand `scene_list_changed_publishes_default_scene_configs` to assert projected empty settings. Add an observation test that receives the skip event and verifies the fake fade receiver gets no `FadeCommand::RecallSceneFade`.

- [ ] **Step 3: Run the tests before implementation changes**

Run: `cargo nextest run -p advanced-show-control scenes`

Expected: new tests pass if existing no-target policy is already correct; any fixture relying on implicit fader scope fails and identifies where explicit `faders: true` is required.

- [ ] **Step 4: Repair only active-fade fixtures**

For tests intended to exercise fader behavior, replace `SceneScopeToggles::default()` with:

```rust
SceneScopeToggles { faders: true, pan: false }
```

Do not weaken the new default or alter policy validation order.

- [ ] **Step 5: Verify and commit**

Run: `cargo nextest run -p advanced-show-control scenes`

Expected: PASS.

```bash
git add src-tauri/src/scenes/policy.rs src-tauri/src/scenes/actor.rs
git commit -m "test: cover empty scene recall safety"
```

### Task 3: Scene Settings Clipboard Domain Model

**Files:**
- Modify: `src-tauri/src/scenes/state.rs`
- Modify: `src-tauri/src/scenes/events.rs`

**Interfaces:**
- Produces: private `SceneSettingsClipboard` owned by `ScenesState`
- Produces: `copy_scene_settings(Uuid) -> Result<bool, String>`
- Produces: `paste_scene_settings(Uuid) -> Result<bool, String>`
- Produces: `ScenesProjectionState::scene_settings_clipboard_available: bool`

- [ ] **Step 1: Write failing pure state tests**

Cover linked and unlinked Copy, missing source, source snapshot independence, missing clipboard, missing destination, unlinked destination, identity preservation, changed Paste, identical no-op, source unchanged, and `replace_snapshot_for_session()` clearing availability.

Use a settings comparison helper in tests:

```rust
assert_eq!(destination.duration_ms, source.duration_ms);
assert_eq!(destination.scope_toggles, source.scope_toggles);
assert_eq!(destination.channel_configs, source.channel_configs);
assert_eq!(destination.scoped_channels, source.scoped_channels);
assert_eq!(destination.internal_scene_id, destination_id);
```

- [ ] **Step 2: Run tests and confirm missing APIs fail**

Run: `cargo nextest run -p advanced-show-control scenes::state`

Expected: compile failure for the new methods/field.

- [ ] **Step 3: Add the minimal private snapshot and state methods**

Define a private cloneable snapshot containing only:

```rust
struct SceneSettingsClipboard {
    duration_ms: u64,
    scope_toggles: SceneScopeToggles,
    channel_configs: Vec<ChannelConfig>,
    scoped_channels: Vec<ChannelRef>,
}
```

Copy clones these fields. Paste validates clipboard and destination before constructing and comparing a prospective destination. Assign all editable fields only after every check succeeds. Clear the field in `replace_snapshot_for_session()`, not ordinary state replacement.

- [ ] **Step 4: Project availability and run state tests**

Add `scene_settings_clipboard_available: self.scene_settings_clipboard.is_some()` to `projection_state()`.

Run: `cargo nextest run -p advanced-show-control scenes::state`

Expected: PASS.

- [ ] **Step 5: Commit the domain model**

```bash
git add src-tauri/src/scenes/state.rs src-tauri/src/scenes/events.rs
git commit -m "feat: add scene settings clipboard state"
```

### Task 4: Actor Commands, Atomic Events, and Projection

**Files:**
- Modify: `src-tauri/src/scenes/commands.rs`
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/projector/view.rs`
- Modify: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Produces: `ScenesCommand::CopySceneSettings { source_internal_scene_id, reply }`
- Produces: `ScenesCommand::PasteSceneSettings { destination_internal_scene_id, reply }`
- Produces: serialized `AppViewState.scene_settings_clipboard_available`

- [ ] **Step 1: Add failing actor tests**

Through mailbox commands and `AppEventBus`, assert Copy publishes `persisted_scene_edit: false` only when availability changes, changed Paste publishes once with `true`, identical Paste publishes nothing, failures publish nothing, and session replacement projects availability false. Run each command once while a pending scene observation exists to cover both actor dispatch branches.

- [ ] **Step 2: Add failing projector tests**

Extend `cache_applies_scenes_projection_state_separately_from_show_state` to apply `scene_settings_clipboard_available: true` and assert the built `AppViewState` carries it.

- [ ] **Step 3: Run tests and confirm compile failures**

Run: `cargo nextest run -p advanced-show-control 'scenes::actor|projector'`

Expected: compile failures until command variants and projection fields exist.

- [ ] **Step 4: Implement command dispatch in both actor match blocks**

Add both variants to the pending-observation and normal command matches. Copy calls the state operation and publishes a non-persisted projection only when availability changes. Paste uses the existing mutation publication path with persisted-edit semantics only when changed. Never obtain LV1 or Fade peer handles.

- [ ] **Step 5: Thread availability through projection defaults and cache**

Add the boolean to `ScenesProjectionState`, `ProjectionCache`, `AppViewState`, initial/fallback construction, cache seeding, event application, and snapshot building. Default it to `false`.

- [ ] **Step 6: Run actor and projector tests**

Run: `cargo nextest run -p advanced-show-control 'scenes::actor|projector'`

Expected: PASS.

- [ ] **Step 7: Commit actor and projection wiring**

```bash
git add src-tauri/src/scenes/commands.rs src-tauri/src/scenes/actor.rs src-tauri/src/projector src-tauri/src/lifecycle/mod.rs
git commit -m "feat: route scene settings copy paste"
```

### Task 5: Tauri and Frontend Command Contracts

**Files:**
- Modify: `src-tauri/src/ui/commands/scenes.rs`
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `ui/src/commands.ts`
- Modify: `ui/src/commands.test.ts`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`
- Modify: `ui/src/appContext.tsx`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/storybook/mockAppCommands.ts`
- Modify: `ui/src/storybook/mockAppState.ts`

**Interfaces:**
- Produces: Tauri commands `copy_scene_settings` and `paste_scene_settings`
- Produces: frontend callbacks `copySceneSettings(internalSceneId)` and `pasteSceneSettings(internalSceneId)`

- [ ] **Step 1: Add failing invoke-wrapper tests**

Assert each wrapper invokes the exact snake-case command with `{ internalSceneId }`.

- [ ] **Step 2: Run frontend command tests**

Run: `npm --prefix ui run test -- commands`

Expected: compile/test failure until wrappers exist.

- [ ] **Step 3: Add thin Tauri adapters and registration**

Follow existing scene command adapters: obtain `ScenesHandle`, create a oneshot, send the explicit command variant, await it, and map errors to strings. Re-export and register both commands; extend the existing command export compile test.

- [ ] **Step 4: Wire frontend services and projected type**

Add both callbacks to `AppCommands`, `AppRuntimeServices`, production services, runtime command construction, test mocks, and Storybook noops. Add required `sceneSettingsClipboardAvailable: boolean` to `AppViewState`, disconnected state, and fixtures.

- [ ] **Step 5: Verify contracts**

Run: `cargo nextest run -p advanced-show-control ui::`

Run: `npm --prefix ui run test -- commands AppRuntime`

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 6: Commit contract wiring**

```bash
git add src-tauri/src/ui ui/src/commands.ts ui/src/commands.test.ts ui/src/App.tsx ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx ui/src/appContext.tsx ui/src/types.ts ui/src/storybook
git commit -m "feat: expose scene settings clipboard commands"
```

### Task 6: Copy/Paste Controls and Storybook States

**Files:**
- Modify: `ui/src/components/SelectedSceneActions.tsx`
- Create: `ui/src/components/SelectedSceneActions.test.tsx`
- Modify: `ui/src/components/SelectedSceneActions.stories.tsx`

**Interfaces:**
- Consumes: projected `sceneSettingsClipboardAvailable` and command callbacks from Task 5
- Produces: accessible Copy/Paste interaction states

- [ ] **Step 1: Write failing interaction tests**

Render with app providers and assert Copy is enabled for linked and unlinked scenes; Paste is disabled without clipboard, disabled for unlinked destinations, enabled for linked destinations with clipboard; clicks dispatch the selected scene ID.

- [ ] **Step 2: Run the component tests**

Run: `npm --prefix ui run test -- SelectedSceneActions`

Expected: failures because Copy has no handler and Paste is permanently disabled.

- [ ] **Step 3: Implement the minimal UI logic**

Read projected availability and use:

```tsx
<ConsoleButton onClick={() => commands.copySceneSettings(scene.internalSceneId)}>
  Copy
</ConsoleButton>
<ConsoleButton
  disabled={!sceneSettingsClipboardAvailable || scene.sceneIndex === null}
  onClick={() => commands.pasteSceneSettings(scene.internalSceneId)}
>
  Paste
</ConsoleButton>
```

Keep native disabled semantics.

- [ ] **Step 4: Add explicit unavailable and available stories**

Use deterministic linked/unlinked scene fixtures and projected clipboard values so Storybook tests and visual snapshots cover each state.

- [ ] **Step 5: Verify and commit UI behavior**

Run: `npm --prefix ui run test -- SelectedSceneActions SceneEditor`

Run: `npm --prefix ui run test:storybook`

Expected: PASS.

```bash
git add ui/src/components/SelectedSceneActions.tsx ui/src/components/SelectedSceneActions.test.tsx ui/src/components/SelectedSceneActions.stories.tsx
git commit -m "feat: enable scene settings copy paste controls"
```

### Task 7: Projected-Configuration Debug Smoke Coverage

**Files:**
- Modify: `ui/src/debug/main.tsx`
- Test artifact: `logs/debug-smoke-report.txt`

**Interfaces:**
- Consumes: production Copy/Paste Tauri commands and projected clipboard availability
- Produces: smoke assertions for #46 and #45 without additional recalls or fader movement

- [ ] **Step 1: Update smoke setup for explicit scope**

After New Session, assert both toggles and both channel arrays are empty. Existing fade scenarios that require faders must explicitly call `set_scene_scope_faders_enabled(..., true)` rather than relying on defaults.

- [ ] **Step 2: Add the Copy/Paste smoke scenario**

Capture source A before Copy, configure it through production commands, Copy A, Paste to linked B, then wait for projection and assert:

```ts
expect(destination.internalSceneId).toBe(destinationId);
expect(destination.sceneIndex).toBe(destinationIndex);
expect(destination.sceneName).toBe(destinationName);
expect(destination.durationMs).toBe(source.durationMs);
expect(destination.scopeToggles).toEqual(source.scopeToggles);
expect(destination.channelConfigs).toEqual(source.channelConfigs);
expect(destination.scopedChannels).toEqual(source.scopedChannels);
expect(sourceAfterPaste).toEqual(sourceBeforePaste);
expect(state.showFileDirty).toBe(true);
```

Create a new session and assert `sceneSettingsClipboardAvailable` is false. Do not recall B in this scenario.

- [ ] **Step 3: Run smoke and inspect the authoritative report**

Run: `make smoke`

Then read: `logs/debug-smoke-report.txt`

Expected: report records the full suite as passed, including empty defaults and projected Copy/Paste configuration. If LV1-compatible hardware is unavailable, record this verification as blocked rather than claiming success.

- [ ] **Step 4: Commit smoke coverage**

```bash
git add ui/src/debug/main.tsx
git commit -m "test: smoke scene settings copy paste"
```

### Task 8: Visual Assets and Public Site Documentation

**Files:**
- Modify: `site/docs/getting-started.md`
- Modify: `site/docs/scenes.md`
- Modify: `site/docs/assets/screenshots/scenes-selected.png`
- Modify: `site/docs/assets/screenshots/scenes-unlinked.png`
- Modify: `site/docs/assets/screenshots/channel-scope.png`
- Source snapshots: `ui/tests/visual/storybook.visual.spec.ts-snapshots/`

**Interfaces:**
- Consumes: final UI behavior from Task 6
- Produces: operator documentation matching empty defaults and Copy/Paste behavior

- [ ] **Step 1: Update the manual text**

In Getting Started, require selecting intended channels and explicitly enabling **FADER** and optionally **PAN**. In Scenes, document empty defaults, copied fields, preserved destination identity, unlinked Copy, linked-only Paste, session-change clipboard clearing, and identical Paste no-op behavior. Remove the obsolete statement that Copy is inert and Paste unavailable.

- [ ] **Step 2: Run visual regression and update intentional snapshots**

Run: `make visual-test`

If failures reflect the intentional Copy/Paste states, run: `make visual-update`

Run `make visual-test` again and expect PASS. Copy only affected stable screenshots into `site/docs/assets/screenshots/`; do not link site Markdown to generated test paths.

- [ ] **Step 3: Build the public site strictly**

Run: `make docs-build`

Expected: `zensical build --clean --strict --config-file site/zensical.toml` succeeds with valid links and image references.

- [ ] **Step 4: Commit documentation and intentional images**

```bash
git add site/docs ui/tests/visual/storybook.visual.spec.ts-snapshots
git commit -m "docs: explain scene settings copy paste"
```

### Task 9: Full Verification and Issue Closure Readiness

**Files:**
- Verify only; modify files only to fix discovered regressions

**Interfaces:**
- Consumes: all previous tasks
- Produces: CI-equivalent evidence for issues #46 and #45

- [ ] **Step 1: Run standard verification**

Run: `make check`

Expected: Rust formatting, clippy, nextest, build, frontend formatting, lint, typecheck, build, Vitest, and Storybook tests all PASS.

- [ ] **Step 2: Re-run documentation verification**

Run: `make docs-build`

Expected: PASS.

- [ ] **Step 3: Inspect final repository state**

Run: `git status --short`

Expected: clean worktree, or only unrelated pre-existing changes that are identified and left untouched.

- [ ] **Step 4: Review acceptance criteria**

Confirm #46 empty defaults/import/recall behavior and #45 snapshot/identity/dirty/no-op/invalid-destination/UI behavior each have a passing test or smoke assertion. Confirm site documentation states the shipped behavior.
