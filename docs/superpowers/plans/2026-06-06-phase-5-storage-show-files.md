# Phase 5 Storage And Show Files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add portable `.lv1show` save/load with strict LV1 exact-match validation, native dialogs, dirty state, duration storage, and backup-on-save.

**Architecture:** Rust remains the source of truth. A new `src-tauri/src/show_file.rs` module owns JSON DTOs, validation/pruning, path helpers, file writing, and backups. `ShellState` integrates show-file metadata and dirty tracking, Tauri commands expose user actions, and React renders compact show-file controls.

**Tech Stack:** Rust 2024, Tauri 2, tokio, serde, serde_json, rfd native dialogs, dirs, React, TypeScript, Vite, Tailwind CSS 4.

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src-tauri/Cargo.toml` | Modify | Add `serde_json`, `dirs`, and `rfd` dependencies |
| `src-tauri/src/show_file.rs` | Create | Show-file DTOs, JSON parse/serialize, strict validation/pruning, paths, save/backups |
| `src-tauri/src/app_state.rs` | Modify | Add show-file metadata, `duration_ms`, `channel_name`, dirty tracking, state load/save hooks |
| `src-tauri/src/commands.rs` | Modify | Add `new_show_file`, `open_show_file_dialog`, `save_show_file`, `save_show_file_as_dialog` commands |
| `src-tauri/src/main.rs` | Modify | Register module and commands |
| `ui/src/types.ts` | Modify | Match new Rust DTO fields |
| `ui/src/App.tsx` | Modify | Render show-file controls, duration control, captured/current channel names |
| `PHASES.md` | Modify | Mark Phase 5 complete after verified implementation |

---

### Task 1: Show File DTOs And Strict Validation

**Files:**
- Create: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/show_file.rs`

- [ ] **Step 1: Write failing DTO and validation tests**

Add `src-tauri/src/show_file.rs` with only tests and placeholder imports at first:

```rust
use lv1_scene_fade_utility::lv1::state::{ChannelInfo, Lv1StateSnapshot, SceneListEntry};

#[cfg(test)]
mod tests {
    use super::*;

    fn lv1_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: lv1_scene_fade_utility::lv1::state::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![
                SceneListEntry { index: 1, name: "Intro".to_string() },
                SceneListEntry { index: 2, name: "Verse".to_string() },
            ],
            channels: vec![
                ChannelInfo { group: 0, channel: 1, name: "Kick".to_string(), gain_db: -5.0, muted: false },
                ChannelInfo { group: 0, channel: 2, name: "Lead".to_string(), gain_db: -8.0, muted: false },
            ],
        }
    }

    fn show_file() -> ShowFile {
        ShowFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: true },
            scene_fade_configs: vec![ShowFileSceneFadeConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                fade_enabled: true,
                duration_ms: 4000,
                fade_targets: vec![ShowFileFadeTarget {
                    group: 0,
                    channel: 2,
                    channel_name: "Lead".to_string(),
                    target_db: -12.5,
                    enabled: true,
                    updated_at: "456".to_string(),
                }],
            }],
        }
    }

    #[test]
    fn show_file_serializes_camel_case_json() {
        let json = serde_json::to_string_pretty(&show_file()).unwrap();

        assert!(json.contains("\"schemaVersion\": 1"));
        assert!(json.contains("\"sceneFadeConfigs\""));
        assert!(json.contains("\"durationMs\": 4000"));
        assert!(json.contains("\"channelName\": \"Lead\""));
    }

    #[test]
    fn validation_keeps_exact_scene_and_target_matches() {
        let report = validate_show_file(&mut show_file(), &lv1_snapshot()).unwrap();

        assert_eq!(report.removed_scenes.len(), 0);
        assert_eq!(report.removed_targets.len(), 0);
    }

    #[test]
    fn validation_deletes_scene_when_name_differs() {
        let mut file = show_file();
        file.scene_fade_configs[0].scene_name = "Renamed Intro".to_string();

        let report = validate_show_file(&mut file, &lv1_snapshot()).unwrap();

        assert!(file.scene_fade_configs.is_empty());
        assert_eq!(report.removed_scenes, vec!["1: Renamed Intro".to_string()]);
    }

    #[test]
    fn validation_deletes_target_when_channel_name_differs() {
        let mut file = show_file();
        file.scene_fade_configs[0].fade_targets[0].channel_name = "Vocal".to_string();

        let report = validate_show_file(&mut file, &lv1_snapshot()).unwrap();

        assert!(file.scene_fade_configs[0].fade_targets.is_empty());
        assert_eq!(report.removed_targets, vec!["Intro 0/2 Vocal".to_string()]);
    }

    #[test]
    fn validation_requires_scene_and_channel_lists() {
        let mut file = show_file();
        let mut snapshot = lv1_snapshot();
        snapshot.scene_list.clear();

        assert_eq!(
            validate_show_file(&mut file, &snapshot).unwrap_err(),
            "Open a show file after LV1 scenes and channels are loaded"
        );
    }
}
```

- [ ] **Step 2: Register the module and run the tests to verify failure**

Modify `src-tauri/src/main.rs`:

```rust
mod app_state;
mod commands;
mod show_file;
```

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file -- --nocapture`

Expected: FAIL because `ShowFile`, `ShowFileSafety`, `ShowFileSceneFadeConfig`, `ShowFileFadeTarget`, and `validate_show_file` are undefined.

- [ ] **Step 3: Implement DTOs and validation**

Replace the top of `src-tauri/src/show_file.rs` before the tests with:

```rust
use lv1_scene_fade_utility::lv1::state::{Lv1StateSnapshot, SceneListEntry};
use serde::{Deserialize, Serialize};

pub const SHOW_FILE_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_DURATION_MS: u64 = 4000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFile {
    pub schema_version: u32,
    pub app_version: String,
    pub saved_at: String,
    pub safety: ShowFileSafety,
    pub scene_fade_configs: Vec<ShowFileSceneFadeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSafety {
    pub lockout: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneFadeConfig {
    pub scene_index: i32,
    pub scene_name: String,
    pub fade_enabled: bool,
    pub duration_ms: u64,
    pub fade_targets: Vec<ShowFileFadeTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileFadeTarget {
    pub group: i32,
    pub channel: i32,
    pub channel_name: String,
    pub target_db: f64,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadValidationReport {
    pub removed_scenes: Vec<String>,
    pub removed_targets: Vec<String>,
}

impl LoadValidationReport {
    pub fn removed_anything(&self) -> bool {
        !self.removed_scenes.is_empty() || !self.removed_targets.is_empty()
    }
}

pub fn validate_show_file(
    file: &mut ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<LoadValidationReport, String> {
    if lv1.scene_list.is_empty() || lv1.channels.is_empty() {
        return Err("Open a show file after LV1 scenes and channels are loaded".to_string());
    }

    if file.schema_version != SHOW_FILE_SCHEMA_VERSION {
        return Err(format!("Unsupported show file schema version {}", file.schema_version));
    }

    let mut report = LoadValidationReport::default();
    let scenes = lv1.scene_list.clone();
    let channels = lv1.channels.clone();

    file.scene_fade_configs.retain_mut(|config| {
        let scene_matches = scenes.iter().any(|scene| exact_scene_match(scene, config));
        if !scene_matches {
            report
                .removed_scenes
                .push(format!("{}: {}", config.scene_index, config.scene_name));
            return false;
        }

        config.fade_targets.retain(|target| {
            let target_matches = channels.iter().any(|channel| {
                channel.group == target.group
                    && channel.channel == target.channel
                    && channel.name == target.channel_name
            });
            if !target_matches {
                report.removed_targets.push(format!(
                    "{} {}/{} {}",
                    config.scene_name, target.group, target.channel, target.channel_name
                ));
            }
            target_matches
        });

        true
    });

    Ok(report)
}

fn exact_scene_match(scene: &SceneListEntry, config: &ShowFileSceneFadeConfig) -> bool {
    scene.index == config.scene_index && scene.name == config.scene_name
}
```

- [ ] **Step 4: Add dependencies and verify tests pass**

Modify `src-tauri/Cargo.toml` dependencies:

```toml
[dependencies]
dirs = "6"
lv1-scene-fade-utility = { path = ".." }
rfd = "0.15"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tauri = { version = "2", features = [] }
tokio = { version = "1", features = ["sync", "time", "rt-multi-thread", "macros"] }
```

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file -- --nocapture`

Expected: PASS for all show-file tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock Cargo.lock src-tauri/src/main.rs src-tauri/src/show_file.rs
git commit -m "feat: add show file validation"
```

---

### Task 2: App State Metadata, Duration, Channel Names, And Dirty Tracking

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Test: `src-tauri/src/app_state.rs`

- [ ] **Step 1: Write failing state tests**

Append these tests inside `src-tauri/src/app_state.rs` `mod tests`:

```rust
    #[tokio::test]
    async fn snapshot_exposes_untitled_show_file_state() {
        let state = ShellState::default();
        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.show_file_name, "Untitled Show");
        assert_eq!(snapshot.show_file_path, None);
        assert!(!snapshot.show_file_dirty);
        assert_eq!(snapshot.show_file_last_saved_at, None);
    }

    #[tokio::test]
    async fn captured_target_stores_channel_name_and_marks_dirty() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state.begin_connection(connected_state_with_scene_and_channel()).await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged { group: 0, channel: 2, gain_db: -4.5 },
            )
            .await;

        let snapshot = state.snapshot().await;
        let target = &snapshot.scene_fade_configs[0].fade_targets[0];
        assert_eq!(target.channel_name, "Lead");
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn new_scene_configs_default_to_four_second_duration() {
        let state = ShellState::default();
        let snapshot = state.begin_connection(connected_state_with_scene_and_channel()).await;

        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 4000);
    }

    #[tokio::test]
    async fn setting_duration_marks_show_dirty() {
        let state = ShellState::default();
        state.begin_connection(connected_state_with_scene_and_channel()).await;

        let snapshot = state
            .set_scene_duration_ms("1::Intro".to_string(), 6500)
            .await
            .unwrap();

        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 6500);
        assert!(snapshot.show_file_dirty);
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state -- --nocapture`

Expected: FAIL because show-file fields, `duration_ms`, `channel_name`, and `set_scene_duration_ms` are missing.

- [ ] **Step 3: Update serializable state types**

Modify the structs at the top of `src-tauri/src/app_state.rs`:

```rust
use crate::show_file::DEFAULT_DURATION_MS;
```

Change `FadeTarget`:

```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub channel_name: String,
    pub target_db: f64,
    pub enabled: bool,
    pub updated_at: String,
}
```

Change `SceneFadeConfig`:

```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFadeConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub fade_enabled: bool,
    pub duration_ms: u64,
    pub fade_targets: Vec<FadeTarget>,
}
```

Add fields to `AppViewState`:

```rust
    pub show_file_name: String,
    pub show_file_path: Option<String>,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
```

Add fields to `ShellInner`:

```rust
    show_file_path: Option<String>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
```

- [ ] **Step 4: Update state mutation code**

In every `SceneFadeConfig` literal, add:

```rust
duration_ms: DEFAULT_DURATION_MS,
```

In every `FadeTarget` literal in tests or implementation, add a suitable `channel_name`, usually:

```rust
channel_name: "Lead".to_string(),
```

Add dirty marking in mutation methods:

```rust
config.fade_enabled = enabled;
inner.show_file_dirty = true;
```

```rust
target.enabled = enabled;
inner.show_file_dirty = true;
```

After successful target removal:

```rust
inner.show_file_dirty = true;
```

Add duration setter to `impl ShellState`:

```rust
    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<AppViewState, String> {
        if !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be between 100 ms and 120000 ms".to_string());
        }

        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.duration_ms = duration_ms;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }
```

In `record_fader_target`, derive channel name before mutating configs:

```rust
        let channel_name = self.lv1_snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .channels
                .iter()
                .find(|ch| ch.group == group && ch.channel == channel)
                .map(|ch| ch.name.clone())
        });

        let Some(channel_name) = channel_name else {
            if self.unknown_fader_warnings.insert((group, channel)) {
                self.push_log(
                    LogSource::Lv1,
                    LogSeverity::Warning,
                    format!("Ignored fader target for unknown channel {group}/{channel}"),
                );
            }
            return;
        };
```

Set `channel_name` on new targets and mark dirty after insert/update:

```rust
                    channel_name: channel_name.clone(),
```

```rust
            self.show_file_dirty = true;
```

In `snapshot_from_inner`, compute show file name and include fields:

```rust
    let show_file_name = inner
        .show_file_path
        .as_ref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled Show")
        .to_string();
```

```rust
        show_file_name,
        show_file_path: inner.show_file_path.clone(),
        show_file_dirty: inner.show_file_dirty,
        show_file_last_saved_at: inner.show_file_last_saved_at.clone(),
```

- [ ] **Step 5: Run tests and fix all struct literals**

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state -- --nocapture`

Expected: PASS for app-state tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state.rs
git commit -m "feat: track show file state"
```

---

### Task 3: Convert Between App State And Show Files

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/show_file.rs`
- Test: `src-tauri/src/app_state.rs`

- [ ] **Step 1: Write failing conversion/load tests**

Append inside `app_state.rs` tests:

```rust
    #[tokio::test]
    async fn export_show_file_contains_current_configs() {
        let state = ShellState::default();
        state.begin_connection(connected_state_with_scene_and_channel()).await;
        state
            .set_scene_fade_enabled("1::Intro".to_string(), true)
            .await
            .unwrap();

        let file = state.export_show_file().await;

        assert_eq!(file.schema_version, 1);
        assert!(file.safety.lockout == false);
        assert_eq!(file.scene_fade_configs[0].scene_index, 1);
        assert_eq!(file.scene_fade_configs[0].duration_ms, 4000);
    }

    #[tokio::test]
    async fn load_show_file_applies_kept_configs_and_logs_pruned_entries() {
        let state = ShellState::default();
        state.begin_connection(connected_state_with_scene_and_channel()).await;
        let mut file = crate::show_file::ShowFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show_file::ShowFileSafety { lockout: true },
            scene_fade_configs: vec![
                crate::show_file::ShowFileSceneFadeConfig {
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    fade_enabled: true,
                    duration_ms: 5000,
                    fade_targets: vec![crate::show_file::ShowFileFadeTarget {
                        group: 0,
                        channel: 2,
                        channel_name: "Lead".to_string(),
                        target_db: -9.0,
                        enabled: true,
                        updated_at: "999".to_string(),
                    }],
                },
                crate::show_file::ShowFileSceneFadeConfig {
                    scene_index: 2,
                    scene_name: "Missing".to_string(),
                    fade_enabled: true,
                    duration_ms: 5000,
                    fade_targets: Vec::new(),
                },
            ],
        };

        let snapshot = state
            .load_show_file_from_dto("/tmp/test.lv1show".to_string(), &mut file)
            .await
            .unwrap();

        assert!(snapshot.lockout);
        assert_eq!(snapshot.scene_fade_configs.len(), 1);
        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 5000);
        assert_eq!(snapshot.scene_fade_configs[0].fade_targets[0].channel_name, "Lead");
        assert!(snapshot.show_file_dirty);
        assert!(snapshot.logs.iter().any(|entry| entry.message.contains("Deleted saved scene config during load: 2: Missing")));
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri export_show_file load_show_file -- --nocapture`

Expected: FAIL because conversion methods are missing.

- [ ] **Step 3: Implement conversion helpers**

In `app_state.rs`, import show-file DTOs:

```rust
use crate::show_file::{
    validate_show_file, ShowFile, ShowFileFadeTarget, ShowFileSafety, ShowFileSceneFadeConfig,
    SHOW_FILE_SCHEMA_VERSION,
};
```

Add methods to `impl ShellState`:

```rust
    pub async fn export_show_file(&self) -> ShowFile {
        let inner = self.inner.lock().await;
        show_file_from_inner(&inner)
    }

    pub async fn load_show_file_from_dto(
        &self,
        path: String,
        file: &mut ShowFile,
    ) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        if inner.listen_mode_active {
            return Err("Stop Listen Mode before opening a show file".to_string());
        }

        let lv1 = inner
            .lv1_snapshot
            .clone()
            .ok_or_else(|| "Open a show file after LV1 scenes and channels are loaded".to_string())?;
        let report = validate_show_file(file, &lv1)?;

        inner.lockout = file.safety.lockout;
        inner.scene_fade_configs = file
            .scene_fade_configs
            .iter()
            .map(scene_config_from_show_file)
            .collect();
        inner.selected_scene_id = inner.scene_fade_configs.first().map(|config| config.scene_id.clone());
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(file.saved_at.clone());
        inner.show_file_dirty = report.removed_anything();

        for scene in report.removed_scenes {
            inner.push_log(LogSource::App, LogSeverity::Warning, format!("Deleted saved scene config during load: {scene}"));
        }
        for target in report.removed_targets {
            inner.push_log(LogSource::App, LogSeverity::Warning, format!("Deleted saved fader target during load: {target}"));
        }
        inner.push_log(LogSource::App, LogSeverity::Info, "Show file loaded".to_string());

        Ok(snapshot_from_inner(&inner))
    }
```

Add helpers below `snapshot_from_inner`:

```rust
fn show_file_from_inner(inner: &ShellInner) -> ShowFile {
    ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: current_timestamp(),
        safety: ShowFileSafety { lockout: inner.lockout },
        scene_fade_configs: inner
            .scene_fade_configs
            .iter()
            .map(|config| ShowFileSceneFadeConfig {
                scene_index: config.scene_index,
                scene_name: config.scene_name.clone(),
                fade_enabled: config.fade_enabled,
                duration_ms: config.duration_ms,
                fade_targets: config
                    .fade_targets
                    .iter()
                    .map(|target| ShowFileFadeTarget {
                        group: target.group,
                        channel: target.channel,
                        channel_name: target.channel_name.clone(),
                        target_db: target.target_db,
                        enabled: target.enabled,
                        updated_at: target.updated_at.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn scene_config_from_show_file(config: &ShowFileSceneFadeConfig) -> SceneFadeConfig {
    SceneFadeConfig {
        scene_id: scene_id(config.scene_index, &config.scene_name),
        scene_index: config.scene_index,
        scene_name: config.scene_name.clone(),
        fade_enabled: config.fade_enabled,
        duration_ms: config.duration_ms,
        fade_targets: config
            .fade_targets
            .iter()
            .map(|target| FadeTarget {
                group: target.group,
                channel: target.channel,
                channel_name: target.channel_name.clone(),
                target_db: target.target_db,
                enabled: target.enabled,
                updated_at: target.updated_at.clone(),
            })
            .collect(),
    }
}
```

- [ ] **Step 4: Run conversion tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri export_show_file load_show_file -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app_state.rs src-tauri/src/show_file.rs
git commit -m "feat: load show files into app state"
```

---

### Task 4: File I/O, Platform Locations, Dialogs, And Backups

**Files:**
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/show_file.rs`

- [ ] **Step 1: Write failing file I/O tests**

Append inside `show_file.rs` tests:

```rust
    #[test]
    fn save_show_file_writes_json_and_creates_backup_on_overwrite() {
        let root = std::env::temp_dir().join(format!("lv1-show-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("test.lv1show");
        let backups = root.join("backups");

        write_show_file(&path, &show_file(), &backups).unwrap();
        write_show_file(&path, &show_file(), &backups).unwrap();

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("sceneFadeConfigs"));
        let backup_count = std::fs::read_dir(&backups).unwrap().count();
        assert_eq!(backup_count, 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_show_file_parses_json() {
        let root = std::env::temp_dir().join(format!("lv1-show-read-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("test.lv1show");

        std::fs::write(&path, serde_json::to_string(&show_file()).unwrap()).unwrap();
        let parsed = read_show_file(&path).unwrap();

        assert_eq!(parsed.scene_fade_configs[0].scene_name, "Intro");
        let _ = std::fs::remove_dir_all(&root);
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file -- --nocapture`

Expected: FAIL because `write_show_file` and `read_show_file` are missing.

- [ ] **Step 3: Implement file I/O and path helpers**

Add to `show_file.rs`:

```rust
use std::path::{Path, PathBuf};

pub fn read_show_file(path: &Path) -> Result<ShowFile, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("Failed to read show file: {err}"))?;
    serde_json::from_str(&text).map_err(|err| format!("Failed to parse show file JSON: {err}"))
}

pub fn write_show_file(path: &Path, file: &ShowFile, backup_dir: &Path) -> Result<(), String> {
    if path.exists() {
        backup_existing_show_file(path, backup_dir)?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("Failed to create show file folder: {err}"))?;
    }

    let temp_path = path.with_extension("lv1show.tmp");
    let json = serde_json::to_string_pretty(file).map_err(|err| format!("Failed to serialize show file: {err}"))?;
    std::fs::write(&temp_path, json).map_err(|err| format!("Failed to write show file: {err}"))?;
    std::fs::rename(&temp_path, path).map_err(|err| format!("Failed to replace show file: {err}"))?;
    Ok(())
}

pub fn default_show_folder() -> PathBuf {
    docs_dir().join("LV1 Scene Fade Utility")
}

pub fn backup_folder() -> PathBuf {
    app_data_dir().join("backups")
}

fn docs_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LV1 Scene Fade Utility")
}

fn backup_existing_show_file(path: &Path, backup_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(backup_dir).map_err(|err| format!("Failed to create backup folder: {err}"))?;
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("show");
    let timestamp = current_millis_string();
    let mut backup_path = backup_dir.join(format!("{timestamp}-{stem}.lv1show"));
    let mut suffix = 1;
    while backup_path.exists() {
        backup_path = backup_dir.join(format!("{timestamp}-{stem}-{suffix}.lv1show"));
        suffix += 1;
    }
    std::fs::copy(path, &backup_path).map_err(|err| format!("Failed to create show file backup: {err}"))?;
    Ok(())
}

fn current_millis_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
```

- [ ] **Step 4: Add state save/new helpers**

In `app_state.rs`, add methods:

```rust
    pub async fn new_show_file(&self) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        if inner.listen_mode_active {
            return Err("Stop Listen Mode before creating a new show file".to_string());
        }
        inner.scene_fade_configs.clear();
        inner.selected_scene_id = None;
        inner.show_file_path = None;
        inner.show_file_dirty = false;
        inner.show_file_last_saved_at = None;
        let scenes = inner.lv1_snapshot.as_ref().map(|snapshot| snapshot.scene_list.clone()).unwrap_or_default();
        inner.reconcile_scene_fade_configs(&scenes);
        inner.push_log(LogSource::App, LogSeverity::Info, "New show file created".to_string());
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn current_show_file_path(&self) -> Option<String> {
        self.inner.lock().await.show_file_path.clone()
    }

    pub async fn mark_show_file_saved(&self, path: String, saved_at: String) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(saved_at);
        inner.show_file_dirty = false;
        inner.push_log(LogSource::App, LogSeverity::Info, "Show file saved".to_string());
        snapshot_from_inner(&inner)
    }
```

- [ ] **Step 5: Add commands and register them**

In `commands.rs`, import:

```rust
use std::path::PathBuf;
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};
```

Add commands:

```rust
#[tauri::command]
pub async fn new_show_file(app: AppHandle, state: State<'_, ShellState>) -> Result<AppViewState, String> {
    let snapshot = state.new_show_file().await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn open_show_file_dialog(app: AppHandle, state: State<'_, ShellState>) -> Result<AppViewState, String> {
    let path = pick_show_file_path().await?.ok_or_else(|| "Open show file cancelled".to_string())?;
    let mut file = read_show_file(&path)?;
    let snapshot = state
        .load_show_file_from_dto(path.to_string_lossy().to_string(), &mut file)
        .await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn save_show_file(app: AppHandle, state: State<'_, ShellState>) -> Result<AppViewState, String> {
    if let Some(path) = state.current_show_file_path().await {
        save_show_file_to_path(app, state, PathBuf::from(path)).await
    } else {
        save_show_file_as_dialog(app, state).await
    }
}

#[tauri::command]
pub async fn save_show_file_as_dialog(app: AppHandle, state: State<'_, ShellState>) -> Result<AppViewState, String> {
    let path = save_show_file_path().await?.ok_or_else(|| "Save show file cancelled".to_string())?;
    save_show_file_to_path(app, state, path).await
}

async fn save_show_file_to_path(app: AppHandle, state: State<'_, ShellState>, path: PathBuf) -> Result<AppViewState, String> {
    let file = state.export_show_file().await;
    let saved_at = file.saved_at.clone();
    write_show_file(&path, &file, &backup_folder())?;
    let snapshot = state.mark_show_file_saved(path.to_string_lossy().to_string(), saved_at).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn pick_show_file_path() -> Result<Option<PathBuf>, String> {
    tokio::task::spawn_blocking(|| {
        std::fs::create_dir_all(default_show_folder()).map_err(|err| format!("Failed to create show file folder: {err}"))?;
        Ok(rfd::FileDialog::new()
            .set_directory(default_show_folder())
            .add_filter("LV1 Show", &["lv1show"])
            .pick_file())
    })
    .await
    .map_err(|err| format!("Open dialog failed: {err}"))?
}

async fn save_show_file_path() -> Result<Option<PathBuf>, String> {
    tokio::task::spawn_blocking(|| {
        std::fs::create_dir_all(default_show_folder()).map_err(|err| format!("Failed to create show file folder: {err}"))?;
        Ok(rfd::FileDialog::new()
            .set_directory(default_show_folder())
            .add_filter("LV1 Show", &["lv1show"])
            .set_file_name("Untitled.lv1show")
            .save_file())
    })
    .await
    .map_err(|err| format!("Save dialog failed: {err}"))?
}
```

In `main.rs`, register commands:

```rust
            commands::new_show_file,
            commands::open_show_file_dialog,
            commands::save_show_file,
            commands::save_show_file_as_dialog,
```

- [ ] **Step 6: Run Rust tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri show_file app_state -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/show_file.rs src-tauri/src/app_state.rs src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: save and open show files"
```

---

### Task 5: Frontend Show File Controls And Duration UI

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Update TypeScript types**

Modify `ui/src/types.ts`:

```ts
export type FadeTarget = {
  group: number;
  channel: number;
  channelName: string;
  targetDb: number;
  enabled: boolean;
  updatedAt: string;
};

export type SceneFadeConfig = {
  sceneId: string;
  sceneIndex: number;
  sceneName: string;
  fadeEnabled: boolean;
  durationMs: number;
  fadeTargets: FadeTarget[];
};
```

Add to `AppViewState`:

```ts
  showFileName: string;
  showFilePath: string | null;
  showFileDirty: boolean;
  showFileLastSavedAt: string | null;
```

Add defaults:

```ts
  showFileName: "Untitled Show",
  showFilePath: null,
  showFileDirty: false,
  showFileLastSavedAt: null,
```

- [ ] **Step 2: Add frontend command wiring**

In `App.tsx`, add command functions to the header area:

```tsx
            <ShowFileControls
              appState={appState}
              newShow={() => runSnapshotCommand("new_show_file")}
              openShow={() => runSnapshotCommand("open_show_file_dialog")}
              saveShow={() => runSnapshotCommand("save_show_file")}
              saveShowAs={() => runSnapshotCommand("save_show_file_as_dialog")}
            />
```

Add component near `StatusBadge`:

```tsx
function ShowFileControls(props: {
  appState: AppViewState;
  newShow: () => void;
  openShow: () => void;
  saveShow: () => void;
  saveShowAs: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl border border-slate-800 bg-slate-950 px-3 py-2">
      <div className="min-w-0 pr-2">
        <p className="max-w-52 truncate text-sm font-semibold text-slate-100">
          {props.appState.showFileName}{props.appState.showFileDirty ? " *" : ""}
        </p>
        <p className="max-w-52 truncate text-xs text-slate-500">
          {props.appState.showFilePath ?? "No show file saved"}
        </p>
      </div>
      <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={props.newShow}>New</button>
      <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={props.openShow}>Open</button>
      <button className="rounded bg-cyan-700 px-3 py-1 text-sm font-semibold text-white hover:bg-cyan-600" onClick={props.saveShow}>Save</button>
      <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={props.saveShowAs}>Save As</button>
    </div>
  );
}
```

- [ ] **Step 3: Wire duration command into Scene tab**

In `App.tsx`, pass a new prop:

```tsx
            setSceneDurationMs={(sceneId, durationMs) =>
              runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs })
            }
```

Update `SceneTab` props:

```tsx
  setSceneDurationMs: (sceneId: string, durationMs: number) => void;
```

In the selected scene detail controls, add:

```tsx
                <label className="grid gap-1 text-sm text-slate-300">
                  Duration
                  <input
                    className="w-28 rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
                    min={0.1}
                    max={120}
                    step={0.1}
                    type="number"
                    value={(selected.durationMs / 1000).toFixed(1)}
                    onChange={(event) => {
                      const seconds = Number(event.target.value);
                      if (Number.isFinite(seconds)) {
                        props.setSceneDurationMs(selected.sceneId, Math.round(seconds * 1000));
                      }
                    }}
                  />
                </label>
```

In `FadeTargetTable`, show captured name and current name:

```tsx
                <td className="px-3 py-2">
                  <span className="block">{target.channelName}</span>
                  <span className="text-xs text-slate-500">Current: {channelName(props.channels, target.group, target.channel)}</span>
                </td>
```

- [ ] **Step 4: Add Rust command for duration**

In `commands.rs`:

```rust
#[tauri::command]
pub async fn set_scene_duration_ms(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    duration_ms: u64,
) -> Result<AppViewState, String> {
    let snapshot = state.set_scene_duration_ms(scene_id, duration_ms).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}
```

In `main.rs`, register:

```rust
            commands::set_scene_duration_ms,
```

- [ ] **Step 5: Run frontend and Rust checks**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/types.ts ui/src/App.tsx src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: add show file controls"
```

---

### Task 6: Final Verification And Phase Checklist

**Files:**
- Modify: `PHASES.md`

- [ ] **Step 1: Run full verification**

Run: `cargo test`

Expected: PASS.

Run: `npm --prefix ui run typecheck`

Expected: PASS.

Run: `npm --prefix ui run build`

Expected: PASS.

- [ ] **Step 2: Manually smoke test native dialogs if feasible**

Run: `npm run tauri dev`

Expected: app opens.

Manual checks:

- Connect or otherwise use a state with LV1 scenes/channels available.
- Capture a target in Listen Mode.
- Save As opens a native save dialog defaulting to the platform Documents app folder.
- Saved file has `.lv1show` JSON with `durationMs` and `channelName`.
- Save over the same path creates a backup under platform app data `backups/`.
- Open uses a native open dialog and reloads the show file.

- [ ] **Step 3: Update phase status**

In `PHASES.md`, update Phase 5 checklist line:

```md
- [x] **Phase 5: Storage And Show Files** — JSON `.lv1show` save/load, native Open/Save dialogs, platform-aware default show folder, internal backup-on-save, exact-match load validation, deletion of missing/renamed scene or channel configs on load, duration storage, captured channel names, and dirty state are implemented and tested. Remapping, scene rename handling, channel rename handling, autosave, and durable rename/reorder matching remain deferred.
```

- [ ] **Step 4: Commit final docs update**

```bash
git add PHASES.md
git commit -m "docs: mark phase 5 complete"
```

---

## Self-Review Notes

Spec coverage:

- Portable `.lv1show` JSON: Tasks 1, 4.
- Strict exact-match scene/channel validation and deletion logs: Tasks 1, 3.
- No load report UI: Task 5 uses Logs only.
- Platform-aware default show folder and internal backup folder: Task 4.
- Native dialogs: Task 4 uses `rfd::FileDialog` inside Tauri commands.
- Backup-on-save and no autosave: Task 4.
- Show-file UI controls and dirty state: Tasks 2, 5.
- Duration and captured channel name persistence: Tasks 2, 3, 5.
- No curve storage: Task 1 file shape omits curve.
- Phase checklist update: Task 6.

Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps remain.

Type consistency: Rust uses snake_case fields with `serde(rename_all = "camelCase")`; TypeScript uses camelCase fields matching serialized `AppViewState`.
