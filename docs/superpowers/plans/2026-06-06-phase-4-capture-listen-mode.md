# Phase 4 Capture Listen Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add in-memory per-scene fade configs and Listen Mode direct-writing of LV1 fader targets to the existing Tauri shell.

**Architecture:** `ShellState` remains the Rust-owned source of truth. Rust reconciles LV1 scenes into `SceneFadeConfig` records, owns selected scene and Listen Mode, applies LV1 fader events to the selected config, and exposes serializable `AppViewState` to React. React renders a split scene list/detail UI and sends intent through Tauri commands only.

**Tech Stack:** Rust 2024, Tauri 2, tokio, serde, React, TypeScript, Vite, Tailwind CSS 4, npm.

---

## File Map

| File | Status | Responsibility |
|---|---|---|
| `src-tauri/src/app_state.rs` | Modify | Rename app DTO to `AppViewState`; add scene fade config state, reconciliation, Listen Mode, target mutation, tests |
| `src-tauri/src/commands.rs` | Modify | Return `AppViewState`; add commands for scene selection, fade enable, Listen Mode, target enable/remove |
| `src-tauri/src/main.rs` | Modify | Register new Tauri commands |
| `ui/src/types.ts` | Modify | Match Rust `AppViewState`, `SceneFadeConfig`, and `FadeTarget` DTOs |
| `ui/src/App.tsx` | Modify | Rename frontend `snapshot` state to `appState`; render split Scene tab and dispatch new commands |
| `docs/superpowers/specs/2026-06-06-phase-4-capture-listen-mode-design.md` | Reference | Approved behavior and scope |
| `PHASES.md` | Modify at end | Mark Phase 4 complete after verification passes |

---

## Task 1: Rename App DTO And Frontend State

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Rename the Rust DTO type**

In `src-tauri/src/app_state.rs`, rename `AppSnapshot` to `AppViewState` and update all method return types in this file. Keep the function name `snapshot()` for now if desired, but its return type must be `AppViewState`.

The struct should still contain the existing fields at this step:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppViewState {
    pub connection: AppConnectionState,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub scene_count: usize,
    pub channel_count: usize,
    pub fade_state: AppFadeState,
    pub lockout: bool,
    pub logs: Vec<AppLogEntry>,
    pub last_event_at: Option<String>,
}
```

- [ ] **Step 2: Update command return types**

In `src-tauri/src/commands.rs`, change the import and all command signatures from `AppSnapshot` to `AppViewState`:

```rust
use crate::app_state::{AppViewState, ShellState};

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppViewState, String> {
    Ok(state.snapshot().await)
}
```

Also update `emit_snapshot` to accept `&AppViewState`.

- [ ] **Step 3: Update frontend type name**

In `ui/src/types.ts`, rename `AppSnapshot` to `AppViewState` and `disconnectedSnapshot` to `disconnectedAppViewState`:

```ts
export type AppViewState = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  scenes: SceneSummary[];
  sceneCount: number;
  channelCount: number;
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
};

export const disconnectedAppViewState: AppViewState = {
  connection: "disconnected",
  currentScene: null,
  scenes: [],
  sceneCount: 0,
  channelCount: 0,
  fadeState: "idle",
  lockout: false,
  logs: [],
  lastEventAt: null,
};
```

- [ ] **Step 4: Rename React local state**

In `ui/src/App.tsx`, update imports and rename local `snapshot` state to `appState`. For example:

```tsx
import { disconnectedAppViewState, type AppViewState } from "./types";

const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);
```

Update helper signatures to use `AppViewState` and invoke calls such as:

```tsx
const next = await invoke<AppViewState>(command, args);
```

- [ ] **Step 5: Verify rename compiles**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: Rust tests pass.

Run: `npm run typecheck`

Expected: TypeScript typecheck passes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state.rs src-tauri/src/commands.rs ui/src/types.ts ui/src/App.tsx
git commit -m "refactor: clarify app view state naming"
```

---

## Task 2: Add Scene Fade Config Types And Reconciliation

**Files:**
- Modify: `src-tauri/src/app_state.rs`

- [ ] **Step 1: Write failing reconciliation tests**

Add these tests to the existing `#[cfg(test)] mod tests` in `src-tauri/src/app_state.rs`:

```rust
#[test]
fn scene_list_reconciliation_creates_default_configs() {
    let mut inner = ShellInner::default();
    inner.reconcile_scene_fade_configs(&[
        SceneListEntry { index: 1, name: "Intro".to_string() },
        SceneListEntry { index: 2, name: "Verse".to_string() },
    ]);

    assert_eq!(inner.scene_fade_configs.len(), 2);
    assert_eq!(inner.scene_fade_configs[0].scene_id, "1::Intro");
    assert!(!inner.scene_fade_configs[0].fade_enabled);
    assert!(inner.scene_fade_configs[0].fade_targets.is_empty());
    assert_eq!(inner.selected_scene_id.as_deref(), Some("1::Intro"));
}

#[test]
fn scene_list_reconciliation_preserves_matching_config_data() {
    let mut inner = ShellInner::default();
    inner.scene_fade_configs = vec![SceneFadeConfig {
        scene_id: "2::Verse".to_string(),
        scene_index: 2,
        scene_name: "Verse".to_string(),
        fade_enabled: true,
        fade_targets: vec![FadeTarget {
            group: 0,
            channel: 4,
            target_db: -5.5,
            enabled: true,
            updated_at: "123".to_string(),
        }],
    }];
    inner.selected_scene_id = Some("2::Verse".to_string());

    inner.reconcile_scene_fade_configs(&[
        SceneListEntry { index: 2, name: "Verse".to_string() },
        SceneListEntry { index: 3, name: "Chorus".to_string() },
    ]);

    let verse = inner.scene_fade_configs.iter().find(|scene| scene.scene_id == "2::Verse").unwrap();
    assert!(verse.fade_enabled);
    assert_eq!(verse.fade_targets.len(), 1);
    assert_eq!(inner.scene_fade_configs.len(), 2);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
}

#[test]
fn scene_list_reconciliation_turns_off_listen_mode_when_selected_scene_disappears() {
    let mut inner = ShellInner::default();
    inner.scene_fade_configs = vec![SceneFadeConfig {
        scene_id: "1::Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        fade_enabled: false,
        fade_targets: Vec::new(),
    }];
    inner.selected_scene_id = Some("1::Intro".to_string());
    inner.listen_mode_active = true;

    inner.reconcile_scene_fade_configs(&[SceneListEntry { index: 2, name: "Verse".to_string() }]);

    assert!(!inner.listen_mode_active);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
    assert!(inner.logs.iter().any(|entry| entry.message == "Listen Mode stopped because selected scene is no longer available"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_list_reconciliation -- --nocapture`

Expected: FAIL because `SceneFadeConfig`, `FadeTarget`, and reconciliation methods do not exist.

- [ ] **Step 3: Add Rust DTO structs and state fields**

In `src-tauri/src/app_state.rs`, add these serializable structs near `SceneSummary`:

```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFadeConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub fade_enabled: bool,
    pub fade_targets: Vec<FadeTarget>,
}
```

Add these fields to `AppViewState`:

```rust
pub scene_fade_configs: Vec<SceneFadeConfig>,
pub selected_scene_id: Option<String>,
pub listen_mode_active: bool,
```

Add these fields to `ShellInner`:

```rust
scene_fade_configs: Vec<SceneFadeConfig>,
selected_scene_id: Option<String>,
listen_mode_active: bool,
```

- [ ] **Step 4: Implement reconciliation helpers**

Add helper functions and methods in `src-tauri/src/app_state.rs`:

```rust
fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}

impl ShellInner {
    fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) {
        let mut next = Vec::with_capacity(scenes.len());

        for scene in scenes {
            let id = scene_id(scene.index, &scene.name);
            if let Some(mut existing) = self
                .scene_fade_configs
                .iter()
                .find(|config| config.scene_id == id)
                .cloned()
            {
                existing.scene_index = scene.index;
                existing.scene_name = scene.name.clone();
                next.push(existing);
            } else {
                next.push(SceneFadeConfig {
                    scene_id: id,
                    scene_index: scene.index,
                    scene_name: scene.name.clone(),
                    fade_enabled: false,
                    fade_targets: Vec::new(),
                });
            }
        }

        let selected_still_exists = self
            .selected_scene_id
            .as_ref()
            .is_some_and(|selected| next.iter().any(|config| &config.scene_id == selected));

        if !selected_still_exists {
            if self.listen_mode_active {
                self.listen_mode_active = false;
                self.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    "Listen Mode stopped because selected scene is no longer available".to_string(),
                );
            }
            self.selected_scene_id = next.first().map(|config| config.scene_id.clone());
        }

        self.scene_fade_configs = next;
    }
}
```

- [ ] **Step 5: Call reconciliation from LV1 state updates**

In `begin_connection`, after `inner.lv1_snapshot = Some(snapshot);`, call reconciliation with a cloned scene list:

```rust
let scenes = inner
    .lv1_snapshot
    .as_ref()
    .map(|snapshot| snapshot.scene_list.clone())
    .unwrap_or_default();
inner.reconcile_scene_fade_configs(&scenes);
```

In `apply_lv1_event_for_generation`, update the `Lv1Event::SceneListChanged` arm so it reconciles after writing the LV1 scene list:

```rust
Lv1Event::SceneListChanged(scenes) => {
    ensure_lv1_snapshot(&mut inner).scene_list = scenes.clone();
    inner.reconcile_scene_fade_configs(scenes);
    inner.push_log(
        LogSource::Lv1,
        LogSeverity::Info,
        format!("Scene list updated: {} scenes", scenes.len()),
    );
}
```

Update `snapshot_from_inner` to include cloned `scene_fade_configs`, `selected_scene_id`, and `listen_mode_active`.

- [ ] **Step 6: Run tests to verify pass**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_list_reconciliation -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/app_state.rs
git commit -m "feat: reconcile scene fade configs"
```

---

## Task 3: Add Listen Mode And Target Mutation State Logic

**Files:**
- Modify: `src-tauri/src/app_state.rs`

- [ ] **Step 1: Write failing Listen Mode tests**

Add these tests to `src-tauri/src/app_state.rs`:

```rust
fn connected_state_with_scene_and_channel() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: vec![ChannelInfo {
            group: 0,
            channel: 2,
            name: "Lead".to_string(),
            gain_db: -8.0,
            muted: false,
        }],
    }
}

#[tokio::test]
async fn listen_mode_requires_selected_scene_and_known_channels() {
    let state = ShellState::default();

    let err = state.set_listen_mode(true).await.unwrap_err();
    assert_eq!(err, "Select a scene before starting Listen Mode");

    let snapshot = state.begin_connection(Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: Vec::new(),
    }).await;
    assert_eq!(snapshot.selected_scene_id.as_deref(), Some("1::Intro"));

    let err = state.set_listen_mode(true).await.unwrap_err();
    assert_eq!(err, "LV1 channel list is empty");
}

#[tokio::test]
async fn fader_events_write_targets_only_while_listen_mode_is_active() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(connected_state_with_scene_and_channel()).await;

    state.apply_lv1_event_for_generation(
        generation,
        &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -4.5 },
    ).await;
    assert!(state.snapshot().await.scene_fade_configs[0].fade_targets.is_empty());

    state.set_listen_mode(true).await.unwrap();
    state.apply_lv1_event_for_generation(
        generation,
        &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -4.5 },
    ).await;

    let view = state.snapshot().await;
    let targets = &view.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].group, 0);
    assert_eq!(targets[0].channel, 2);
    assert_eq!(targets[0].target_db, -4.5);
    assert!(targets[0].enabled);
}

#[tokio::test]
async fn repeated_fader_event_updates_existing_target() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(connected_state_with_scene_and_channel()).await;
    state.set_listen_mode(true).await.unwrap();

    state.apply_lv1_event_for_generation(generation, &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -4.5 }).await;
    state.apply_lv1_event_for_generation(generation, &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -3.0 }).await;

    let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_db, -3.0);
}

#[tokio::test]
async fn removed_target_can_be_recaptured_while_listen_mode_is_active() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(connected_state_with_scene_and_channel()).await;
    state.set_listen_mode(true).await.unwrap();

    state.apply_lv1_event_for_generation(generation, &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -4.5 }).await;
    state.remove_fade_target("1::Intro", 0, 2).await.unwrap();
    assert!(state.snapshot().await.scene_fade_configs[0].fade_targets.is_empty());

    state.apply_lv1_event_for_generation(generation, &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -2.0 }).await;
    let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_db, -2.0);
}

#[tokio::test]
async fn disconnect_turns_off_listen_mode_and_preserves_configs() {
    let state = ShellState::default();
    state.begin_connection(connected_state_with_scene_and_channel()).await;
    state.set_listen_mode(true).await.unwrap();

    let view = state.disconnect().await;

    assert!(!view.listen_mode_active);
    assert_eq!(view.scene_fade_configs.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri listen_mode -- --nocapture`

Expected: FAIL because the new methods do not exist.

- [ ] **Step 3: Add ShellState mutation methods**

In `impl ShellState`, add these methods:

```rust
pub async fn select_scene_config(&self, scene_id: String) -> Result<AppViewState, String> {
    let mut inner = self.inner.lock().await;
    if inner.listen_mode_active {
        return Err("Stop Listen Mode before selecting another scene".to_string());
    }
    if !inner.scene_fade_configs.iter().any(|config| config.scene_id == scene_id) {
        return Err("Scene config not found".to_string());
    }
    inner.selected_scene_id = Some(scene_id);
    Ok(view_state_from_inner(&inner))
}

pub async fn set_scene_fade_enabled(&self, scene_id: String, enabled: bool) -> Result<AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let config = inner
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;
    config.fade_enabled = enabled;
    Ok(view_state_from_inner(&inner))
}

pub async fn set_listen_mode(&self, active: bool) -> Result<AppViewState, String> {
    let mut inner = self.inner.lock().await;
    if active {
        if inner.selected_scene_id.is_none() {
            return Err("Select a scene before starting Listen Mode".to_string());
        }
        let channel_count = inner.lv1_snapshot.as_ref().map(|snapshot| snapshot.channels.len()).unwrap_or(0);
        if channel_count == 0 {
            return Err("LV1 channel list is empty".to_string());
        }
    }
    inner.listen_mode_active = active;
    inner.push_log(
        LogSource::App,
        LogSeverity::Info,
        format!("Listen Mode {}", if active { "enabled" } else { "disabled" }),
    );
    Ok(view_state_from_inner(&inner))
}

pub async fn set_fade_target_enabled(
    &self,
    scene_id: String,
    group: i32,
    channel: i32,
    enabled: bool,
) -> Result<AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let target = find_target_mut(&mut inner, &scene_id, group, channel)?;
    target.enabled = enabled;
    Ok(view_state_from_inner(&inner))
}

pub async fn remove_fade_target(&self, scene_id: &str, group: i32, channel: i32) -> Result<AppViewState, String> {
    let mut inner = self.inner.lock().await;
    let config = inner
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;
    let before = config.fade_targets.len();
    config.fade_targets.retain(|target| !(target.group == group && target.channel == channel));
    if config.fade_targets.len() == before {
        return Err("Fade target not found".to_string());
    }
    Ok(view_state_from_inner(&inner))
}
```

Add this helper outside the impl:

```rust
fn find_target_mut<'a>(
    inner: &'a mut ShellInner,
    scene_id: &str,
    group: i32,
    channel: i32,
) -> Result<&'a mut FadeTarget, String> {
    let config = inner
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;
    config
        .fade_targets
        .iter_mut()
        .find(|target| target.group == group && target.channel == channel)
        .ok_or_else(|| "Fade target not found".to_string())
}
```

- [ ] **Step 4: Direct-write fader events while active**

Add this `ShellInner` method:

```rust
fn record_fader_target(&mut self, group: i32, channel: i32, gain_db: f64) {
    if !self.listen_mode_active {
        return;
    }

    let Some(selected_scene_id) = self.selected_scene_id.clone() else {
        return;
    };

    let channel_known = self
        .lv1_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.channels.iter().any(|ch| ch.group == group && ch.channel == channel));
    if !channel_known {
        self.push_log(
            LogSource::Lv1,
            LogSeverity::Warning,
            format!("Ignored fader target for unknown channel {group}/{channel}"),
        );
        return;
    }

    let timestamp = current_timestamp();
    if let Some(config) = self
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == selected_scene_id)
    {
        if let Some(target) = config
            .fade_targets
            .iter_mut()
            .find(|target| target.group == group && target.channel == channel)
        {
            target.target_db = gain_db;
            target.updated_at = timestamp;
        } else {
            config.fade_targets.push(FadeTarget {
                group,
                channel,
                target_db: gain_db,
                enabled: true,
                updated_at: timestamp,
            });
        }
    }
}
```

In the `Lv1Event::FaderChanged` arm, after updating the mirrored channel gain, call:

```rust
inner.record_fader_target(*group, *channel, *gain_db);
```

In `disconnect`, before `snapshot_from_inner`, set:

```rust
inner.listen_mode_active = false;
```

- [ ] **Step 5: Run Listen Mode tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri listen_mode -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Run all Tauri state tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/app_state.rs
git commit -m "feat: add listen mode target capture"
```

---

## Task 4: Expose Phase 4 Tauri Commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add command handlers**

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub async fn select_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let view = state.select_scene_config(scene_id).await?;
    emit_snapshot(&app, &view);
    Ok(view)
}

#[tauri::command]
pub async fn set_scene_fade_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let view = state.set_scene_fade_enabled(scene_id, enabled).await?;
    emit_snapshot(&app, &view);
    Ok(view)
}

#[tauri::command]
pub async fn set_listen_mode(
    app: AppHandle,
    state: State<'_, ShellState>,
    active: bool,
) -> Result<AppViewState, String> {
    let view = state.set_listen_mode(active).await?;
    emit_snapshot(&app, &view);
    Ok(view)
}

#[tauri::command]
pub async fn set_fade_target_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
    enabled: bool,
) -> Result<AppViewState, String> {
    let view = state
        .set_fade_target_enabled(scene_id, group, channel, enabled)
        .await?;
    emit_snapshot(&app, &view);
    Ok(view)
}

#[tauri::command]
pub async fn remove_fade_target(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
) -> Result<AppViewState, String> {
    let view = state.remove_fade_target(&scene_id, group, channel).await?;
    emit_snapshot(&app, &view);
    Ok(view)
}
```

- [ ] **Step 2: Register commands**

In `src-tauri/src/main.rs`, add the new functions to `tauri::generate_handler![...]` beside the existing commands:

```rust
commands::select_scene_config,
commands::set_scene_fade_enabled,
commands::set_listen_mode,
commands::set_fade_target_enabled,
commands::remove_fade_target,
```

- [ ] **Step 3: Verify command compile**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: expose scene capture commands"
```

---

## Task 5: Update Frontend Types And Scene Tab UI

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Update TypeScript DTOs**

In `ui/src/types.ts`, add:

```ts
export type FadeTarget = {
  group: number;
  channel: number;
  targetDb: number;
  enabled: boolean;
  updatedAt: string;
};

export type SceneFadeConfig = {
  sceneId: string;
  sceneIndex: number;
  sceneName: string;
  fadeEnabled: boolean;
  fadeTargets: FadeTarget[];
};
```

Extend `AppViewState` and `disconnectedAppViewState`:

```ts
sceneFadeConfigs: SceneFadeConfig[];
selectedSceneId: string | null;
listenModeActive: boolean;
```

Set defaults to `[]`, `null`, and `false`.

- [ ] **Step 2: Pass command handlers into SceneTab**

In `ui/src/App.tsx`, render `SceneTab` like this:

```tsx
{activeTab === "scene" && (
  <SceneTab
    appState={appState}
    selectScene={(sceneId) => runSnapshotCommand("select_scene_config", { sceneId })}
    setSceneFadeEnabled={(sceneId, enabled) => runSnapshotCommand("set_scene_fade_enabled", { sceneId, enabled })}
    setListenMode={(active) => runSnapshotCommand("set_listen_mode", { active })}
    setFadeTargetEnabled={(sceneId, group, channel, enabled) =>
      runSnapshotCommand("set_fade_target_enabled", { sceneId, group, channel, enabled })
    }
    removeFadeTarget={(sceneId, group, channel) => runSnapshotCommand("remove_fade_target", { sceneId, group, channel })}
  />
)}
```

- [ ] **Step 3: Replace SceneTab implementation**

Replace the current `SceneTab` function with this split layout:

```tsx
function SceneTab(props: {
  appState: AppViewState;
  selectScene: (sceneId: string) => void;
  setSceneFadeEnabled: (sceneId: string, enabled: boolean) => void;
  setListenMode: (active: boolean) => void;
  setFadeTargetEnabled: (sceneId: string, group: number, channel: number, enabled: boolean) => void;
  removeFadeTarget: (sceneId: string, group: number, channel: number) => void;
}) {
  const selected = props.appState.sceneFadeConfigs.find(
    (scene) => scene.sceneId === props.appState.selectedSceneId,
  );

  return (
    <div className="grid gap-5 lg:grid-cols-[22rem_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Scenes</h2>
        <p className="mt-1 text-sm text-slate-400">
          Select the scene fade config to edit. Scene selection locks while Listen Mode is active.
        </p>
        <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
          {props.appState.sceneFadeConfigs.length === 0 ? (
            <p className="p-3 text-sm text-slate-400">No scenes loaded.</p>
          ) : (
            props.appState.sceneFadeConfigs.map((scene) => {
              const selectedRow = scene.sceneId === props.appState.selectedSceneId;
              return (
                <button
                  className={
                    selectedRow
                      ? "block w-full border-b border-slate-800 bg-cyan-950/40 px-3 py-3 text-left last:border-b-0"
                      : "block w-full border-b border-slate-800 px-3 py-3 text-left hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 last:border-b-0"
                  }
                  disabled={props.appState.listenModeActive}
                  key={scene.sceneId}
                  onClick={() => props.selectScene(scene.sceneId)}
                >
                  <span className="block text-sm font-semibold text-slate-100">
                    {scene.sceneIndex}: {scene.sceneName}
                  </span>
                  <span className="mt-1 block text-xs text-slate-400">
                    {scene.fadeEnabled ? "Enabled" : "Disabled"} · {scene.fadeTargets.length} targets
                  </span>
                </button>
              );
            })
          )}
        </div>
      </section>

      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        {selected ? (
          <div>
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h2 className="text-lg font-semibold">
                  {selected.sceneIndex}: {selected.sceneName}
                </h2>
                <p className="mt-1 text-sm text-slate-400">
                  Current LV1 scene does not affect which scene config is edited.
                </p>
              </div>
              <div className="flex flex-wrap gap-3">
                <button
                  className={
                    selected.fadeEnabled
                      ? "rounded-lg border border-emerald-500/60 bg-emerald-950 px-4 py-2 font-semibold text-emerald-100"
                      : "rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800"
                  }
                  onClick={() => props.setSceneFadeEnabled(selected.sceneId, !selected.fadeEnabled)}
                >
                  {selected.fadeEnabled ? "Fade Enabled" : "Fade Disabled"}
                </button>
                <button
                  className={
                    props.appState.listenModeActive
                      ? "rounded-lg bg-amber-700 px-4 py-2 font-bold text-white hover:bg-amber-600"
                      : "rounded-lg bg-cyan-700 px-4 py-2 font-bold text-white hover:bg-cyan-600"
                  }
                  onClick={() => props.setListenMode(!props.appState.listenModeActive)}
                >
                  {props.appState.listenModeActive ? "Stop Listen Mode" : "Start Listen Mode"}
                </button>
              </div>
            </div>

            <FadeTargetTable
              scene={selected}
              setFadeTargetEnabled={props.setFadeTargetEnabled}
              removeFadeTarget={props.removeFadeTarget}
            />
          </div>
        ) : (
          <p className="text-sm text-slate-400">Select a scene to edit its fade targets.</p>
        )}
      </section>
    </div>
  );
}
```

- [ ] **Step 4: Add target table component**

Add below `SceneTab`:

```tsx
function FadeTargetTable(props: {
  scene: SceneFadeConfig;
  setFadeTargetEnabled: (sceneId: string, group: number, channel: number, enabled: boolean) => void;
  removeFadeTarget: (sceneId: string, group: number, channel: number) => void;
}) {
  return (
    <div className="mt-5 overflow-auto rounded-lg border border-slate-800">
      {props.scene.fadeTargets.length === 0 ? (
        <p className="p-3 text-sm text-slate-400">No fader targets captured. Start Listen Mode and move LV1 faders.</p>
      ) : (
        <table className="w-full min-w-[42rem] text-sm">
          <thead className="bg-slate-950 text-left text-slate-400">
            <tr>
              <th className="px-3 py-2">Include</th>
              <th className="px-3 py-2">Group</th>
              <th className="px-3 py-2">Channel</th>
              <th className="px-3 py-2">Target</th>
              <th className="px-3 py-2">Updated</th>
              <th className="px-3 py-2">Action</th>
            </tr>
          </thead>
          <tbody>
            {props.scene.fadeTargets.map((target) => (
              <tr className="border-t border-slate-800" key={`${target.group}-${target.channel}`}>
                <td className="px-3 py-2">
                  <input
                    checked={target.enabled}
                    onChange={(event) =>
                      props.setFadeTargetEnabled(
                        props.scene.sceneId,
                        target.group,
                        target.channel,
                        event.target.checked,
                      )
                    }
                    type="checkbox"
                  />
                </td>
                <td className="px-3 py-2">{target.group}</td>
                <td className="px-3 py-2">{target.channel}</td>
                <td className="px-3 py-2">{formatDb(target.targetDb)}</td>
                <td className="px-3 py-2 text-slate-400">{target.updatedAt}</td>
                <td className="px-3 py-2">
                  <button
                    className="rounded border border-red-800 px-3 py-1 text-red-100 hover:bg-red-950"
                    onClick={() => props.removeFadeTarget(props.scene.sceneId, target.group, target.channel)}
                  >
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function formatDb(value: number) {
  return `${value.toFixed(1)} dB`;
}
```

Note: current channel name display is omitted until `AppViewState` exposes channel summaries. Do not store channel names on `FadeTarget`.

- [ ] **Step 5: Typecheck frontend**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/types.ts ui/src/App.tsx
git commit -m "feat: render listen mode scene editor"
```

---

## Task 6: Expose Channel Summaries For Derived Target Names

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Add Rust channel summary DTO**

In `src-tauri/src/app_state.rs`, add near `SceneSummary`:

```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub group: i32,
    pub channel: i32,
    pub name: String,
}
```

Add to `AppViewState`:

```rust
pub channels: Vec<ChannelSummary>,
```

In `view_state_from_inner`, build channels from `lv1_snapshot.channels`:

```rust
let channels = inner
    .lv1_snapshot
    .as_ref()
    .map(|snapshot| {
        snapshot
            .channels
            .iter()
            .map(|channel| ChannelSummary {
                group: channel.group,
                channel: channel.channel,
                name: channel.name.clone(),
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
```

Include `channels` in `AppViewState`.

- [ ] **Step 2: Update TypeScript types**

In `ui/src/types.ts`, add:

```ts
export type ChannelSummary = {
  group: number;
  channel: number;
  name: string;
};
```

Add `channels: ChannelSummary[];` to `AppViewState` and default it to `[]`.

- [ ] **Step 3: Render derived channel names**

Pass `channels={props.appState.channels}` into `FadeTargetTable`.

Update `FadeTargetTable` props:

```tsx
channels: ChannelSummary[];
```

Add a channel name helper:

```tsx
function channelName(channels: ChannelSummary[], group: number, channel: number) {
  return channels.find((item) => item.group === group && item.channel === channel)?.name ?? "Unknown";
}
```

Add a `Name` column between `Channel` and `Target` and render:

```tsx
<td className="px-3 py-2">{channelName(props.channels, target.group, target.channel)}</td>
```

- [ ] **Step 4: Verify derived names compile**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app_state.rs ui/src/types.ts ui/src/App.tsx
git commit -m "feat: derive target channel names from mirror"
```

---

## Task 7: Final Verification And Phase Checklist

**Files:**
- Modify: `PHASES.md`

- [ ] **Step 1: Run full Rust tests**

Run: `cargo test`

Expected: all workspace Rust tests pass.

- [ ] **Step 2: Run frontend typecheck and build**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: Vite production build succeeds.

- [ ] **Step 3: Update Phase 4 status**

In `PHASES.md`, change line 11 from unchecked to complete and summarize the implemented behavior:

```md
- [x] **Phase 4: Capture Engine And Listen Mode** — in-memory scene fade configs, selected-scene editing, Listen Mode direct-write capture, target enable/remove behavior, scene-list reconciliation, and split Scene tab UI are implemented and tested. Persistence and durable rename/reorder matching remain deferred to Phase 5.
```

- [ ] **Step 4: Check git diff**

Run: `git diff -- src-tauri/src/app_state.rs src-tauri/src/commands.rs src-tauri/src/main.rs ui/src/types.ts ui/src/App.tsx PHASES.md`

Expected: diff only includes Phase 4 implementation and checklist update.

- [ ] **Step 5: Commit**

```bash
git add PHASES.md
git commit -m "docs: mark phase 4 complete"
```

---

## Self-Review Notes

- Spec coverage: plan covers naming, Rust-owned state, scene config reconciliation, direct-write Listen Mode, scene-selection lock, target remove/recapture, command errors, split UI, derived channel names, tests, and Phase 5 deferrals.
- Scope kept: no JSON persistence, no auto recall, no HTTP API, no fader sends from Listen Mode.
- Type consistency: the plan uses `AppViewState`, `SceneFadeConfig`, `FadeTarget`, `sceneFadeConfigs`, `selectedSceneId`, and `listenModeActive` consistently across Rust and TypeScript.
