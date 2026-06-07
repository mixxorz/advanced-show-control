# Phase 7 Scene Recall Fader Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically start safe scoped fader fades when LV1 recalls an app-managed scene with nonzero duration.

**Architecture:** Add a dedicated `SceneRecallFader` runtime task that listens for `Lv1Event::SceneChanged`, asks `ShellState` to validate/build a fade request, then uses `AppCommandBus` to abort any previous fade and start the new one. Keep validation in the app-state layer because it owns scene configs, lockout, logs, and the current LV1 projection.

**Tech Stack:** Rust, Tokio actors/channels, existing `AppEventBus`, `AppCommandBus`, Tauri shell state, `cargo test`, `npm`/Vite only for final UI type-check if needed.

---

## File Structure

- Create `src-tauri/src/app_state/scene_recall.rs`: validation and logging for scene recall fade requests. Defines `SceneRecallDecision` and `SceneRecallFadeRequest` for the Tauri app-state crate.
- Create `src-tauri/src/app_state/scene_recall_tests.rs`: unit tests for validation and logs using existing `ShellState` helpers.
- Modify `src-tauri/src/app_state/mod.rs`: register the new app-state module and tests.
- Create `src-tauri/src/scene_recall_fader.rs`: generation-scoped task that subscribes to `AppEventBus`, handles scene changes, aborts existing fades after validation, and starts the new fade.
- Modify `src-tauri/src/main.rs`: register the new module.
- Modify `src-tauri/src/app_state/shell.rs`: add a runtime handle for `SceneRecallFader` so reconnect/disconnect cleanup aborts it.
- Modify `src-tauri/src/commands.rs`: spawn and install `SceneRecallFader` during `connect_lv1`.
- Modify `PHASES.md`: mark Phase 7 complete after implementation and tests pass.

---

### Task 1: Add Scene Recall Validation Types And First Passing Request

**Files:**
- Create: `src-tauri/src/app_state/scene_recall.rs`
- Create: `src-tauri/src/app_state/scene_recall_tests.rs`
- Modify: `src-tauri/src/app_state/mod.rs`

- [ ] **Step 1: Register the new modules**

Patch `src-tauri/src/app_state/mod.rs`:

```rust
mod capture;
#[cfg(test)]
mod capture_tests;
mod events;
#[cfg(test)]
mod events_tests;
mod scene_recall;
#[cfg(test)]
mod scene_recall_tests;
mod shell;
mod show_file_mapping;
#[cfg(test)]
mod show_file_mapping_tests;
#[cfg(test)]
mod test_support;
mod view;

pub use scene_recall::{SceneRecallDecision, SceneRecallFadeRequest};
pub use shell::RuntimeHandles;
pub use shell::ShellState;
pub use view::AppViewState;
```

- [ ] **Step 2: Write the failing validation test**

Create `src-tauri/src/app_state/scene_recall_tests.rs`:

```rust
use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::lv1::model::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneState};

use super::scene_recall::SceneRecallDecision;
use super::shell::ShellState;
use super::test_support::scene_config;
use super::view::{ChannelConfig, ChannelRef, LogSeverity, LogSource};

#[tokio::test]
async fn configured_nonzero_scene_builds_fade_request() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    match decision {
        SceneRecallDecision::Start(request) => {
            assert_eq!(request.scene_id, "1::Intro");
            assert_eq!(request.scene_label, "1: Intro");
            assert_eq!(request.fade_config.duration_ms, 4_000);
            assert_eq!(request.fade_config.curve, FadeCurve::Linear);
            assert_eq!(request.fade_config.targets.len(), 1);
            assert_eq!(request.fade_config.targets[0].group, 0);
            assert_eq!(request.fade_config.targets[0].channel, 2);
            assert_eq!(request.fade_config.targets[0].target_db, -12.5);
        }
        other => panic!("unexpected decision: {other:?}"),
    }

    let snapshot = state.snapshot().await;
    assert!(snapshot.logs.iter().any(|log| {
        log.source == LogSource::App
            && log.severity == LogSeverity::Info
            && log.message == "Auto fade ready for scene 1: Intro with 1 target"
    }));
}

fn snapshot_for_intro() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: Some(SceneState {
            index: 1,
            name: "Intro".to_string(),
        }),
        scene_list: Vec::new(),
        channels: vec![ChannelInfo {
            group: 0,
            channel: 2,
            name: "Lead".to_string(),
            gain_db: -8.0,
            muted: false,
        }],
    }
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri configured_nonzero_scene_builds_fade_request`

Expected: FAIL because `scene_recall` module and `prepare_scene_recall_fade_for_generation` do not exist yet.

- [ ] **Step 4: Implement minimal validation for the happy path**

Create `src-tauri/src/app_state/scene_recall.rs`:

```rust
use std::collections::HashSet;

use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeTarget};
use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, SceneState};

use super::shell::{ShellState, scene_id};
use super::view::{LogSeverity, LogSource};

#[derive(Debug, Clone, PartialEq)]
pub struct SceneRecallFadeRequest {
    pub scene_id: String,
    pub scene_label: String,
    pub fade_config: FadeConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneRecallDecision {
    Start(SceneRecallFadeRequest),
    Skip,
    Blocked,
    StaleGeneration,
}

impl ShellState {
    pub async fn prepare_scene_recall_fade_for_generation(
        &self,
        generation: u64,
        recalled_scene: &SceneState,
    ) -> SceneRecallDecision {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return SceneRecallDecision::StaleGeneration;
        }

        let Some(snapshot) = inner.lv1_snapshot.as_ref() else {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: LV1 state is unavailable",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        };

        if snapshot.connection != ConnectionStatus::Connected {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: LV1 is not connected",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let Some(current_scene) = snapshot.scene.as_ref() else {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: current scene snapshot is unavailable",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        };

        if current_scene.index != recalled_scene.index || current_scene.name != recalled_scene.name {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: scene identity mismatch",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let id = scene_id(recalled_scene.index, &recalled_scene.name);
        let Some(config) = inner
            .scene_configs
            .iter()
            .find(|config| config.scene_id == id)
            .cloned()
        else {
            return SceneRecallDecision::Skip;
        };

        if config.duration_ms == 0 {
            inner.push_log(
                LogSource::App,
                LogSeverity::Info,
                format!(
                    "Auto fade skipped for scene {}: {}: duration is 0",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Skip;
        }

        if inner.lockout {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: lockout is enabled",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        if snapshot.channels.is_empty() {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: live channel snapshot is empty",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let live_channels = snapshot
            .channels
            .iter()
            .map(|channel| (channel.group, channel.channel))
            .collect::<HashSet<_>>();
        let mut targets = Vec::with_capacity(config.scoped_channels.len());

        for scoped in &config.scoped_channels {
            if !live_channels.contains(&(scoped.group, scoped.channel)) {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} is missing from live topology",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            }

            let Some(stored) = config
                .channel_configs
                .iter()
                .find(|entry| entry.group == scoped.group && entry.channel == scoped.channel)
            else {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} has no stored config",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            };

            let Some(target_db) = stored.fader_db else {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} has no stored fader value",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            };

            targets.push(FadeTarget {
                group: scoped.group,
                channel: scoped.channel,
                target_db,
            });
        }

        if targets.is_empty() {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: no scoped targets",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let scene_label = format!("{}: {}", recalled_scene.index, recalled_scene.name);
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!(
                "Auto fade ready for scene {scene_label} with {} target{}",
                targets.len(),
                if targets.len() == 1 { "" } else { "s" }
            ),
        );

        SceneRecallDecision::Start(SceneRecallFadeRequest {
            scene_id: id,
            scene_label,
            fade_config: FadeConfig {
                targets,
                duration_ms: config.duration_ms,
                curve: FadeCurve::Linear,
            },
        })
    }
}
```

- [ ] **Step 5: Run the test and verify it passes**

Run: `cargo test -p lv1-scene-fade-utility-tauri configured_nonzero_scene_builds_fade_request`

Expected: PASS.

- [ ] **Step 6: Commit validation happy path**

Run: `git status --short`

Expected: only `src-tauri/src/app_state/mod.rs`, `src-tauri/src/app_state/scene_recall.rs`, and `src-tauri/src/app_state/scene_recall_tests.rs` are changed for this task.

Run:

```bash
git add src-tauri/src/app_state/mod.rs src-tauri/src/app_state/scene_recall.rs src-tauri/src/app_state/scene_recall_tests.rs
git commit -m "feat: validate scene recall fade requests"
```

---

### Task 2: Complete Validation Safety Coverage

**Files:**
- Modify: `src-tauri/src/app_state/scene_recall_tests.rs`
- Modify: `src-tauri/src/app_state/scene_recall.rs`

- [ ] **Step 1: Add validation tests for skips and blocks**

Append to `src-tauri/src/app_state/scene_recall_tests.rs`:

```rust
#[tokio::test]
async fn duration_zero_skips_without_starting_fade() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        )];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Skip);
}

#[tokio::test]
async fn lockout_blocks_scene_recall_fade() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;
    state.set_lockout(true).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn missing_scene_config_skips() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Skip);
}

#[tokio::test]
async fn scene_identity_mismatch_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Renamed Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn missing_live_channel_snapshot_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState {
                index: 1,
                name: "Intro".to_string(),
            }),
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn scoped_channel_without_stored_fader_value_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: None,
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn scoped_channel_missing_from_live_topology_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 9,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 9 }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn stale_generation_is_ignored() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let (_next_generation, _) = state.disconnect().await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::StaleGeneration);
}
```

- [ ] **Step 2: Run validation tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_tests`

Expected: PASS. If any fail, adjust `prepare_scene_recall_fade_for_generation` only enough to match the specified decisions and logs.

- [ ] **Step 3: Add duration-zero duplicate log guard test**

Append to `src-tauri/src/app_state/scene_recall_tests.rs`:

```rust
#[tokio::test]
async fn duration_zero_skip_logs_once_per_generation_for_same_scene() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef { group: 0, channel: 2 }],
        )];
    }

    for _ in 0..2 {
        assert_eq!(
            state
                .prepare_scene_recall_fade_for_generation(
                    generation,
                    &SceneState {
                        index: 1,
                        name: "Intro".to_string(),
                    },
                )
                .await,
            SceneRecallDecision::Skip
        );
    }

    let snapshot = state.snapshot().await;
    let skip_logs = snapshot
        .logs
        .iter()
        .filter(|log| log.message == "Auto fade skipped for scene 1: Intro: duration is 0")
        .count();
    assert_eq!(skip_logs, 1);
}
```

- [ ] **Step 4: Implement duplicate duration-zero guard**

Modify `src-tauri/src/app_state/shell.rs`:

```rust
use std::collections::{HashSet, VecDeque};
```

Add to `ShellInner`:

```rust
pub(super) duration_zero_skip_logs: HashSet<String>,
```

Because `ShellInner` derives `Default`, `HashSet` will initialize automatically.

Modify the duration-zero branch in `src-tauri/src/app_state/scene_recall.rs`:

```rust
if config.duration_ms == 0 {
    if inner.duration_zero_skip_logs.insert(id.clone()) {
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!(
                "Auto fade skipped for scene {}: {}: duration is 0",
                recalled_scene.index, recalled_scene.name
            ),
        );
    }
    return SceneRecallDecision::Skip;
}
```

- [ ] **Step 5: Run validation tests again**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_tests`

Expected: PASS.

- [ ] **Step 6: Commit validation safety coverage**

Run: `git status --short`

Expected: only `src-tauri/src/app_state/scene_recall.rs`, `src-tauri/src/app_state/scene_recall_tests.rs`, and `src-tauri/src/app_state/shell.rs` are changed for this task.

Run:

```bash
git add src-tauri/src/app_state/scene_recall.rs src-tauri/src/app_state/scene_recall_tests.rs src-tauri/src/app_state/shell.rs
git commit -m "test: cover scene recall safety blocks"
```

---

### Task 3: Add SceneRecallFader Runtime Task

**Files:**
- Create: `src-tauri/src/scene_recall_fader.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write runtime task implementation**

Create `src-tauri/src/scene_recall_fader.rs`:

```rust
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use tokio::task::JoinHandle;

use crate::app_state::{SceneRecallDecision, ShellState};

pub fn spawn_scene_recall_fader(
    state: ShellState,
    generation: u64,
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
) -> JoinHandle<()> {
    let mut events = event_bus.subscribe();

    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    match state
                        .prepare_scene_recall_fade_for_generation(generation, &scene)
                        .await
                    {
                        SceneRecallDecision::Start(request) => {
                            if command_bus.abort_all_fades().await.is_ok() {
                                state
                                    .log_scene_recall_fader_info(format!(
                                        "Previous fade aborted before auto fade for scene {}",
                                        request.scene_label
                                    ))
                                    .await;
                            }

                            if command_bus.start_fade(request.fade_config).await.is_ok() {
                                state
                                    .log_scene_recall_fader_info(format!(
                                        "Auto fade started for scene {}",
                                        request.scene_label
                                    ))
                                    .await;
                            }
                        }
                        SceneRecallDecision::Skip
                        | SceneRecallDecision::Blocked
                        | SceneRecallDecision::StaleGeneration => {}
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("scene-recall-fader", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
```

- [ ] **Step 2: Add public logging helper to ShellState**

Append to `src-tauri/src/app_state/scene_recall.rs` inside `impl ShellState`:

```rust
pub async fn log_scene_recall_fader_info(&self, message: String) {
    let mut inner = self.inner.lock().await;
    inner.push_log(LogSource::App, LogSeverity::Info, message);
}
```

- [ ] **Step 3: Register module in Tauri crate**

Modify `src-tauri/src/main.rs`:

```rust
mod app_state;
mod commands;
mod scene_recall_fader;
mod show_file;
```

- [ ] **Step 4: Run compile check**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_tests`

Expected: PASS. This confirms the new task compiles.

- [ ] **Step 5: Commit SceneRecallFader task**

Run: `git status --short`

Expected: only `src-tauri/src/scene_recall_fader.rs`, `src-tauri/src/main.rs`, and `src-tauri/src/app_state/scene_recall.rs` are changed for this task.

Run:

```bash
git add src-tauri/src/scene_recall_fader.rs src-tauri/src/main.rs src-tauri/src/app_state/scene_recall.rs
git commit -m "feat: add scene recall fader task"
```

---

### Task 4: Install SceneRecallFader During Connect Lifecycle

**Files:**
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Add runtime handle slot**

Modify `RuntimeHandles` in `src-tauri/src/app_state/shell.rs`:

```rust
#[derive(Default)]
pub struct RuntimeHandles {
    pub active_generation: u64,
    pub lv1: Option<lv1_scene_fade_utility::lv1::state::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
    pub command_bus: Option<AppCommandBus>,
    pub projector: Option<JoinHandle<()>>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
}
```

Modify `RuntimeHandles::abort_all`:

```rust
pub async fn abort_all(&mut self) {
    if let Some(command_bus) = self.command_bus.clone() {
        command_bus.clear_targets().await;
    }
    if let Some(projector) = self.projector.take() {
        projector.abort();
    }
    if let Some(scene_recall_fader) = self.scene_recall_fader.take() {
        scene_recall_fader.abort();
    }
    self.active_generation = 0;
    self.lv1 = None;
    self.fade = None;
    self.command_bus = None;
}
```

- [ ] **Step 2: Spawn and store the fader task during connect**

Modify imports in `src-tauri/src/commands.rs`:

```rust
use crate::scene_recall_fader::spawn_scene_recall_fader;
```

Modify `connect_lv1` runtime handle creation in `src-tauri/src/commands.rs`:

```rust
let mut runtime_handles = RuntimeHandles {
    active_generation: 0,
    lv1: Some(lv1.clone()),
    fade: Some(fade),
    command_bus: Some(fade_command_bus.clone()),
    projector: None,
    scene_recall_fader: Some(spawn_scene_recall_fader(
        shell_state.clone(),
        generation,
        fade_command_bus.clone(),
        event_bus.clone(),
    )),
};
```

- [ ] **Step 3: Run command tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri commands::tests`

Expected: PASS. If struct literal tests fail, add `scene_recall_fader: None` to any `RuntimeHandles` literal in tests.

- [ ] **Step 4: Commit runtime installation**

Run: `git status --short`

Expected: only `src-tauri/src/app_state/shell.rs` and `src-tauri/src/commands.rs` are changed for this task.

Run:

```bash
git add src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs
git commit -m "feat: install scene recall fader runtime"
```

---

### Task 5: Add Runtime Behavior Tests For Abort-Then-Start

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Add runtime task tests**

Append to `src-tauri/src/scene_recall_fader.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::ShellState;
    use crate::app_state::shell::scene_id;
    use crate::app_state::view::{ChannelConfig, ChannelRef, SceneConfig};
    use lv1_scene_fade_utility::fade::engine::FadeEngineHandle;
    use lv1_scene_fade_utility::fade::types::FadeCommand;
    use lv1_scene_fade_utility::lv1::model::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneState};
    use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
    use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn valid_scene_recall_aborts_existing_fade_then_starts_new_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let (fade_tx, mut fade_rx) = mpsc::channel(8);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;

        tokio::spawn(async move {
            while let Some(command) = fade_rx.recv().await {
                match command {
                    FadeCommand::AbortAll { reply } => {
                        let _ = reply.send(Ok(()));
                    }
                    FadeCommand::StartFade { config, reply } => {
                        assert_eq!(config.duration_ms, 4_000);
                        assert_eq!(config.targets.len(), 1);
                        assert_eq!(config.targets[0].target_db, -12.5);
                        let _ = reply.send(Ok(()));
                        break;
                    }
                    FadeCommand::FinishNow { reply } => {
                        let _ = reply.send(Ok(()));
                    }
                }
            }
        });

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state.begin_connection(snapshot_for_intro()).await;
        install_intro_config(&state).await;

        let handle = spawn_scene_recall_fader(
            state.clone(),
            generation,
            command_bus,
            event_bus.clone(),
        );

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 1,
            name: "Intro".to_string(),
        })));

        tokio::time::timeout(std::time::Duration::from_millis(250), async {
            loop {
                let snapshot = state.snapshot().await;
                if snapshot
                    .logs
                    .iter()
                    .any(|log| log.message == "Auto fade started for scene 1: Intro")
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("scene recall fader should start fade");

        handle.abort();
    }

    #[tokio::test]
    async fn blocked_recall_does_not_abort_existing_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let (fade_tx, mut fade_rx) = mpsc::channel(8);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state.begin_connection(snapshot_for_intro()).await;
        state.set_lockout(true).await;
        install_intro_config(&state).await;

        let handle = spawn_scene_recall_fader(
            state.clone(),
            generation,
            command_bus,
            event_bus.clone(),
        );

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 1,
            name: "Intro".to_string(),
        })));

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(fade_rx.try_recv().is_err());

        handle.abort();
    }

    async fn install_intro_config(state: &ShellState) {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![SceneConfig {
            scene_id: scene_id(1, "Intro"),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 4_000,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            scoped_channels: vec![ChannelRef { group: 0, channel: 2 }],
        }];
    }

    fn snapshot_for_intro() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState {
                index: 1,
                name: "Intro".to_string(),
            }),
            scene_list: Vec::new(),
            channels: vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -8.0,
                muted: false,
            }],
        }
    }
}
```

- [ ] **Step 2: Make `FadeEngineHandle::new` available to Tauri crate tests if needed**

If the test cannot call `FadeEngineHandle::new`, modify `src/fade/engine.rs`:

```rust
impl FadeEngineHandle {
    pub fn new(tx: mpsc::Sender<FadeCommand>) -> Self {
        Self { tx }
    }
```

The constructor is currently `pub(crate)` in the core crate. The Tauri crate integration test needs a fake fade handle.

- [ ] **Step 3: Run runtime task tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_fader`

Expected: PASS.

- [ ] **Step 4: Commit runtime behavior tests**

Run: `git status --short`

Expected: only `src-tauri/src/scene_recall_fader.rs` and, if needed, `src/fade/engine.rs` are changed for this task.

Run:

```bash
git add src-tauri/src/scene_recall_fader.rs src/fade/engine.rs
git commit -m "test: cover scene recall fade runtime"
```

---

### Task 6: Final Phase 7 Verification And Phase Checklist

**Files:**
- Modify: `PHASES.md`

- [x] **Step 1: Run full Rust tests**

Run: `cargo test --workspace`

Expected: PASS for all Rust workspace tests.

- [x] **Step 2: Run frontend build/type check**

Run: `npm run build`

Expected: PASS. This catches any generated/type exposure issues even though Phase 7 is backend-heavy.

- [x] **Step 3: Mark Phase 7 complete**

Modify `PHASES.md` lines 14 and 20:

```markdown
- [x] **Phase 7: Scene Recall Automation** — `SceneRecallFader` automatically validates LV1 scene recall events, blocks unsafe recalls, skips duration `0` scenes, aborts overlapping fades only after a valid new recall, and starts scoped stored fader fades.
```

```markdown
**Immediate Next Build Order:** Phase 8/9 external control next.
```

- [x] **Step 4: Inspect git status and diff**

Run: `git status --short`

Expected: only intended files changed.

Run: `git diff -- PHASES.md IDEAS.md docs/superpowers/specs/2026-06-08-phase-7-scene-recall-fader-design.md docs/superpowers/plans/2026-06-08-phase-7-scene-recall-fader.md src-tauri/src/app_state/mod.rs src-tauri/src/app_state/scene_recall.rs src-tauri/src/app_state/scene_recall_tests.rs src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs src-tauri/src/main.rs src-tauri/src/scene_recall_fader.rs src/fade/engine.rs`

Expected: diff matches Phase 7, spec, plan, and `IDEAS.md` changes only.

- [x] **Step 5: Commit final Phase 7 docs and checklist**

Run:

```bash
git add PHASES.md IDEAS.md docs/superpowers/specs/2026-06-08-phase-7-scene-recall-fader-design.md docs/superpowers/plans/2026-06-08-phase-7-scene-recall-fader.md
git commit -m "docs: document phase 7 scene recall fader"
```

---

## Self-Review Notes

- Spec coverage: validation, safety blocks, generation guard, overlap policy, logs, runtime lifecycle, and tests are covered by Tasks 1-6.
- Scope control: Avid VENUE-style partial overlap is intentionally only in `IDEAS.md`; Phase 7 keeps the approved MVP abort-then-start policy.
- Commit behavior: each task includes an explicit commit step with intended files and message.

---
