# Scene Scope Toggles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-scene fader scope enablement so `duration_ms == 0` means immediate fader movement instead of disabled recall automation.

**Architecture:** Store a `SceneScopeToggles { faders: bool }` object on each scene config and carry it through core state, Tauri DTOs, show files, and UI snapshots. Recall policy skips only when fader scope is disabled; otherwise it starts a fade config even with `duration_ms == 0`, letting the existing fade engine complete immediately on its first tick.

**Tech Stack:** Rust core crate, Tauri Rust shell, serde JSON DTOs, React/TypeScript UI, Tailwind CSS, cargo tests, npm typecheck/build.

---

## File Structure

- Modify `src/show/types.rs`: add `SceneScopeToggles` and `scope_toggles` to `SceneConfig`.
- Modify `src/show/capture.rs`: preserve or default scope toggles while storing scene configs, allow duration `0`, and add `set_scene_scope_faders_enabled`.
- Modify `src/show/commands.rs`, `src/show/actor.rs`, `src/show/handle.rs`: route the new show command.
- Modify `src/show/state.rs`: update tests and test helpers for `scope_toggles`.
- Modify `src/scene_recall/policy.rs`: skip disabled fader scope and stop skipping duration `0`.
- Modify `src-tauri/src/show_file.rs`: add show-file `ShowFileSceneScopeToggles` with serde default.
- Modify `src-tauri/src/app_state/show_file_mapping.rs`: map scope toggles to and from show files.
- Modify `src-tauri/src/app_state/shell.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`: expose `set_scene_scope_faders_enabled`.
- Modify `src-tauri/src/app_state/*_tests.rs` and `src-tauri/src/show_file.rs` tests: cover mapping, dirty state, command path, and JSON output.
- Modify `ui/src/types.ts`, `ui/src/App.tsx`, `ui/src/components/SceneTab.tsx`, `ui/src/components/DurationInput.tsx`, `ui/src/format.ts`: add UI state, command wiring, toggle control, and immediate duration labels.
- Modify `PROJECT.md`, `PHASES.md`, `docs/architecture.md`, `IDEAS.md`: update behavior docs and mark the idea complete.

---

### Task 1: Core Show Model And Mutator

**Files:**
- Modify: `src/show/types.rs`
- Modify: `src/show/capture.rs`
- Modify: `src/show/commands.rs`
- Modify: `src/show/actor.rs`
- Modify: `src/show/handle.rs`
- Test: `src/show/state.rs`

- [ ] **Step 1: Write failing core show-state tests**

Add tests to `src/show/state.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn store_scene_config_defaults_fader_scope_enabled() {
    let mut state = ShowState::default();
    let changed = state
        .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
        .unwrap();

    assert!(changed);
    assert!(state.scene_configs[0].scope_toggles.faders);
}

#[test]
fn store_scene_config_preserves_fader_scope_toggle() {
    let mut state = ShowState::default();
    state
        .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
        .unwrap();
    assert!(state
        .set_scene_scope_faders_enabled("1::scene-1", false)
        .unwrap());

    state
        .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -3.0)])
        .unwrap();

    assert!(!state.scene_configs[0].scope_toggles.faders);
}

#[test]
fn scene_scope_fader_toggle_mutation_reports_noop() {
    let mut state = ShowState::default();
    state
        .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
        .unwrap();

    assert!(state
        .set_scene_scope_faders_enabled("1::scene-1", false)
        .unwrap());
    assert!(!state
        .set_scene_scope_faders_enabled("1::scene-1", false)
        .unwrap());
    assert!(!state.scene_configs[0].scope_toggles.faders);
}

#[test]
fn scene_scope_fader_toggle_requires_existing_scene_config() {
    let mut state = ShowState::default();

    let err = state
        .set_scene_scope_faders_enabled("missing", false)
        .unwrap_err();

    assert_eq!(err, "Scene config not found");
}

#[test]
fn scene_duration_allows_zero_for_immediate_movement() {
    let mut state = ShowState::default();
    state
        .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
        .unwrap();

    assert!(state.set_scene_duration_ms("1::scene-1", 0).unwrap());
    assert_eq!(state.scene_configs[0].duration_ms, 0);
}
```

If `channel(...)` is not already available in that test module, add this helper in the same module:

```rust
fn channel(group: i32, channel: i32, name: &str, gain_db: f64) -> ChannelInfo {
    ChannelInfo {
        group,
        channel,
        name: name.to_string(),
        gain_db,
        muted: false,
    }
}
```

- [ ] **Step 2: Run failing core show tests**

Run: `cargo test -p advanced-show-control show::state`

Expected: FAIL because `scope_toggles` and `set_scene_scope_faders_enabled` do not exist, and duration `0` is rejected.

- [ ] **Step 3: Add core types**

In `src/show/types.rs`, add the toggle type and field:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneScopeToggles {
    pub faders: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self { faders: true }
    }
}
```

Update `SceneConfig`:

```rust
pub struct SceneConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub scope_toggles: SceneScopeToggles,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
}
```

- [ ] **Step 4: Add core mutator and zero-duration validation**

In `src/show/capture.rs`, import `SceneScopeToggles`, preserve the existing toggle on store, and allow duration `0`:

```rust
use super::types::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};
```

Before constructing `snapshot`, add:

```rust
let existing = self.get_scene_config(scene_id);
let scope_toggles = existing
    .as_ref()
    .map(|scene| scene.scope_toggles.clone())
    .unwrap_or_default();
```

Use `existing.as_ref()` for duration preservation and set the new field:

```rust
duration_ms: existing
    .as_ref()
    .map(|scene| scene.duration_ms)
    .unwrap_or(1_000),
scope_toggles,
```

Change validation in `set_scene_duration_ms`:

```rust
if duration_ms != 0 && !(100..=120_000).contains(&duration_ms) {
    return Err("Fade duration must be 0 or between 100 ms and 120000 ms".to_string());
}
```

Add the mutator:

```rust
pub fn set_scene_scope_faders_enabled(
    &mut self,
    scene_id: &str,
    enabled: bool,
) -> Result<bool, String> {
    let scene = self
        .get_scene_config_mut(scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;
    if scene.scope_toggles.faders == enabled {
        Ok(false)
    } else {
        scene.scope_toggles.faders = enabled;
        Ok(true)
    }
}
```

- [ ] **Step 5: Route the core command**

In `src/show/commands.rs`, add:

```rust
SetSceneScopeFadersEnabled {
    scene_id: String,
    enabled: bool,
    reply: oneshot::Sender<Result<bool, String>>,
},
```

In `src/show/handle.rs`, add:

```rust
pub async fn set_scene_scope_faders_enabled(
    &self,
    scene_id: String,
    enabled: bool,
) -> Result<Result<bool, String>, ShowActorError> {
    let (reply, reply_rx) = oneshot::channel();
    self.request(
        ShowCommand::SetSceneScopeFadersEnabled {
            scene_id,
            enabled,
            reply,
        },
        reply_rx,
    )
    .await
}
```

In `src/show/actor.rs`, add a match arm that publishes `SceneConfigChanged` only for `Ok(true)`:

```rust
ShowCommand::SetSceneScopeFadersEnabled {
    scene_id,
    enabled,
    reply,
} => {
    let result = state.set_scene_scope_faders_enabled(&scene_id, enabled);
    if matches!(result, Ok(true)) {
        event_bus.publish(AppEvent::Show(ShowEvent::SceneConfigChanged { scene_id }));
    }
    let _ = reply.send(result);
}
```

- [ ] **Step 6: Update existing `SceneConfig` test fixtures**

Every direct `SceneConfig { ... }` literal in `src/show/state.rs` and other core tests must include:

```rust
scope_toggles: SceneScopeToggles::default(),
```

Add `SceneScopeToggles` to imports where needed.

- [ ] **Step 7: Run core show tests**

Run: `cargo test -p advanced-show-control show`

Expected: PASS.

- [ ] **Step 8: Commit core show model**

Run:

```bash
git add src/show/types.rs src/show/capture.rs src/show/commands.rs src/show/actor.rs src/show/handle.rs src/show/state.rs
git commit -m "feat: add scene fader scope toggle"
```

---

### Task 2: Recall Policy Behavior

**Files:**
- Modify: `src/scene_recall/policy.rs`

- [ ] **Step 1: Write failing recall policy tests**

In `src/scene_recall/policy.rs`, update the test helper `config` to include `scope_toggles: SceneScopeToggles::default()`, then add:

```rust
#[test]
fn skips_when_fader_scope_is_disabled() {
    let mut scene_config = config(1000, Some(-12.5));
    scene_config.scope_toggles.faders = false;

    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: SceneState {
            index: 1,
            name: "Intro".to_string(),
        },
        lv1_snapshot: snapshot(
            Some(SceneState {
                index: 1,
                name: "Intro".to_string(),
            }),
            vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Ch 2".to_string(),
                gain_db: 0.0,
                muted: false,
            }],
        ),
        lockout: false,
        scene_config: Some(scene_config),
    });

    assert!(matches!(
        decision,
        RecallPolicyDecision::Skip { reason } if reason == "fader scope is disabled"
    ));
}

#[test]
fn starts_enabled_zero_duration_scene_as_immediate_move() {
    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: SceneState {
            index: 1,
            name: "Intro".to_string(),
        },
        lv1_snapshot: snapshot(
            Some(SceneState {
                index: 1,
                name: "Intro".to_string(),
            }),
            vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Ch 2".to_string(),
                gain_db: 0.0,
                muted: false,
            }],
        ),
        lockout: false,
        scene_config: Some(config(0, Some(-12.5))),
    });

    assert!(matches!(
        decision,
        RecallPolicyDecision::Start(config) if config.duration_ms == 0 && config.targets.len() == 1
    ));
}
```

- [ ] **Step 2: Run failing recall tests**

Run: `cargo test -p advanced-show-control scene_recall::policy`

Expected: FAIL because disabled fader scope is ignored and zero duration is skipped.

- [ ] **Step 3: Implement recall policy changes**

In `decide_scene_recall`, replace the current duration-zero skip with a fader-scope check after `scene_config` is unwrapped:

```rust
if !config.scope_toggles.faders {
    return skipped("fader scope is disabled");
}
```

Remove this old block:

```rust
if config.duration_ms == 0 {
    return skipped("duration is 0");
}
```

- [ ] **Step 4: Run recall tests**

Run: `cargo test -p advanced-show-control scene_recall::policy`

Expected: PASS.

- [ ] **Step 5: Commit recall behavior**

Run:

```bash
git add src/scene_recall/policy.rs
git commit -m "fix: allow zero-duration scene recalls"
```

---

### Task 3: Show File And Tauri Command Path

**Files:**
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/app_state/show_file_mapping_tests.rs`
- Test: `src-tauri/src/app_state/capture_tests.rs`
- Test: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing Tauri storage and shell tests**

In `src-tauri/src/app_state/show_file_mapping_tests.rs`, add a test that constructs a `ShowFileSceneConfig` with `scope_toggles` missing by parsing JSON:

```rust
#[test]
fn old_show_file_scene_configs_default_fader_scope_enabled() {
    let json = r#"
    {
      "schemaVersion": 1,
      "appVersion": "0.1.0",
      "savedAt": "2026-06-09T00:00:00Z",
      "safety": { "lockout": false },
      "sceneConfigs": [{
        "sceneIndex": 1,
        "sceneName": "Intro",
        "durationMs": 0,
        "channelConfigs": [],
        "scopedChannels": []
      }]
    }
    "#;

    let file: crate::show_file::ShowFile = serde_json::from_str(json).unwrap();

    assert!(file.scene_configs[0].scope_toggles.faders);
}
```

In `src-tauri/src/app_state/capture_tests.rs`, add:

```rust
#[tokio::test]
async fn set_scene_scope_faders_enabled_updates_toggle_and_marks_dirty() {
    let state = test_shell_state_with_show_file().await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let snapshot = state
        .set_scene_scope_faders_enabled("1::Intro".to_string(), false)
        .await
        .unwrap();

    assert!(!snapshot.scene_configs[0].scope_toggles.faders);
    assert!(snapshot.show_file_dirty);
}
```

Use the existing test helper names in that file; if the shell-state constructor has a different name, use the existing helper that creates a loaded show file and LV1 snapshot.

- [ ] **Step 2: Run failing Tauri tests**

Run: `cargo test -p advanced-show-control-tauri app_state::show_file_mapping_tests app_state::capture_tests`

Expected: FAIL because show-file DTOs and shell method do not expose scope toggles yet.

- [ ] **Step 3: Add show-file DTO with serde default**

In `src-tauri/src/show_file.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneScopeToggles {
    pub faders: bool,
}

impl Default for ShowFileSceneScopeToggles {
    fn default() -> Self {
        Self { faders: true }
    }
}
```

Add this field to `ShowFileSceneConfig`:

```rust
#[serde(default)]
pub scope_toggles: ShowFileSceneScopeToggles,
```

- [ ] **Step 4: Map scope toggles in show-file import/export**

In `src-tauri/src/app_state/show_file_mapping.rs`, import `ShowFileSceneScopeToggles`. In `export_show_file_for_save`, set:

```rust
scope_toggles: ShowFileSceneScopeToggles {
    faders: config.scope_toggles.faders,
},
```

In `load_show_file_from_dto`, set:

```rust
scope_toggles: advanced_show_control::show::types::SceneScopeToggles {
    faders: config.scope_toggles.faders,
},
```

- [ ] **Step 5: Add Tauri shell method and command**

In `src-tauri/src/app_state/shell.rs`, add near `set_scene_duration_ms`:

```rust
pub async fn set_scene_scope_faders_enabled(
    &self,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let _ = self
        .show
        .set_scene_scope_faders_enabled(scene_id, enabled)
        .await
        .map_err(|err| format!("{err:?}"))?;
    Ok(self.snapshot().await)
}
```

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state
        .set_scene_scope_faders_enabled(scene_id, enabled)
        .await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}
```

In `src-tauri/src/main.rs`, register `commands::set_scene_scope_faders_enabled` in the `generate_handler!` list.

- [ ] **Step 6: Update direct DTO/test literals**

Every direct `ShowFileSceneConfig { ... }` literal in `src-tauri/src/show_file.rs` and `src-tauri/src/app_state/show_file_mapping_tests.rs` must include:

```rust
scope_toggles: ShowFileSceneScopeToggles::default(),
```

Every direct core `SceneConfig { ... }` literal in Tauri tests must include:

```rust
scope_toggles: advanced_show_control::show::types::SceneScopeToggles::default(),
```

- [ ] **Step 7: Run Tauri tests**

Run: `cargo test -p advanced-show-control-tauri app_state::show_file_mapping_tests app_state::capture_tests commands::tests`

Expected: PASS.

- [ ] **Step 8: Commit Tauri storage and command path**

Run:

```bash
git add src-tauri/src/show_file.rs src-tauri/src/app_state/show_file_mapping.rs src-tauri/src/app_state/show_file_mapping_tests.rs src-tauri/src/app_state/capture_tests.rs src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: persist scene scope toggles"
```

---

### Task 4: Frontend Toggle And Immediate Duration Labels

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/format.ts`
- Modify: `ui/src/components/SceneTab.tsx`
- Modify: `ui/src/components/DurationInput.tsx`

- [ ] **Step 1: Update TypeScript types**

In `ui/src/types.ts`, add:

```ts
export type SceneScopeToggles = {
  faders: boolean;
};
```

Add to `SceneConfig`:

```ts
scopeToggles: SceneScopeToggles;
```

- [ ] **Step 2: Add duration display helper**

In `ui/src/format.ts`, add:

```ts
export function formatSceneDurationSummary(durationMs: number) {
  return durationMs === 0 ? "Immediate" : `${durationMs} ms`;
}
```

Keep `formatDurationSeconds` unchanged unless typecheck shows it needs adjustment.

- [ ] **Step 3: Wire Tauri command in App**

In `ui/src/App.tsx`, add a prop passed to `SceneTab`:

```tsx
setSceneScopeFadersEnabled={(sceneId: string, enabled: boolean) =>
  runSnapshotCommand("set_scene_scope_faders_enabled", { sceneId, enabled }, setAppState, setCommandError)
}
```

- [ ] **Step 4: Add SceneTab prop and summary labels**

In `ui/src/components/SceneTab.tsx`, update imports:

```ts
import { formatSceneDurationSummary } from "../format";
```

Add prop:

```ts
setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => void;
```

Replace the scene list summary with:

```tsx
{formatSceneDurationSummary(scene.durationMs)} · FADERS {scene.scopeToggles.faders ? "on" : "off"} · {scene.scopedChannels.length}/{scene.channelConfigs.length} scoped
```

- [ ] **Step 5: Add FADERS toggle control**

In `ui/src/components/SceneTab.tsx`, add this between `DurationInput` and `ScopeGrid`:

```tsx
<div className="mt-4 rounded-lg border border-slate-800 p-4">
  <div className="flex flex-wrap items-center justify-between gap-3">
    <div>
      <h3 className="font-semibold text-slate-100">Scene Scope</h3>
      <p className="mt-1 text-sm text-slate-400">
        FADERS controls whether scoped faders move when this LV1 scene is recalled.
      </p>
    </div>
    <button
      className={
        selected.scopeToggles.faders
          ? "rounded bg-cyan-700 px-4 py-2 text-sm font-bold text-white hover:bg-cyan-600"
          : "rounded bg-slate-800 px-4 py-2 text-sm font-bold text-slate-300 hover:bg-slate-700"
      }
      onClick={() => props.setSceneScopeFadersEnabled(selected.sceneId, !selected.scopeToggles.faders)}
    >
      FADERS {selected.scopeToggles.faders ? "ON" : "OFF"}
    </button>
  </div>
</div>
```

- [ ] **Step 6: Update duration input helper text**

In `ui/src/components/DurationInput.tsx`, change the helper text to make zero immediate:

```tsx
<span className="text-xs text-slate-500">Use 0 for an immediate move. Values above 0 are clamped from 0.1 to 120 seconds.</span>
```

- [ ] **Step 7: Run frontend verification**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 8: Commit frontend changes**

Run:

```bash
git add ui/src/types.ts ui/src/App.tsx ui/src/format.ts ui/src/components/SceneTab.tsx ui/src/components/DurationInput.tsx
git commit -m "feat: add faders scene scope toggle UI"
```

---

### Task 5: Docs And Full Verification

**Files:**
- Modify: `PROJECT.md`
- Modify: `PHASES.md`
- Modify: `docs/architecture.md`
- Modify: `IDEAS.md`

- [ ] **Step 1: Update behavior docs**

In `PROJECT.md`, update the data-model example to include:

```ts
scopeToggles: {
  faders: boolean;
};
```

Also add one sentence near the recall workflow: `A duration of 0 means enabled faders move to their stored targets immediately; disabling fader movement is controlled by the scene-level FADERS toggle.`

- [ ] **Step 2: Update phase and architecture docs**

In `PHASES.md`, replace the Phase 7 line that says duration `0` scenes are skipped with wording that says disabled fader scope is skipped and duration `0` scenes move immediately.

In `docs/architecture.md`, update lines describing `SceneRecallFader` so they no longer say it treats duration `0` scenes as disabled. Use: `It skips scenes whose fader scope toggle is disabled and starts validated fader moves even when duration is 0.`

- [ ] **Step 3: Mark idea complete**

In `IDEAS.md`, change the scene scope enablement item from `- [ ]` to `- [x]`.

- [ ] **Step 4: Run formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS. If it fails, run `cargo fmt --all`, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 5: Run Rust verification**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 6: Run frontend verification**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 7: Commit docs and final fixes**

Run:

```bash
git add PROJECT.md PHASES.md docs/architecture.md IDEAS.md
git commit -m "docs: document scene scope toggles"
```

If verification formatting touched source files from earlier tasks, include those intended files in the same commit only if they are formatting-only changes caused by this implementation.

---

## Self-Review

- Spec coverage: covered data model, recall behavior, UI/commands, storage compatibility, tests, and docs.
- Placeholder scan: no `TBD`, `TODO`, or open-ended implementation steps remain.
- Type consistency: Rust uses `SceneScopeToggles` / `scope_toggles`; TypeScript and JSON use `SceneScopeToggles` / `scopeToggles`; command name is consistently `set_scene_scope_faders_enabled`.
