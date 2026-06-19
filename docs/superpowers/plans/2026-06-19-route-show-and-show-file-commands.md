# Route Show And Show-File Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route low-risk show/app mutations and show-file commands through `AppCommandBus` while preserving the current frontend command contract and transitional direct snapshot emission.

**Architecture:** `ShowStateHandle` already owns show mutation and publishes `AppEvent::Show`; phases 8-10 add show-owned command and show-file mapping helpers plus public routing methods on `AppCommandBus`. Tauri command handlers keep owning dialogs and filesystem IO, call the lifecycle-owned current `AppCommandBus` for show/show-file mutation and mapping, then keep returning and emitting `AppViewState` snapshots until the later projector-only and React contract phases.

**Tech Stack:** Rust 2024, Tauri 2, Tokio, `thiserror`, `cargo nextest`, React/TypeScript frontend unchanged except typecheck verification.

## Global Constraints

- Phase scope is migration phases 8-10 from `docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md`.
- Route low-risk show/app commands through `AppCommandBus`, including cue, lockout, selected scene, duration, scope edits, and store scene config.
- Move show-file DTOs, import/export mapping, pruning, and validation into `show/`.
- Route show-file commands through `AppCommandBus`, using show-owned import/export/mapping and publishing `ShowEvent` through `ShowStateHandle` mutations.
- Do not move UI-requested recall validation or dispatch in this phase.
- Do not build the new projector cache in this phase.
- Do not remove `ShellState`, `ActiveCommandBus`, direct emits, command-return snapshots, or frontend `applySnapshot` usage in this phase.
- Preserve Tauri command names and current frontend return payloads.
- Preserve `app-status-changed` direct emission behavior until projector-only phase.
- Do not route logs through `AppEventBus`.
- Do not change LV1 protocol behavior.
- Do not weaken lockout, exact scene identity, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- Show/app mutations must continue to publish `AppEvent::Show` through `ShowStateHandle` only when the underlying mutation changes state.
- Before claiming completion, run targeted Rust tests plus `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo build --workspace`, `cargo build -p advanced-show-control --bin lv1-probe`, `npm --prefix ui run typecheck`, and `npm run tauri -- build`.

---

## File Structure

- Modify `src-tauri/src/show/commands.rs`: define show-owned command result types and helper functions that wrap `ShowStateHandle` mutations without knowing about Tauri, `ShellState`, or frontend snapshots.
- Create/modify `src-tauri/src/show/show_file.rs`: own show-file DTOs, schema version, import/export mapping, pruning, and validation against LV1 scene snapshots.
- Modify `src-tauri/src/show/mod.rs`: export the show-file module.
- Modify `src-tauri/src/show_file.rs`: reduce to filesystem and path infrastructure only, or delete after moving DTO/mapping/pruning out if no filesystem helpers remain outside a renamed infrastructure module.
- Modify `src-tauri/src/runtime/commands.rs`: add public `AppCommandBus` methods for covered show/app and show-file commands; clone the current `ShowStateHandle` under the targets lock, drop the lock, call show-owned helpers, and map missing show state to `AppCommandError::ShowUnavailable`.
- Modify `src-tauri/src/commands.rs`: adapt covered Tauri commands to obtain the current lifecycle command bus, call the new bus methods, preserve current tracing where it already exists, then emit and return `state.snapshot().await`.
- Modify `src-tauri/src/app_state/shell.rs`: remove or reduce direct show-mutation helpers only after call sites no longer need them; leave snapshot, runtime lifecycle, show-file, projection, and recall helpers intact.
- Move or update tests from `src-tauri/src/app_state/show_file_mapping_tests.rs` into show-owned tests where they cover DTO/mapping/pruning; keep shell/Tauri tests only for transitional path, selected-scene, file metadata, dialogs, and snapshot emission behavior.
- Do not modify `ui/` or projector structure for this phase except for verification fallout.

---

### Task 1: Add Show Command Results And Bus Routing

**Files:**
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Test: `src-tauri/src/runtime/commands.rs`

**Interfaces:**
- Consumes: `ShowStateHandle::{cue_scene,set_lockout,set_scene_duration,set_scene_scope_faders_enabled,set_scene_scope_pan_enabled,set_channel_scoped,set_all_channels_scoped,store_scene_config,get_scene_config}` and `Lv1StateSnapshot`.
- Produces: `show::commands::ShowCommandResult`, `show::commands::SelectedSceneResult`, `show::commands::cue_scene`, `show::commands::select_scene_config`, `show::commands::set_lockout`, `show::commands::set_scene_duration_ms`, `show::commands::set_scene_scope_faders_enabled`, `show::commands::set_scene_scope_pan_enabled`, `show::commands::set_channel_scoped`, `show::commands::set_all_channels_scoped`, `show::commands::store_scene_config`, and matching `AppCommandBus` public methods.

- [ ] **Step 1: Write failing command-bus tests for missing and present show targets**

Add these imports inside `#[cfg(test)] mod tests` in `src-tauri/src/runtime/commands.rs` if they are not already present:

```rust
use crate::lv1::types::ChannelInfo;
use crate::show::types::{ChannelConfig, SceneConfig, SceneScopeToggles, ShowSnapshot};
```

Add these test helpers near the top of the same test module:

```rust
fn scene_config() -> SceneConfig {
    SceneConfig {
        scene_id: "1:Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        duration_ms: 0,
        channel_configs: vec![ChannelConfig {
            group: 0,
            channel: 1,
            fader_db: Some(-12.0),
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }],
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    }
}

fn channel_info() -> ChannelInfo {
    ChannelInfo {
        group: 0,
        channel: 1,
        name: "Vocal".to_string(),
        gain_db: -12.0,
        muted: false,
        pan: None,
        balance: None,
        width: None,
        pan_mode: None,
    }
}

async fn bus_with_show_snapshot(snapshot: ShowSnapshot) -> (AppCommandBus, AppEventBus) {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    show.replace_snapshot(snapshot).await;
    let bus = AppCommandBus::new();
    bus.set_show(Some(show)).await;
    (bus, event_bus)
}
```

Add these failing tests:

```rust
#[tokio::test]
async fn set_lockout_routes_to_show_state() {
    let (bus, event_bus) = bus_with_show_snapshot(ShowSnapshot::empty()).await;
    let mut events = event_bus.subscribe();

    let result = bus.set_lockout(true).await.unwrap();

    assert!(result.changed);
    assert!(bus.get_show_snapshot().await.unwrap().lockout);
    assert!(matches!(
        events.recv().await.unwrap(),
        crate::runtime::events::AppEvent::Show(_)
    ));
}

#[tokio::test]
async fn set_scene_duration_routes_to_show_state() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus
        .set_scene_duration_ms("1:Intro".to_string(), 2_500)
        .await
        .unwrap();

    assert!(result.changed);
    let updated = bus.get_scene_config("1:Intro".to_string()).await.unwrap().unwrap();
    assert_eq!(updated.duration_ms, 2_500);
}

#[tokio::test]
async fn scope_edit_routes_to_show_state() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus
        .set_channel_scoped("1:Intro".to_string(), 0, 1, true)
        .await
        .unwrap();

    assert!(result.changed);
    let updated = bus.get_scene_config("1:Intro".to_string()).await.unwrap().unwrap();
    assert_eq!(updated.scoped_channels.len(), 1);
    assert_eq!(updated.scoped_channels[0].group, 0);
    assert_eq!(updated.scoped_channels[0].channel, 1);
}

#[tokio::test]
async fn all_channels_scope_routes_to_show_state() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus
        .set_all_channels_scoped("1:Intro".to_string(), true)
        .await
        .unwrap();

    assert!(result.changed);
    let updated = bus.get_scene_config("1:Intro".to_string()).await.unwrap().unwrap();
    assert_eq!(updated.scoped_channels.len(), 1);
}

#[tokio::test]
async fn scene_scope_toggles_route_to_show_state() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let faders = bus
        .set_scene_scope_faders_enabled("1:Intro".to_string(), false)
        .await
        .unwrap();
    let pan = bus
        .set_scene_scope_pan_enabled("1:Intro".to_string(), true)
        .await
        .unwrap();

    assert!(faders.changed);
    assert!(pan.changed);
    let updated = bus.get_scene_config("1:Intro".to_string()).await.unwrap().unwrap();
    assert!(!updated.scope_toggles.faders);
    assert!(updated.scope_toggles.pan);
}

#[tokio::test]
async fn cue_scene_routes_to_show_state_and_returns_scene() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus.cue_scene("1:Intro".to_string()).await.unwrap();

    assert!(result.changed);
    assert_eq!(result.scene.scene_id, "1:Intro");
    assert_eq!(bus.get_show_snapshot().await.unwrap().cued_scene_id, Some("1:Intro".to_string()));
}

#[tokio::test]
async fn select_scene_config_validates_through_show_state() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus.select_scene_config("1:Intro".to_string()).await.unwrap();

    assert_eq!(result.scene.scene_id, "1:Intro");
}

#[tokio::test]
async fn store_scene_config_routes_to_show_state_with_lv1_channels() {
    let snapshot = ShowSnapshot {
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let result = bus
        .store_scene_config("1:Intro".to_string(), vec![channel_info()])
        .await
        .unwrap();

    assert!(result.changed);
    let updated = bus.get_scene_config("1:Intro".to_string()).await.unwrap().unwrap();
    assert_eq!(updated.channel_configs.len(), 1);
    assert_eq!(updated.channel_configs[0].fader_db, Some(-12.0));
}

#[tokio::test]
async fn low_risk_show_commands_return_show_unavailable_without_target() {
    let bus = AppCommandBus::new();

    assert_eq!(bus.set_lockout(true).await.unwrap_err(), AppCommandError::ShowUnavailable);
    assert_eq!(
        bus.set_scene_duration_ms("1:Intro".to_string(), 100).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
    assert_eq!(
        bus.set_channel_scoped("1:Intro".to_string(), 0, 1, true).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
    assert_eq!(
        bus.set_all_channels_scoped("1:Intro".to_string(), true).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
    assert_eq!(
        bus.set_scene_scope_faders_enabled("1:Intro".to_string(), false).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
    assert_eq!(
        bus.set_scene_scope_pan_enabled("1:Intro".to_string(), true).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
    assert_eq!(bus.cue_scene("1:Intro".to_string()).await.unwrap_err(), AppCommandError::ShowUnavailable);
    assert_eq!(bus.select_scene_config("1:Intro".to_string()).await.unwrap_err(), AppCommandError::ShowUnavailable);
    assert_eq!(
        bus.store_scene_config("1:Intro".to_string(), vec![channel_info()]).await.unwrap_err(),
        AppCommandError::ShowUnavailable
    );
}
```

- [ ] **Step 2: Run targeted tests and verify failure**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::commands::tests::set_lockout_routes_to_show_state runtime::commands::tests::low_risk_show_commands_return_show_unavailable_without_target
```

Expected: FAIL to compile because methods such as `AppCommandBus::set_lockout` and result fields such as `changed` are not defined.

- [ ] **Step 3: Add show-owned command result helpers**

Replace the placeholder contents of `src-tauri/src/show/commands.rs` with:

```rust
//! Show-owned application command handlers.

use crate::lv1::types::ChannelInfo;

use super::handle::ShowStateHandle;
use super::types::SceneConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

pub async fn set_lockout(show: &ShowStateHandle, enabled: bool) -> ShowCommandResult {
    ShowCommandResult {
        changed: show.set_lockout(enabled).await,
    }
}

pub async fn set_scene_duration_ms(
    show: &ShowStateHandle,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_scene_duration(scene_id, duration_ms).await?,
    })
}

pub async fn set_scene_scope_faders_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_scene_scope_faders_enabled(scene_id, enabled)
            .await?,
    })
}

pub async fn set_scene_scope_pan_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_scene_scope_pan_enabled(scene_id, enabled).await?,
    })
}

pub async fn set_channel_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_channel_scoped(scene_id, group, channel, scoped)
            .await?,
    })
}

pub async fn set_all_channels_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_all_channels_scoped(scene_id, scoped).await?,
    })
}

pub async fn cue_scene(
    show: &ShowStateHandle,
    scene_id: String,
) -> Result<CueSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id.clone())
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(CueSceneResult {
        changed: show.cue_scene(scene_id).await?,
        scene,
    })
}

pub async fn select_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
) -> Result<SelectedSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id)
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(SelectedSceneResult { scene })
}

pub async fn store_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
    channels: Vec<ChannelInfo>,
) -> Result<ShowCommandResult, String> {
    if show.get_scene_config(scene_id.clone()).await.is_none() {
        return Err("Scene config not found".to_string());
    }

    Ok(ShowCommandResult {
        changed: show.store_scene_config(scene_id, channels).await?,
    })
}
```

- [ ] **Step 4: Add command-bus routing methods**

In `src-tauri/src/runtime/commands.rs`, update imports:

```rust
use crate::lv1::types::{ChannelInfo, Lv1StateSnapshot};
use crate::show::commands::{CueSceneResult, SelectedSceneResult, ShowCommandResult};
```

Add this helper and methods inside `impl AppCommandBus`, after `get_lockout` and before `get_lv1_state`:

```rust
    async fn show_target(&self) -> Result<ShowStateHandle, AppCommandError> {
        self.targets
            .lock()
            .await
            .show
            .clone()
            .ok_or(AppCommandError::ShowUnavailable)
    }

    pub async fn set_lockout(
        &self,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::commands::set_lockout(&show, enabled).await)
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_duration_ms(&show, scene_id, duration_ms)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_scope_faders_enabled(&show, scene_id, enabled)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_scope_pan_enabled(&show, scene_id, enabled)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_channel_scoped(&show, scene_id, group, channel, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_all_channels_scoped(&show, scene_id, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn cue_scene(&self, scene_id: String) -> Result<CueSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::cue_scene(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn select_scene_config(
        &self,
        scene_id: String,
    ) -> Result<SelectedSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::select_scene_config(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::store_scene_config(&show, scene_id, channels)
            .await
            .map_err(AppCommandError::CommandFailed)
    }
```

- [ ] **Step 5: Run targeted command-bus tests**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::commands::tests
```

Expected: PASS for runtime command-bus tests.

- [ ] **Step 6: Commit Task 1**

Run:

```bash
git add src-tauri/src/show/commands.rs src-tauri/src/runtime/commands.rs
git commit -m "refactor: route show mutations through command bus"
```

---

### Task 2: Adapt Tauri Commands To Use The Active Command Bus

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Test: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `AppLifecycle::command_bus_holder()`, `ActiveCommandBus::current()`, and the `AppCommandBus` methods from Task 1.
- Produces: covered Tauri commands that call `AppCommandBus` first, then preserve snapshot return and `emit_snapshot` behavior.

- [ ] **Step 1: Add a command-bus lookup helper**

Add this helper near the other private helpers in `src-tauri/src/commands.rs`, after `resolve_connect_target`:

```rust
async fn current_command_bus(
    active_command_bus: ActiveCommandBus,
    command_name: &'static str,
) -> Result<AppCommandBus, String> {
    active_command_bus.current().await.ok_or_else(|| {
        tracing::warn!(
            event = "command_blocked",
            command = command_name,
            reason = "app command bus is unavailable",
            "Command blocked: app command bus is unavailable"
        );
        "App command bus is unavailable".to_string()
    })
}

fn map_app_command_error(error: crate::runtime::commands::AppCommandError) -> String {
    match error {
        crate::runtime::commands::AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}
```

- [ ] **Step 2: Change covered Tauri command signatures to accept lifecycle state**

Update these command signatures in `src-tauri/src/commands.rs` to include `lifecycle: State<'_, AppLifecycle>`:

```rust
pub async fn set_scene_duration_ms(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    duration_ms: u64,
) -> Result<AppViewState, String>
```

```rust
pub async fn select_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String>
```

```rust
pub async fn cue_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String>
```

```rust
pub async fn store_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String>
```

```rust
pub async fn set_channel_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<AppViewState, String>
```

```rust
pub async fn set_all_channels_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    scoped: bool,
) -> Result<AppViewState, String>
```

```rust
pub async fn set_scene_scope_faders_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String>
```

```rust
pub async fn set_scene_scope_pan_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String>
```

```rust
pub async fn set_lockout(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    enabled: bool,
) -> Result<AppViewState, String>
```

- [ ] **Step 3: Replace direct ShellState mutations in covered Tauri commands**

For each covered Tauri command, use this pattern:

```rust
let command_bus = current_command_bus(lifecycle.command_bus_holder(), "command_name").await?;
command_bus
    .matching_method(args)
    .await
    .map_err(map_app_command_error)?;
let snapshot = state.snapshot().await;
emit_snapshot(&app, &snapshot);
Ok(snapshot)
```

Use these exact method mappings:

```rust
command_bus.set_scene_duration_ms(scene_id, duration_ms).await
command_bus.cue_scene(scene_id).await
command_bus.store_scene_config(scene_id, lv1.channels).await
command_bus.set_channel_scoped(scene_id, group, channel, scoped).await
command_bus.set_all_channels_scoped(scene_id, scoped).await
command_bus.set_scene_scope_faders_enabled(scene_id, enabled).await
command_bus.set_scene_scope_pan_enabled(scene_id, enabled).await
command_bus.set_lockout(enabled).await
```

For `store_scene_config`, preserve the current LV1 snapshot source until recall/lifecycle projection phases move it:

```rust
let lv1 = state
    .lv1_snapshot()
    .await
    .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())?;
command_bus
    .store_scene_config(scene_id, lv1.channels)
    .await
    .map_err(map_app_command_error)?;
```

For `cue_scene`, preserve existing tracing by replacing `cue_scene_snapshot` with:

```rust
async fn cue_scene_snapshot(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    scene_id: String,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_cue_requested",
        scene_id = %scene_id,
        "Scene cue requested"
    );

    let command_bus = current_command_bus(active_command_bus, "cue_scene").await?;
    let result = command_bus
        .cue_scene(scene_id.clone())
        .await
        .map_err(|error| {
            if matches!(error, crate::runtime::commands::AppCommandError::CommandFailed(_)) {
                tracing::warn!(
                    event = "scene_cue_blocked",
                    scene_id = %scene_id,
                    reason = "scene config not found",
                    "Scene cue blocked: scene config not found"
                );
            }
            map_app_command_error(error)
        })?;

    tracing::info!(
        event = "scene_cued",
        scene_id = %result.scene.scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene cued: {}",
        result.scene.scene_name
    );

    Ok(state.snapshot().await)
}
```

For `select_scene_config`, validate through `AppCommandBus` first, then keep the selected scene value in `ShellState` because selected-scene UI projection has not moved into show-owned state yet:

```rust
let command_bus = current_command_bus(lifecycle.command_bus_holder(), "select_scene_config").await?;
command_bus
    .select_scene_config(scene_id.clone())
    .await
    .map_err(map_app_command_error)?;
let snapshot = state.select_scene_config(scene_id).await?;
emit_snapshot(&app, &snapshot);
Ok(snapshot)
```

- [ ] **Step 4: Add `ShellState::lv1_snapshot` accessor if needed**

If no accessor already exists, add this method to `impl ShellState` in `src-tauri/src/app_state/shell.rs`:

```rust
pub async fn lv1_snapshot(&self) -> Option<Lv1StateSnapshot> {
    self.inner.lock().await.lv1_snapshot.clone()
}
```

Do not expose `inner` publicly and do not move LV1 mirror ownership in this task.

- [ ] **Step 5: Remove now-unused direct ShellState mutation methods if compile proves they are unused**

After adapting the Tauri commands, run `cargo check --workspace --all-targets`. If the methods below are unused and no tests or show-file code still call them, remove them from `src-tauri/src/app_state/shell.rs`:

```rust
set_scene_duration_ms
cue_scene
store_scene_config
set_channel_scoped
set_all_channels_scoped
set_scene_scope_faders_enabled
set_scene_scope_pan_enabled
```

Keep `select_scene_config` because the selected scene is still shell projection state in this phase. Keep `set_lockout` only if existing tests or non-covered command paths still use it; otherwise remove it after command adaptation.

- [ ] **Step 6: Run targeted compile/test checks**

Run:

```bash
cargo check --workspace --all-targets
cargo nextest run -p advanced-show-control commands::tests
```

Expected: PASS. Existing command tests should still receive `AppViewState` snapshots and should not require frontend changes.

- [ ] **Step 7: Commit Task 2**

Run:

```bash
git add src-tauri/src/commands.rs src-tauri/src/app_state/shell.rs
git commit -m "refactor: route tauri show commands through command bus"
```

---

### Task 3: Characterize Transitional Snapshot And Event Behavior

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs` if additional unit coverage is simpler there
- Test: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: Task 2 command routing and current `emit_snapshot` transitional behavior.
- Produces: regression coverage that phase 8 preserves command-return snapshots/direct emits and no-op show commands do not publish misleading show events.

- [ ] **Step 1: Add or update command tests that prove covered commands still return snapshots**

In `src-tauri/src/commands.rs` tests, update existing direct calls for changed Tauri signatures to pass `AppLifecycle` state where necessary. If the existing helpers construct `ShellState` only, add an `AppLifecycle`, create an `AppCommandBus`, install the test state's show handle, and install the bus into the lifecycle holder.

Use this helper inside the tests module if no equivalent exists:

```rust
async fn lifecycle_with_show(state: &ShellState) -> AppLifecycle {
    let lifecycle = AppLifecycle::default();
    let bus = AppCommandBus::new();
    bus.set_show(Some(state.show.clone())).await;
    lifecycle.set_command_bus(Some(bus)).await;
    lifecycle
}
```

Use `AppLifecycle::set_command_bus(Some(bus)).await`; do not add a new public lifecycle API for tests.

Add or update a test equivalent to:

```rust
#[tokio::test]
async fn cue_scene_updates_show_state_through_command_bus_and_returns_snapshot() {
    let state = recall_state_with_unstored_scene(true).await;
    let lifecycle = lifecycle_with_show(&state).await;

    let snapshot = cue_scene_snapshot(
        state.clone(),
        lifecycle.command_bus_holder(),
        "1:Intro".to_string(),
    )
    .await
    .unwrap();

    assert_eq!(snapshot.cued_scene_id, Some("1:Intro".to_string()));
    assert!(snapshot.lockout);
}
```

- [ ] **Step 2: Add no-op event characterization for command bus path**

Add this test to `src-tauri/src/runtime/commands.rs` tests:

```rust
#[tokio::test]
async fn no_op_show_command_through_bus_does_not_publish_show_event() {
    let snapshot = ShowSnapshot {
        lockout: true,
        scene_configs: vec![scene_config()],
        ..ShowSnapshot::empty()
    };
    let (bus, event_bus) = bus_with_show_snapshot(snapshot).await;
    let mut events = event_bus.subscribe();

    while events.try_recv().is_ok() {}

    let result = bus.set_lockout(true).await.unwrap();

    assert!(!result.changed);
    assert!(events.try_recv().is_err());
}
```

- [ ] **Step 3: Run targeted tests**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::commands::tests commands::tests
```

Expected: PASS. If command tests need updates because signatures changed, update test call sites only; do not alter frontend command names or payloads.

- [ ] **Step 4: Commit Task 3**

Run:

```bash
git add src-tauri/src/commands.rs src-tauri/src/runtime/commands.rs
git commit -m "test: cover routed show command behavior"
```

---

### Task 4: Phase-8 Documentation And Checkpoint Verification

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/plans/2026-06-19-route-show-and-show-file-commands.md` only if execution discovered plan corrections

**Interfaces:**
- Consumes: Tasks 1-3 behavior.
- Produces: updated architecture docs that describe phase-8 routing while show-file phases continue in later tasks.

- [ ] **Step 1: Update architecture docs**

In `docs/architecture.md`, update the `Bus Contracts`, `Command Flow`, and `File Structure` sections to state:

```markdown
Low-risk show/app mutations such as cue, lockout, selected scene validation, duration edits, scope edits, and storing scene config are routed through `AppCommandBus` during the transition. The Tauri command layer still returns and directly emits `AppViewState` snapshots until the projector-only and frontend command-contract phases remove that temporary behavior.
```

Keep wording explicit that show-file commands and UI-requested recall are still pending later phases.

- [ ] **Step 2: Run formatting and focused checks**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control runtime::commands::tests commands::tests
```

Expected: PASS.

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo nextest run --workspace
cargo build --workspace
cargo build -p advanced-show-control --bin lv1-probe
npm --prefix ui run typecheck
npm run tauri -- build
```

Expected: PASS. The known non-fatal Tauri warning about bundle identifier ending with `.app` may still appear and does not fail the build.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
git status --short
```

Expected: Only intended phase-8 files are modified. No frontend files should change unless `npm` tooling changed lockfiles unexpectedly; do not commit unrelated generated changes.

- [ ] **Step 5: Commit Task 4**

Run:

```bash
git add docs/architecture.md
git commit -m "docs: describe routed show command boundary"
```

---

### Task 5: Move Show-File DTOs And Mapping Into Show

**Files:**
- Create: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/show/mod.rs`
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Test: move mapping/pruning tests from `src-tauri/src/app_state/show_file_mapping_tests.rs` to `src-tauri/src/show/show_file.rs` where they no longer need `ShellState`

**Interfaces:**
- Consumes: `ShowSnapshot`, `ShowStateHandle`, `Lv1StateSnapshot`, `SceneListEntry`, `scene_id`, and current `ShowFile` DTOs from `src-tauri/src/show_file.rs`.
- Produces: `crate::show::show_file::{SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileSafety, ShowFileSceneConfig, ShowFileSceneScopeToggles, ShowFileChannelConfig, ShowFileChannelRef, LoadValidationReport, export_show_file, import_show_file, prune_show_file_to_lv1_scenes}`.

- [ ] **Step 1: Write show-owned mapping tests before moving code**

Create `#[cfg(test)] mod tests` in new `src-tauri/src/show/show_file.rs` with these focused tests. Use the current DTO names, but import them from `super::*` after the move:

```rust
#[test]
fn export_show_file_contains_current_configs() {
    let snapshot = crate::show::types::ShowSnapshot {
        lockout: true,
        cued_scene_id: Some("1:Intro".to_string()),
        scene_configs: vec![crate::show::types::SceneConfig {
            scene_id: "1:Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5_000,
            channel_configs: vec![crate::show::types::ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-8.0),
                pan: Some(-12.0),
                balance: Some(3.0),
                width: Some(1.2),
                pan_mode: Some(crate::lv1::types::PanMode::Stereo),
            }],
            scoped_channels: vec![crate::show::types::ChannelRef { group: 0, channel: 2 }],
            scope_toggles: crate::show::types::SceneScopeToggles { faders: false, pan: true },
        }],
    };

    let file = export_show_file(snapshot, "saved".to_string());

    assert_eq!(file.schema_version, SHOW_FILE_SCHEMA_VERSION);
    assert_eq!(file.saved_at, "saved");
    assert!(file.safety.lockout);
    assert_eq!(file.cued_scene_id, Some("1:Intro".to_string()));
    assert_eq!(file.scene_configs[0].scene_index, 1);
    assert_eq!(file.scene_configs[0].channel_configs[0].fader_db, Some(-8.0));
    assert!(!file.scene_configs[0].scope_toggles.faders);
    assert!(file.scene_configs[0].scope_toggles.pan);
}

#[test]
fn import_show_file_prunes_missing_scenes_and_filters_cue() {
    let mut file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: ShowFileSafety { lockout: true },
        cued_scene_id: Some("2:Missing".to_string()),
        scene_configs: vec![
            ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: ShowFileSceneScopeToggles::default(),
            },
            ShowFileSceneConfig {
                scene_index: 2,
                scene_name: "Missing".to_string(),
                duration_ms: 5_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: ShowFileSceneScopeToggles::default(),
            },
        ],
    };
    let lv1 = crate::lv1::types::Lv1StateSnapshot {
        connection: crate::lv1::types::ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![crate::lv1::types::SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: Vec::new(),
    };

    let imported = import_show_file(&mut file, &lv1).unwrap();

    assert!(imported.report.removed_anything());
    assert_eq!(imported.report.removed_scenes, vec!["2: Missing".to_string()]);
    assert_eq!(imported.snapshot.scene_configs.len(), 1);
    assert_eq!(imported.snapshot.scene_configs[0].scene_id, "1:Intro");
    assert_eq!(imported.snapshot.cued_scene_id, None);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo nextest run -p advanced-show-control show::show_file
```

Expected: FAIL to compile because `src-tauri/src/show/show_file.rs` and the exported functions do not exist yet.

- [ ] **Step 3: Move DTOs and mapping into `show/show_file.rs`**

Create `src-tauri/src/show/show_file.rs` by moving these DTOs and validation types from `src-tauri/src/show_file.rs`:

```rust
pub const SHOW_FILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFile { ... }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSafety { pub lockout: bool }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneConfig { ... }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct ShowFileSceneScopeToggles { pub faders: bool, pub pan: bool }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelConfig { ... }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelRef { pub group: i32, pub channel: i32 }

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadValidationReport { pub removed_scenes: Vec<String> }
```

Use the full field definitions exactly as they exist in `src-tauri/src/show_file.rs`; do not change serialized field names or schema version.

Add these show-owned mapping APIs:

```rust
pub struct ImportedShowFile {
    pub snapshot: crate::show::types::ShowSnapshot,
    pub selected_scene_id: Option<String>,
    pub report: LoadValidationReport,
}

pub fn export_show_file(snapshot: crate::show::types::ShowSnapshot, saved_at: String) -> ShowFile {
    ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at,
        safety: ShowFileSafety { lockout: snapshot.lockout },
        cued_scene_id: snapshot.cued_scene_id,
        scene_configs: snapshot.scene_configs.into_iter().map(show_scene_to_file_scene).collect(),
    }
}

pub fn import_show_file(
    file: &mut ShowFile,
    lv1: &crate::lv1::types::Lv1StateSnapshot,
) -> Result<ImportedShowFile, String> {
    let report = prune_show_file_to_lv1_scenes(file, lv1)?;
    let kept_scene_ids = file
        .scene_configs
        .iter()
        .map(|config| crate::show::types::scene_id(config.scene_index, &config.scene_name))
        .collect::<std::collections::HashSet<_>>();
    let selected_scene_id = file
        .scene_configs
        .first()
        .map(|config| crate::show::types::scene_id(config.scene_index, &config.scene_name));
    let snapshot = crate::show::types::ShowSnapshot {
        lockout: file.safety.lockout,
        scene_configs: file.scene_configs.iter().map(file_scene_to_show_scene).collect(),
        cued_scene_id: file
            .cued_scene_id
            .clone()
            .filter(|scene_id| kept_scene_ids.contains(scene_id)),
    };

    Ok(ImportedShowFile { snapshot, selected_scene_id, report })
}
```

Implement private `show_scene_to_file_scene` and `file_scene_to_show_scene` helpers by moving the existing mapping code from `ShellState::{export_show_file_for_save,load_show_file_from_dto}`.

- [ ] **Step 4: Export module and update old imports**

In `src-tauri/src/show/mod.rs`, add:

```rust
pub mod show_file;
```

In `src-tauri/src/show_file.rs`, remove the DTO/pruning definitions and import DTOs from `crate::show::show_file` for `read_show_file` and `write_show_file`:

```rust
use crate::show::show_file::ShowFile;
```

Update existing imports in tests and code from `crate::show_file::{ShowFile, ...}` to `crate::show::show_file::{ShowFile, ...}`. Keep filesystem helpers imported from `crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file}`.

- [ ] **Step 5: Make ShellState mapping methods delegate to show-owned mapping temporarily**

In `src-tauri/src/app_state/show_file_mapping.rs`, replace inline DTO mapping with calls to `crate::show::show_file::{export_show_file, import_show_file}`:

```rust
pub async fn export_show_file_for_save(&self, saved_at: String) -> Result<crate::show::show_file::ShowFile, String> {
    let show = self.show.get_snapshot().await;
    Ok(crate::show::show_file::export_show_file(show, saved_at))
}
```

For `load_show_file_from_dto`, keep path/file metadata in `ShellState`, but use `import_show_file(file, &lv1)?` for snapshot, selected scene id, and validation report.

- [ ] **Step 6: Run mapping tests**

Run:

```bash
cargo nextest run -p advanced-show-control show::show_file app_state::show_file_mapping_tests
```

Expected: PASS. The old shell mapping tests may still exist temporarily, but pure mapping/pruning behavior should now be covered in `show::show_file`.

- [ ] **Step 7: Commit Task 5**

Run:

```bash
git add src-tauri/src/show/show_file.rs src-tauri/src/show/mod.rs src-tauri/src/show_file.rs src-tauri/src/app_state/show_file_mapping.rs src-tauri/src/app_state/show_file_mapping_tests.rs
git commit -m "refactor: move show file mapping into show"
```

---

### Task 6: Add Show-File Command-Bus Methods

**Files:**
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Test: `src-tauri/src/runtime/commands.rs`

**Interfaces:**
- Consumes: `crate::show::show_file::{ShowFile, export_show_file, import_show_file}` and `ShowStateHandle::{clear,reconcile_scene_list,replace_snapshot,get_snapshot}`.
- Produces: `AppCommandBus::{new_show_file, export_show_file_for_save, load_show_file_from_dto}` and show-owned command helpers with result values that include selected-scene id and validation report where needed.

- [ ] **Step 1: Add failing command-bus tests for show-file commands**

In `src-tauri/src/runtime/commands.rs` tests, add:

```rust
#[tokio::test]
async fn new_show_file_routes_through_show_state_and_reconciles_lv1_scenes() {
    let bus = AppCommandBus::new();
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus);
    show.replace_snapshot(ShowSnapshot { lockout: true, scene_configs: vec![scene_config()], cued_scene_id: Some("1:Intro".to_string()) }).await;
    bus.set_show(Some(show)).await;
    let lv1 = Lv1StateSnapshot {
        connection: crate::lv1::types::ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![crate::lv1::types::SceneListEntry { index: 2, name: "Verse".to_string() }],
        channels: Vec::new(),
    };

    let result = bus.new_show_file(Some(lv1)).await.unwrap();

    assert_eq!(result.selected_scene_id, Some("2:Verse".to_string()));
    let snapshot = bus.get_show_snapshot().await.unwrap();
    assert!(!snapshot.lockout);
    assert_eq!(snapshot.scene_configs[0].scene_id, "2:Verse");
}

#[tokio::test]
async fn export_show_file_for_save_routes_through_show_state() {
    let snapshot = ShowSnapshot { scene_configs: vec![scene_config()], ..ShowSnapshot::empty() };
    let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

    let file = bus.export_show_file_for_save("saved".to_string()).await.unwrap();

    assert_eq!(file.saved_at, "saved");
    assert_eq!(file.scene_configs[0].scene_name, "Intro");
}

#[tokio::test]
async fn load_show_file_from_dto_routes_through_show_state() {
    let bus = AppCommandBus::new();
    let event_bus = AppEventBus::default();
    bus.set_show(Some(ShowStateHandle::new_empty(event_bus))).await;
    let lv1 = Lv1StateSnapshot {
        connection: crate::lv1::types::ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![crate::lv1::types::SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: Vec::new(),
    };
    let mut file = crate::show::show_file::ShowFile {
        schema_version: crate::show::show_file::SHOW_FILE_SCHEMA_VERSION,
        app_version: "0.1.0".to_string(),
        saved_at: "saved".to_string(),
        safety: crate::show::show_file::ShowFileSafety { lockout: true },
        cued_scene_id: None,
        scene_configs: vec![crate::show::show_file::ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: crate::show::show_file::ShowFileSceneScopeToggles::default(),
        }],
    };

    let result = bus.load_show_file_from_dto(&mut file, lv1).await.unwrap();

    assert_eq!(result.selected_scene_id, Some("1:Intro".to_string()));
    assert!(!result.report.removed_anything());
    assert!(bus.get_show_snapshot().await.unwrap().lockout);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::commands::tests::new_show_file_routes_through_show_state_and_reconciles_lv1_scenes runtime::commands::tests::load_show_file_from_dto_routes_through_show_state
```

Expected: FAIL to compile because the new command-bus methods and result types do not exist yet.

- [ ] **Step 3: Add show-owned show-file command helpers**

In `src-tauri/src/show/commands.rs`, add result types:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShowFileResult {
    pub selected_scene_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadShowFileResult {
    pub selected_scene_id: Option<String>,
    pub saved_at: String,
    pub report: crate::show::show_file::LoadValidationReport,
}
```

Add helper functions:

```rust
pub async fn new_show_file(
    show: &ShowStateHandle,
    lv1: Option<crate::lv1::types::Lv1StateSnapshot>,
) -> Result<NewShowFileResult, String> {
    show.clear().await;
    if let Some(lv1) = lv1 {
        if !lv1.scene_list.is_empty() {
            show.reconcile_scene_list(lv1.scene_list).await;
        }
    }
    let selected_scene_id = show.get_snapshot().await.scene_configs.first().map(|scene| scene.scene_id.clone());
    Ok(NewShowFileResult { selected_scene_id })
}

pub async fn export_show_file_for_save(
    show: &ShowStateHandle,
    saved_at: String,
) -> crate::show::show_file::ShowFile {
    crate::show::show_file::export_show_file(show.get_snapshot().await, saved_at)
}

pub async fn load_show_file_from_dto(
    show: &ShowStateHandle,
    file: &mut crate::show::show_file::ShowFile,
    lv1: crate::lv1::types::Lv1StateSnapshot,
) -> Result<LoadShowFileResult, String> {
    let imported = crate::show::show_file::import_show_file(file, &lv1)?;
    show.replace_snapshot(imported.snapshot).await;
    Ok(LoadShowFileResult {
        selected_scene_id: imported.selected_scene_id,
        saved_at: file.saved_at.clone(),
        report: imported.report,
    })
}
```

- [ ] **Step 4: Add `AppCommandBus` show-file methods**

In `src-tauri/src/runtime/commands.rs`, import the result types and add methods:

```rust
pub async fn new_show_file(
    &self,
    lv1: Option<Lv1StateSnapshot>,
) -> Result<crate::show::commands::NewShowFileResult, AppCommandError> {
    let show = self.show_target().await?;
    crate::show::commands::new_show_file(&show, lv1)
        .await
        .map_err(AppCommandError::CommandFailed)
}

pub async fn export_show_file_for_save(
    &self,
    saved_at: String,
) -> Result<crate::show::show_file::ShowFile, AppCommandError> {
    let show = self.show_target().await?;
    Ok(crate::show::commands::export_show_file_for_save(&show, saved_at).await)
}

pub async fn load_show_file_from_dto(
    &self,
    file: &mut crate::show::show_file::ShowFile,
    lv1: Lv1StateSnapshot,
) -> Result<crate::show::commands::LoadShowFileResult, AppCommandError> {
    let show = self.show_target().await?;
    crate::show::commands::load_show_file_from_dto(&show, file, lv1)
        .await
        .map_err(AppCommandError::CommandFailed)
}
```

- [ ] **Step 5: Run targeted tests**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::commands::tests show::show_file
```

Expected: PASS.

- [ ] **Step 6: Commit Task 6**

Run:

```bash
git add src-tauri/src/show/commands.rs src-tauri/src/runtime/commands.rs
git commit -m "refactor: add show file commands to command bus"
```

---

### Task 7: Route Tauri Show-File Commands Through Command Bus

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Test: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: Task 6 `AppCommandBus` methods and transitional `ShellState` file metadata fields.
- Produces: `new_show_file`, `open_show_file_dialog`, `save_show_file`, and `save_show_file_as_dialog` command handlers that route show-file mutation/mapping through `AppCommandBus` while preserving snapshot returns and direct emits.

- [ ] **Step 1: Add ShellState metadata-only helpers**

Move the metadata parts of current `load_show_file_from_dto`, `new_show_file`, and `mark_show_file_saved` into small ShellState helpers:

```rust
pub async fn lv1_snapshot_required_for_show_file(&self) -> Result<Lv1StateSnapshot, String> {
    self.lv1_snapshot()
        .await
        .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())
}

pub async fn apply_new_show_file_metadata(&self, selected_scene_id: Option<String>) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.selected_scene_id = selected_scene_id;
    inner.show_file_path = None;
    inner.show_file_dirty = false;
    inner.show_file_last_saved_at = None;
    drop(inner);
    tracing::info!(event = "show_file_created", "New show file created");
    self.snapshot().await
}

pub async fn apply_loaded_show_file_metadata(
    &self,
    path: PathBuf,
    selected_scene_id: Option<String>,
    saved_at: String,
    dirty: bool,
) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.selected_scene_id = selected_scene_id;
    inner.show_file_path = Some(path);
    inner.show_file_last_saved_at = Some(saved_at);
    inner.show_file_dirty = dirty;
    drop(inner);
    tracing::info!(event = "show_file_opened", "Show file loaded");
    self.snapshot().await
}
```

Keep `current_show_file_path` and `mark_show_file_saved` as metadata helpers. Remove old `ShellState::export_show_file_for_save`, `ShellState::load_show_file_from_dto`, and `ShellState::new_show_file` once no call sites remain.

- [ ] **Step 2: Route `new_show_file` command**

In `src-tauri/src/commands.rs`, change `new_show_file` to accept `lifecycle: State<'_, AppLifecycle>` and use:

```rust
let command_bus = current_command_bus(lifecycle.command_bus_holder(), "new_show_file").await?;
let lv1 = state.lv1_snapshot().await;
let result = command_bus
    .new_show_file(lv1)
    .await
    .map_err(map_app_command_error)?;
let snapshot = state.apply_new_show_file_metadata(result.selected_scene_id).await;
emit_snapshot(&app, &snapshot);
Ok(snapshot)
```

- [ ] **Step 3: Route open/load command**

In `open_show_file_dialog`, after `read_show_file(&path)?`, use:

```rust
let command_bus = current_command_bus(lifecycle.command_bus_holder(), "open_show_file_dialog").await?;
let lv1 = state.lv1_snapshot_required_for_show_file().await?;
let result = command_bus
    .load_show_file_from_dto(&mut file, lv1)
    .await
    .map_err(map_app_command_error)?;
for scene in result.report.removed_scenes.iter() {
    tracing::warn!(
        event = "show_file_scene_pruned",
        scene = %scene,
        "Skipped loading \"{scene}\" because it was not found in the current scene list."
    );
}
let snapshot = state
    .apply_loaded_show_file_metadata(
        path,
        result.selected_scene_id,
        result.saved_at,
        result.report.removed_anything(),
    )
    .await;
emit_snapshot(&app, &snapshot);
Ok(snapshot)
```

Update the command signature to include `lifecycle: State<'_, AppLifecycle>`.

- [ ] **Step 4: Route save/export commands**

Update `save_show_file_to_path` to accept `active_command_bus: ActiveCommandBus` and use:

```rust
let saved_at = crate::time::current_timestamp_millis();
let command_bus = current_command_bus(active_command_bus, "save_show_file").await?;
let file = command_bus
    .export_show_file_for_save(saved_at.clone())
    .await
    .map_err(map_app_command_error)?;
write_show_file(&path, &file, &backup_folder())?;
Ok(state.mark_show_file_saved(path, saved_at).await)
```

Update `save_show_file` and `save_show_file_as_dialog` to pass `lifecycle.command_bus_holder()` into `save_show_file_to_path`. In `save_show_file_as_dialog`, replace the preflight `state.export_show_file_for_save(String::new()).await?` with:

```rust
let command_bus = current_command_bus(lifecycle.command_bus_holder(), "save_show_file_as_dialog").await?;
command_bus
    .export_show_file_for_save(String::new())
    .await
    .map_err(map_app_command_error)?;
```

- [ ] **Step 5: Update tests and command exposure checks**

Update direct command tests to pass lifecycle state for `new_show_file`, `open_show_file_dialog`, `save_show_file`, and `save_show_file_as_dialog` where they call functions directly. Add a helper equivalent to Task 3 `lifecycle_with_show` and install `state.show.clone()` into the bus.

Add a command test proving missing active bus blocks show-file commands:

```rust
#[tokio::test]
async fn new_show_file_requires_active_command_bus() {
    let state = ShellState::default();
    let lifecycle = AppLifecycle::default();
    let err = new_show_file_snapshot_for_test(state, lifecycle)
        .await
        .unwrap_err();

    assert_eq!(err, "App command bus is unavailable");
}
```

If no test-only helper exists for `new_show_file`, add a private helper mirroring the command body so the test does not need a Tauri `AppHandle`.

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo check --workspace --all-targets
cargo nextest run -p advanced-show-control commands::tests app_state::show_file_mapping_tests runtime::commands::tests show::show_file
```

Expected: PASS. If `app_state::show_file_mapping_tests` has become only metadata/shell tests, rename or reduce it; do not leave duplicate mapping coverage in app_state after the mapping moves to `show/`.

- [ ] **Step 7: Commit Task 7**

Run:

```bash
git add src-tauri/src/commands.rs src-tauri/src/app_state/show_file_mapping.rs src-tauri/src/app_state/shell.rs src-tauri/src/app_state/show_file_mapping_tests.rs
git commit -m "refactor: route show file commands through command bus"
```

---

### Task 8: Documentation And Full Verification For Phases 8-10

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/plans/2026-06-19-route-show-and-show-file-commands.md` only if execution discovered plan corrections

**Interfaces:**
- Consumes: Tasks 1-7 behavior.
- Produces: docs reflecting that phases 8-10 are complete and later recall/projector/frontend cleanup remains pending.

- [ ] **Step 1: Update architecture docs**

In `docs/architecture.md`, update the `Bus Contracts`, `Command Flow`, and `File Structure` sections to state:

```markdown
Low-risk show/app mutations and show-file import/export mapping route through `AppCommandBus`. The `show/` module owns show-file DTOs, schema version, import/export mapping, pruning, and validation against LV1 scene snapshots. The Tauri adapter still owns native dialogs and filesystem read/write plumbing, and it still returns/directly emits `AppViewState` snapshots until the projector-only and frontend command-contract phases remove that temporary behavior.
```

Keep wording explicit that UI-requested recall, projector cache, logging projection, React command-result cleanup, `ShellState` removal, and `ActiveCommandBus` removal are still pending later phases.

- [ ] **Step 2: Run focused checks**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control runtime::commands::tests commands::tests show::show_file app_state::show_file_mapping_tests
```

Expected: PASS.

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo nextest run --workspace
cargo build --workspace
cargo build -p advanced-show-control --bin lv1-probe
npm --prefix ui run typecheck
npm run tauri -- build
```

Expected: PASS. The known non-fatal Tauri warning about bundle identifier ending with `.app` may still appear and does not fail the build.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
git status --short
```

Expected: Only intended phase-8-to-10 files are modified. No frontend files should change unless `npm` tooling changed lockfiles unexpectedly; do not commit unrelated generated changes.

- [ ] **Step 5: Commit Task 8**

Run:

```bash
git add docs/architecture.md
git commit -m "docs: describe routed show file boundary"
```

---

## Self-Review Checklist

- Spec coverage: phases 8-10 are covered by Tasks 1-8; later phases 11-17 are explicitly out of scope.
- Event boundary: covered show/app mutations still publish through `ShowStateHandle`, not `AppCommandBus`.
- Show-file boundary: DTOs, schema version, mapping, pruning, and validation move into `show/`; Tauri keeps dialogs and filesystem IO.
- Projection boundary: direct emits and command-return snapshots are intentionally preserved in Task 2.
- Safety boundary: UI-requested recall, scene recall automation, fade behavior, LV1 writes, and generation guards are not changed.
- Type consistency: `ShowCommandResult { changed: bool }` and `CueSceneResult { changed: bool, scene: SceneConfig }` are defined before use by `AppCommandBus` and Tauri command tests.
- Verification: targeted command tests plus full Rust/frontend/Tauri checks are required before completion.
