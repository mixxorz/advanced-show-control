# Scene Store Scope Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Listen Mode target capture with a Store workflow, broad scene/channel config data model, and grouped scope toggle grid.

**Architecture:** Rust `ShellState` remains the source of truth and exports `AppViewState` to React. Scene configs store a full current channel snapshot via `channel_configs`, while `scoped_channels` is scene-level scope. React renders scene selection plus a Store button and grouped toggle grid backed by Tauri snapshot commands.

**Tech Stack:** Rust/Tauri backend, Tokio tests, React + TypeScript + Vite frontend, serde JSON show files.

---

## File Structure

- Modify `src-tauri/src/app_state/view.rs`: rename view DTOs to `SceneConfig`, `ChannelConfig`, `ChannelRef`; remove `FadeTarget`, `fade_enabled`, and `listen_mode_active` from exported state.
- Modify `src-tauri/src/app_state/shell.rs`: rename internal `scene_fade_configs` to `scene_configs`; remove `listen_mode_active` and unknown fader warning state; map live channels to view state.
- Modify `src-tauri/src/app_state/capture.rs`: turn this into scene config editing/store/scope commands or rename later if desired; remove Listen Mode and target commands.
- Modify `src-tauri/src/app_state/events.rs`: keep live LV1 mirror updates, remove fader target recording.
- Modify `src-tauri/src/app_state/show_file_mapping.rs`: serialize and load the new `sceneConfigs`, `channelConfigs`, and `scopedChannels` shape.
- Modify `src-tauri/src/show_file.rs`: replace show file DTOs and validation with scene-only validation.
- Modify `src-tauri/src/commands.rs` and `src-tauri/src/main.rs`: remove old commands and register `store_scene_config`, `set_channel_scoped`, and `set_all_channels_scoped`.
- Modify tests under `src-tauri/src/app_state/*_tests.rs` and `src-tauri/src/show_file.rs`: replace capture/listen expectations with store/scope behavior.
- Modify `ui/src/types.ts`: mirror the new DTO shape and remove Listen Mode state.
- Modify `ui/src/App.tsx`: wire new commands into `SceneTab`.
- Modify `ui/src/components/SceneTab.tsx`: replace Listen Mode/table UI with Store button and grouped scope toggle grid.
- Modify `ui/src/format.ts`: add display helpers for group labels/master labels if kept frontend-side.
- Update `PHASES.md` or related docs if implementation changes phase status/copy.

---

### Task 1: Rename The Rust View Model

**Files:**
- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Test: `src-tauri/src/app_state/shell.rs`

- [ ] **Step 1: Write the failing snapshot test**

In `src-tauri/src/app_state/shell.rs`, update `snapshot_maps_lv1_scene_and_counts` to assert the new fields and absence of Listen Mode in the constructed snapshot:

```rust
assert_eq!(snapshot.scene_configs.len(), 0);
assert_eq!(snapshot.selected_scene_id, None);
```

Also update `default_snapshot_exposes_untitled_show_and_is_not_dirty`:

```rust
assert!(snapshot.scene_configs.is_empty());
assert_eq!(snapshot.selected_scene_id, None);
```

Do not reference `scene_fade_configs` or `listen_mode_active` in these tests.

- [ ] **Step 2: Run Rust tests to verify compile failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::shell::tests`

Expected: FAIL because `AppViewState.scene_configs` does not exist yet and old field names still exist.

- [ ] **Step 3: Implement the new view DTOs**

In `src-tauri/src/app_state/view.rs`, replace `FadeTarget` and `SceneFadeConfig` with:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
}
```

In `AppViewState`, replace:

```rust
pub scene_fade_configs: Vec<SceneFadeConfig>,
pub listen_mode_active: bool,
```

with:

```rust
pub scene_configs: Vec<SceneConfig>,
```

In `src-tauri/src/app_state/shell.rs`, update imports and `ShellInner`:

```rust
pub(super) scene_configs: Vec<SceneConfig>,
```

Remove `listen_mode_active` and `unknown_fader_warnings` from `ShellInner`.

In `snapshot_from_inner`, set:

```rust
scene_configs: inner.scene_configs.clone(),
selected_scene_id: inner.selected_scene_id.clone(),
```

and remove `listen_mode_active`.

- [ ] **Step 4: Run focused tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::shell::tests`

Expected: PASS after fixing compile errors in the touched module.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app_state/view.rs src-tauri/src/app_state/shell.rs
git commit -m "refactor: rename scene config view model"
```

---

### Task 2: Reconcile Scenes Into New Configs

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/events_tests.rs`
- Modify: `src-tauri/src/app_state/test_support.rs`

- [ ] **Step 1: Update reconciliation tests for `SceneConfig` defaults**

In `src-tauri/src/app_state/events_tests.rs`, replace expectations that mention `scene_fade_configs`, `fade_enabled`, or `fade_targets` with:

```rust
let config = &snapshot.scene_configs[0];
assert_eq!(config.scene_id, "1::Intro");
assert_eq!(config.scene_index, 1);
assert_eq!(config.scene_name, "Intro");
assert_eq!(config.duration_ms, 0);
assert!(config.channel_configs.is_empty());
assert!(config.scoped_channels.is_empty());
```

- [ ] **Step 2: Run reconciliation tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::events_tests`

Expected: FAIL because reconciliation still builds old `SceneFadeConfig` defaults.

- [ ] **Step 3: Implement reconciliation defaults**

In `src-tauri/src/app_state/events.rs`, update imports to use `SceneConfig` only. Wherever a scene config is created during scene-list reconciliation, create:

```rust
SceneConfig {
    scene_id: scene_id(scene.index, &scene.name),
    scene_index: scene.index,
    scene_name: scene.name.clone(),
    duration_ms: 0,
    channel_configs: Vec::new(),
    scoped_channels: Vec::new(),
}
```

Remove calls to `record_fader_target` from fader event handling. Delete `record_fader_target` and any unknown fader warning logic.

- [ ] **Step 4: Run reconciliation tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::events_tests`

Expected: PASS after updating assertions and removed warning tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app_state/events.rs src-tauri/src/app_state/events_tests.rs src-tauri/src/app_state/test_support.rs
git commit -m "refactor: reconcile scenes into scene configs"
```

---

### Task 3: Add Store And Scope State Commands

**Files:**
- Modify: `src-tauri/src/app_state/capture.rs`
- Modify: `src-tauri/src/app_state/capture_tests.rs`

- [ ] **Step 1: Replace capture tests with store/scope tests**

In `src-tauri/src/app_state/capture_tests.rs`, remove Listen Mode tests and add these tests:

```rust
#[tokio::test]
async fn store_scene_config_snapshots_all_current_channels_and_scopes_first_store() {
    let state = ShellState::default();
    state.begin_connection(connected_state_with_scene_and_channel()).await;

    let snapshot = state.store_scene_config("1::Intro".to_string()).await.unwrap();

    let config = &snapshot.scene_configs[0];
    assert_eq!(config.channel_configs.len(), 1);
    assert_eq!(config.channel_configs[0].group, 0);
    assert_eq!(config.channel_configs[0].channel, 2);
    assert_eq!(config.channel_configs[0].fader_db, Some(-8.0));
    assert_eq!(config.scoped_channels.len(), 1);
    assert_eq!(config.scoped_channels[0].group, 0);
    assert_eq!(config.scoped_channels[0].channel, 2);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn store_scene_config_preserves_existing_scope_on_later_store() {
    let state = ShellState::default();
    state.begin_connection(connected_state_with_scene_and_channel()).await;
    state.store_scene_config("1::Intro".to_string()).await.unwrap();
    state.set_channel_scoped("1::Intro".to_string(), 0, 2, false).await.unwrap();

    let snapshot = state.store_scene_config("1::Intro".to_string()).await.unwrap();

    assert!(snapshot.scene_configs[0].scoped_channels.is_empty());
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].fader_db, Some(-8.0));
}

#[tokio::test]
async fn set_all_channels_scoped_sets_and_clears_scope() {
    let state = ShellState::default();
    state.begin_connection(connected_state_with_scene_and_channel()).await;
    state.store_scene_config("1::Intro".to_string()).await.unwrap();

    let none = state.set_all_channels_scoped("1::Intro".to_string(), false).await.unwrap();
    assert!(none.scene_configs[0].scoped_channels.is_empty());

    let all = state.set_all_channels_scoped("1::Intro".to_string(), true).await.unwrap();
    assert_eq!(all.scene_configs[0].scoped_channels.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::capture_tests`

Expected: FAIL because store/scope methods do not exist.

- [ ] **Step 3: Implement store and scope methods**

In `src-tauri/src/app_state/capture.rs`, keep `select_scene_config` and `set_scene_duration_ms`, remove `set_scene_fade_enabled`, `set_listen_mode`, `set_fade_target_enabled`, `remove_fade_target`, and `find_target_mut`.

Add imports:

```rust
use std::collections::HashSet;
use super::view::{ChannelConfig, ChannelRef};
```

Add methods:

```rust
pub async fn store_scene_config(&self, scene_id: String) -> Result<super::view::AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let channels = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| snapshot.channels.clone())
        .filter(|channels| !channels.is_empty())
        .ok_or_else(|| "LV1 channel list is empty".to_string())?;

    let config = inner
        .scene_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;

    let first_store = config.channel_configs.is_empty();
    let new_refs = channels
        .iter()
        .map(|channel| ChannelRef { group: channel.group, channel: channel.channel })
        .collect::<Vec<_>>();
    let new_ref_set = new_refs
        .iter()
        .map(|channel| (channel.group, channel.channel))
        .collect::<HashSet<_>>();

    config.channel_configs = channels
        .iter()
        .map(|channel| ChannelConfig {
            group: channel.group,
            channel: channel.channel,
            fader_db: Some(channel.gain_db),
        })
        .collect();

    if first_store {
        config.scoped_channels = new_refs;
    } else {
        config
            .scoped_channels
            .retain(|channel| new_ref_set.contains(&(channel.group, channel.channel)));
    }

    inner.show_file_dirty = true;
    Ok(snapshot_from_inner(&inner))
}

pub async fn set_channel_scoped(
    &self,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<super::view::AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let config = inner
        .scene_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;

    if !config.channel_configs.iter().any(|entry| entry.group == group && entry.channel == channel) {
        return Err("Channel config not found".to_string());
    }

    config.scoped_channels.retain(|entry| !(entry.group == group && entry.channel == channel));
    if scoped {
        config.scoped_channels.push(ChannelRef { group, channel });
    }

    inner.show_file_dirty = true;
    Ok(snapshot_from_inner(&inner))
}

pub async fn set_all_channels_scoped(
    &self,
    scene_id: String,
    scoped: bool,
) -> Result<super::view::AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let config = inner
        .scene_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;

    config.scoped_channels = if scoped {
        config
            .channel_configs
            .iter()
            .map(|entry| ChannelRef { group: entry.group, channel: entry.channel })
            .collect()
    } else {
        Vec::new()
    };

    inner.show_file_dirty = true;
    Ok(snapshot_from_inner(&inner))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::capture_tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app_state/capture.rs src-tauri/src/app_state/capture_tests.rs
git commit -m "feat: add scene store and scope commands"
```

---

### Task 4: Update Show File DTOs And Validation

**Files:**
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping_tests.rs`

- [ ] **Step 1: Write show-file mapping tests for new JSON shape**

In `src-tauri/src/app_state/show_file_mapping_tests.rs`, update `export_show_file_contains_current_configs` to:

```rust
let state = ShellState::default();
state.begin_connection(connected_state_with_scene_and_channel()).await;
state.store_scene_config("1::Intro".to_string()).await.unwrap();

let file = state.export_show_file("saved".to_string()).await;

assert_eq!(file.schema_version, 1);
assert_eq!(file.saved_at, "saved");
assert_eq!(file.scene_configs[0].scene_index, 1);
assert_eq!(file.scene_configs[0].duration_ms, 0);
assert_eq!(file.scene_configs[0].channel_configs[0].fader_db, Some(-8.0));
assert_eq!(file.scene_configs[0].scoped_channels.len(), 1);
```

Add a validation test in `src-tauri/src/show_file.rs` proving channels are not pruned:

```rust
#[test]
fn validate_show_file_does_not_remove_channel_configs_or_scope() {
    let lv1 = lv1_snapshot();
    let mut file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: ShowFileSafety { lockout: false },
        scene_configs: vec![ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 0,
            channel_configs: vec![ShowFileChannelConfig { group: 99, channel: 99, fader_db: Some(-9.0) }],
            scoped_channels: vec![ShowFileChannelRef { group: 99, channel: 99 }],
        }],
    };

    let report = validate_show_file(&mut file, &lv1).unwrap();

    assert!(!report.removed_anything());
    assert_eq!(file.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(file.scene_configs[0].scoped_channels.len(), 1);
}
```

- [ ] **Step 2: Run show-file tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file app_state::show_file_mapping_tests`

Expected: FAIL because DTOs still use `scene_fade_configs` and `fade_targets`.

- [ ] **Step 3: Implement DTOs and mapping**

In `src-tauri/src/show_file.rs`, replace `ShowFileSceneFadeConfig` and `ShowFileFadeTarget` with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneConfig {
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ShowFileChannelConfig>,
    pub scoped_channels: Vec<ShowFileChannelRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelRef {
    pub group: i32,
    pub channel: i32,
}
```

Change `ShowFile` field to:

```rust
pub scene_configs: Vec<ShowFileSceneConfig>,
```

In `validate_show_file`, remove channel validation and only retain scene configs by exact `scene_index` and `scene_name`. Keep `removed_scenes`; remove or stop populating `removed_targets`.

In `src-tauri/src/app_state/show_file_mapping.rs`, map between `SceneConfig`/`ChannelConfig`/`ChannelRef` and show-file DTOs using `scene_configs`, `channel_configs`, and `scoped_channels`.

- [ ] **Step 4: Run show-file tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file app_state::show_file_mapping_tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/show_file.rs src-tauri/src/app_state/show_file_mapping.rs src-tauri/src/app_state/show_file_mapping_tests.rs
git commit -m "refactor: store scene configs in show files"
```

---

### Task 5: Replace Tauri Commands And Registration

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Search: `ui/src/App.tsx`

- [ ] **Step 1: Run a compile check to surface old command references**

Run: `cargo test -p lv1-scene-fade-utility-tauri --no-run`

Expected: FAIL while old commands are still registered or referenced.

- [ ] **Step 2: Replace command functions**

In `src-tauri/src/commands.rs`, remove `set_scene_fade_enabled`, `set_listen_mode`, `set_fade_target_enabled`, and `remove_fade_target` command functions.

Add:

```rust
#[tauri::command]
pub async fn store_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = state.store_scene_config(scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_channel_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_channel_scoped(scene_id, group, channel, scoped).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    scoped: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_all_channels_scoped(scene_id, scoped).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}
```

In `src-tauri/src/main.rs`, remove old command registrations and register the three new commands.

- [ ] **Step 3: Run compile check**

Run: `cargo test -p lv1-scene-fade-utility-tauri --no-run`

Expected: PASS for command registration.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: expose scene store scope commands"
```

---

### Task 6: Update Frontend Types And Command Wiring

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Update TypeScript types**

In `ui/src/types.ts`, replace `FadeTarget` and `SceneFadeConfig` with:

```ts
export type ChannelRef = {
  group: number;
  channel: number;
};

export type ChannelConfig = {
  group: number;
  channel: number;
  faderDb: number | null;
};

export type SceneConfig = {
  sceneId: string;
  sceneIndex: number;
  sceneName: string;
  durationMs: number;
  channelConfigs: ChannelConfig[];
  scopedChannels: ChannelRef[];
};
```

In `AppViewState`, replace:

```ts
sceneFadeConfigs: SceneFadeConfig[];
listenModeActive: boolean;
```

with:

```ts
sceneConfigs: SceneConfig[];
```

Update `disconnectedAppViewState` to use `sceneConfigs: []` and remove `listenModeActive`.

- [ ] **Step 2: Update `App.tsx` command props**

In `ui/src/App.tsx`, replace old SceneTab props with:

```tsx
<SceneTab
  appState={appState}
  selectScene={(sceneId) => runSnapshotCommand("select_scene_config", { sceneId }, setAppState, setCommandError)}
  setSceneDurationMs={(sceneId, durationMs) =>
    runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs }, setAppState, setCommandError)
  }
  storeSceneConfig={(sceneId) =>
    runSnapshotCommand("store_scene_config", { sceneId }, setAppState, setCommandError)
  }
  setChannelScoped={(sceneId, group, channel, scoped) =>
    runSnapshotCommand("set_channel_scoped", { sceneId, group, channel, scoped }, setAppState, setCommandError)
  }
  setAllChannelsScoped={(sceneId, scoped) =>
    runSnapshotCommand("set_all_channels_scoped", { sceneId, scoped }, setAppState, setCommandError)
  }
/>
```

- [ ] **Step 3: Run typecheck to verify SceneTab still fails**

Run: `npm run typecheck`

Expected: FAIL because `SceneTab.tsx` still uses old prop and type names.

- [ ] **Step 4: Commit after type definitions and wiring compile with next task**

Do not commit yet if typecheck fails. Continue to Task 7, then commit frontend changes together.

---

### Task 7: Replace SceneTab Table With Grouped Scope Grid

**Files:**
- Modify: `ui/src/components/SceneTab.tsx`
- Modify: `ui/src/format.ts`

- [ ] **Step 1: Add frontend display helpers**

In `ui/src/format.ts`, add:

```ts
export function channelDisplayGroup(group: number) {
  if (group === 0) return "Inputs";
  if (group === 1) return "Groups";
  if (group === 2) return "Aux";
  if (group === 6) return "Matrix";
  if ([3, 4, 5, 7, 8].includes(group)) return "Masters";
  return "Unknown";
}

export function channelDisplayGroupOrder(groupName: string) {
  return ["Inputs", "Groups", "Aux", "Matrix", "Masters", "Unknown"].indexOf(groupName);
}

export function channelButtonLabel(group: number, channel: number) {
  if (group === 3) return "LR";
  if (group === 4) return "C";
  if (group === 5) return "Mono";
  if (group === 7) return "Cue";
  if (group === 8) return "TB";
  return String(channel);
}
```

- [ ] **Step 2: Replace `SceneTab.tsx` component contract and rendering**

Update imports:

```ts
import type { AppViewState, ChannelConfig, SceneConfig } from "../types";
import { channelButtonLabel, channelDisplayGroup, channelDisplayGroupOrder, channelName, formatDb } from "../format";
import { DurationInput } from "./DurationInput";
```

Update props:

```ts
export function SceneTab(props: {
  appState: AppViewState;
  selectScene: (sceneId: string) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  storeSceneConfig: (sceneId: string) => void;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const selected = props.appState.sceneConfigs.find((scene) => scene.sceneId === props.appState.selectedSceneId);
```

Use `props.appState.sceneConfigs` in the scene list. Replace status text with:

```tsx
{scene.durationMs > 0 ? `${scene.durationMs} ms` : "Disabled"} · {scene.scopedChannels.length}/{scene.channelConfigs.length} scoped
```

Remove Listen Mode and Fade Enabled buttons. Add Store button:

```tsx
<button
  className="rounded-lg bg-cyan-700 px-4 py-2 font-bold text-white hover:bg-cyan-600"
  onClick={() => props.storeSceneConfig(selected.sceneId)}
>
  Store
</button>
```

Replace `FadeTargetTable` with `ScopeGrid`:

```tsx
<ScopeGrid
  channels={props.appState.channels}
  scene={selected}
  setAllChannelsScoped={props.setAllChannelsScoped}
  setChannelScoped={props.setChannelScoped}
/>
```

Add helper functions and component:

```tsx
function channelKey(group: number, channel: number) {
  return `${group}:${channel}`;
}

function ScopeGrid(props: {
  channels: AppViewState["channels"];
  scene: SceneConfig;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const scoped = new Set(props.scene.scopedChannels.map((entry) => channelKey(entry.group, entry.channel)));
  const groups = new Map<string, ChannelConfig[]>();

  for (const config of props.scene.channelConfigs) {
    const groupName = channelDisplayGroup(config.group);
    groups.set(groupName, [...(groups.get(groupName) ?? []), config]);
  }

  const grouped = [...groups.entries()].sort(
    ([a], [b]) => channelDisplayGroupOrder(a) - channelDisplayGroupOrder(b),
  );

  if (props.scene.channelConfigs.length === 0) {
    return <p className="mt-5 rounded-lg border border-slate-800 p-4 text-sm text-slate-400">Store the current mixer state to choose scoped channels.</p>;
  }

  return (
    <div className="mt-5 rounded-lg border border-slate-800 p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="font-semibold text-slate-100">Scoped Channels</h3>
        <div className="flex gap-2">
          <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={() => props.setAllChannelsScoped(props.scene.sceneId, true)}>All</button>
          <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={() => props.setAllChannelsScoped(props.scene.sceneId, false)}>None</button>
        </div>
      </div>
      <div className="mt-4 space-y-4">
        {grouped.map(([groupName, configs]) => (
          <section key={groupName}>
            <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-400">{groupName}</h4>
            <div className="mt-2 flex flex-wrap gap-2">
              {configs.sort((a, b) => a.channel - b.channel).map((config) => {
                const key = channelKey(config.group, config.channel);
                const isScoped = scoped.has(key);
                return (
                  <button
                    className={isScoped ? "rounded bg-cyan-700 px-3 py-2 text-sm font-bold text-white" : "rounded bg-slate-800 px-3 py-2 text-sm font-bold text-slate-300 hover:bg-slate-700"}
                    key={key}
                    onClick={() => props.setChannelScoped(props.scene.sceneId, config.group, config.channel, !isScoped)}
                    title={`${channelName(props.channels, config.group, config.channel)} · ${formatDb(config.faderDb ?? 0)}`}
                  >
                    {channelButtonLabel(config.group, config.channel)}
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Run frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 4: Commit frontend type and UI changes**

```bash
git add ui/src/types.ts ui/src/App.tsx ui/src/components/SceneTab.tsx ui/src/format.ts
git commit -m "feat: add scoped channel grid"
```

---

### Task 8: Remove Remaining Legacy References And Verify

**Files:**
- Search all repo files.
- Modify any remaining code/docs that still references removed workflow incorrectly.

- [ ] **Step 1: Search for legacy identifiers**

Run: `rg "listenMode|listen_mode|SceneFadeConfig|FadeTarget|fadeTargets|sceneFadeConfigs|fadeEnabled|set_listen_mode|set_fade_target_enabled|remove_fade_target"`

Expected: Only historical spec/plan docs may mention old terms. No source files should contain these identifiers.

- [ ] **Step 2: Remove or rename source references**

If the search finds source references, update them to the new model. Examples:

```rust
scene_configs
channel_configs
scoped_channels
store_scene_config
set_channel_scoped
set_all_channels_scoped
```

```ts
sceneConfigs
channelConfigs
scopedChannels
storeSceneConfig
setChannelScoped
setAllChannelsScoped
```

- [ ] **Step 3: Run full verification**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 4: Update docs if user-facing phase status changed**

If `PHASES.md` still says Phase 4 uses Listen Mode as the current workflow, update the relevant line to describe Store + scope. Keep this concise:

```md
- [x] **Phase 4: Capture Engine And Listen Mode** — superseded by explicit scene Store and scoped-channel editing; fader movement capture has been removed.
```

- [ ] **Step 5: Commit cleanup**

```bash
git add PHASES.md src-tauri ui
git commit -m "chore: remove listen mode workflow references"
```

---

## Self-Review

Spec coverage:

- Data model: Tasks 1, 2, and 4 define `SceneConfig`, `ChannelConfig`, `ChannelRef`, `channelConfigs`, and `scopedChannels`.
- Store behavior: Task 3 implements first-store all-scope, later-store scope preservation, dirty state, and no Listen Mode dependency.
- Workflow decommissioning: Tasks 2, 3, 5, and 8 remove Listen Mode state, commands, fader-event capture, and old identifiers.
- UI design: Tasks 6 and 7 implement Store, duration, grouped toggle grid, blue/grey scoped state, and All/None controls.
- Save/load: Task 4 implements new show-file shape and scene-only validation.
- Testing: Each implementation task starts with failing tests or compile/typecheck failure and includes verification commands.

Placeholder scan: No `TBD`, `TODO`, or open-ended implementation placeholders remain. Any conditional cleanup in Task 8 includes exact allowed replacements and verification.

Type consistency: The plan consistently uses Rust snake_case fields (`scene_configs`, `channel_configs`, `scoped_channels`) and TypeScript camelCase fields (`sceneConfigs`, `channelConfigs`, `scopedChannels`) through serde camelCase boundaries.
