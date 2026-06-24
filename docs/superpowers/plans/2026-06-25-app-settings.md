# App Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add app-level settings infrastructure, JSON persistence, projection, Tauri replacement command, and frontend Settings controls without wiring settings to runtime behavior.

**Architecture:** A new app-lifetime `settings` actor owns `AppSettings`, loads `<app config dir>/settings.json` on startup, normalizes safe values, saves the full JSON file immediately after changed replacements, and publishes `SettingsEvent::StateChanged`. The projector includes settings in `AppViewState`; the frontend edits settings by sending full-object replacements only.

**Tech Stack:** Rust/Tauri actor modules with Tokio mailboxes and `AppEventBus`; Serde JSON persistence; React/TypeScript UI with Vitest tests.

## Global Constraints

- Settings are application preferences, not show/session data.
- Settings are stored in the Tauri app config directory as `settings.json`.
- Settings are loaded on app startup and saved immediately after each changed replacement.
- This slice establishes the final Rust data model and editing surface; settings do not need to affect runtime behavior yet.
- Frontend always replaces the full `AppSettings` object; there are no per-setting public commands.
- Settings changes do not mark the current `.ascs` session dirty.
- Default `fader_override_sensitivity` is `9`.
- Rust tests must be either pure side-effect-free unit tests or actor tests that interact only through the actor mailbox and `AppEventBus`.
- Update `docs/architecture.md` and `docs/roadmap.md` as part of implementation.

---

## File Structure

- Create `src-tauri/src/settings/types.rs`: durable settings model, defaults, normalization helpers.
- Create `src-tauri/src/settings/commands.rs`: mailbox command enum and command result.
- Create `src-tauri/src/settings/events.rs`: settings event type.
- Create `src-tauri/src/settings/handle.rs`: dumb cloneable mailbox sender.
- Create `src-tauri/src/settings/state.rs`: side-effect-free state transitions and file load/save helpers.
- Create `src-tauri/src/settings/actor.rs`: actor construction, startup load, command loop, persistence, event publication.
- Create `src-tauri/src/settings/mod.rs`: public module exports.
- Modify `src-tauri/src/lib.rs`: expose `settings` module.
- Modify `src-tauri/src/runtime/events.rs`: add `AppEvent::Settings(SettingsEvent)`.
- Modify `src-tauri/src/projector/view.rs`: add `settings: AppSettings` to `AppViewState`.
- Modify `src-tauri/src/projector/cache.rs`: cache settings and include them in snapshots.
- Modify `src-tauri/src/projector/runtime.rs`: seed initial settings and apply settings events.
- Modify `src-tauri/src/lifecycle/mod.rs`: carry `SettingsHandle` and pass initial settings to projector startup.
- Modify `src-tauri/src/ui/mod.rs`: build/spawn settings actor, manage settings handle, register command.
- Create `src-tauri/src/ui/commands/settings.rs`: Tauri command adapter for full settings replacement.
- Modify `src-tauri/src/ui/commands.rs`: export settings command module.
- Modify `ui/src/types.ts`: mirror `AppSettings` and add to `AppViewState`.
- Modify `ui/src/commands.ts`: add `replaceAppSettings(settings)`.
- Create `ui/src/components/SettingsTab.tsx`: controls for all settings.
- Create `ui/src/components/SettingsTab.test.tsx`: frontend behavior tests.
- Create `ui/src/components/SettingsTab.stories.tsx`: Storybook state coverage.
- Modify `ui/src/components/AppShell.tsx`: render `SettingsTab` instead of placeholder.
- Modify `ui/src/storybook/mockAppState.ts`: include default settings.
- Modify `docs/architecture.md`: document settings actor ownership and persistence.
- Modify `docs/roadmap.md`: mark settings infrastructure/UI slice complete and behavior wiring deferred.

---

### Task 1: Rust Settings Model And Pure Normalization

**Files:**
- Create: `src-tauri/src/settings/types.rs`
- Create: `src-tauri/src/settings/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `AppSettings`, `KeyboardShortcutSettings`, `KeyboardShortcut`, `KeyboardShortcutModifiers`, `TimeDisplayFormat`.
- Produces: `AppSettings::normalized(self) -> AppSettings`.
- Produces: `AppSettings::default()` with sensitivity `9`.

- [ ] **Step 1: Write pure unit tests for defaults and normalization**

Add this test module at the bottom of `src-tauri/src/settings/types.rs` while creating the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_use_agreed_values() {
        let settings = AppSettings::default();

        assert!(!settings.auto_load_last_show_file);
        assert!(!settings.auto_save_sessions);
        assert_eq!(settings.keyboard_shortcuts.go.key, "Space");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
        assert!(!settings.auto_cue_next_scene_on_go);
        assert_eq!(settings.time_display, TimeDisplayFormat::TwentyFourHour);
        assert_eq!(settings.fader_override_sensitivity, 9);
    }

    #[test]
    fn normalization_clamps_sensitivity_and_trims_shortcuts() {
        let settings = AppSettings {
            fader_override_sensitivity: 99,
            keyboard_shortcuts: KeyboardShortcutSettings {
                go: KeyboardShortcut {
                    key: "  Enter  ".to_string(),
                    modifiers: KeyboardShortcutModifiers {
                        shift: true,
                        ..Default::default()
                    },
                },
                cue: KeyboardShortcut {
                    key: "   ".to_string(),
                    modifiers: KeyboardShortcutModifiers::default(),
                },
            },
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.fader_override_sensitivity, 10);
        assert_eq!(settings.keyboard_shortcuts.go.key, "Enter");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
    }

    #[test]
    fn normalization_clamps_sensitivity_to_minimum() {
        let settings = AppSettings {
            fader_override_sensitivity: 0,
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.fader_override_sensitivity, 1);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control settings::types`

Expected: FAIL because `settings` module and types are not defined yet.

- [ ] **Step 3: Implement the settings model**

Create `src-tauri/src/settings/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub auto_load_last_show_file: bool,
    pub auto_save_sessions: bool,
    pub keyboard_shortcuts: KeyboardShortcutSettings,
    pub auto_cue_next_scene_on_go: bool,
    pub time_display: TimeDisplayFormat,
    pub fader_override_sensitivity: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_load_last_show_file: false,
            auto_save_sessions: false,
            keyboard_shortcuts: KeyboardShortcutSettings::default(),
            auto_cue_next_scene_on_go: false,
            time_display: TimeDisplayFormat::TwentyFourHour,
            fader_override_sensitivity: 9,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.fader_override_sensitivity = self.fader_override_sensitivity.clamp(1, 10);
        self.keyboard_shortcuts = self.keyboard_shortcuts.normalized();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcutSettings {
    pub go: KeyboardShortcut,
    pub cue: KeyboardShortcut,
}

impl Default for KeyboardShortcutSettings {
    fn default() -> Self {
        Self {
            go: KeyboardShortcut::go_default(),
            cue: KeyboardShortcut::cue_default(),
        }
    }
}

impl KeyboardShortcutSettings {
    fn normalized(self) -> Self {
        Self {
            go: self.go.normalized_or(KeyboardShortcut::go_default()),
            cue: self.cue.normalized_or(KeyboardShortcut::cue_default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcut {
    pub key: String,
    pub modifiers: KeyboardShortcutModifiers,
}

impl Default for KeyboardShortcut {
    fn default() -> Self {
        Self::go_default()
    }
}

impl KeyboardShortcut {
    fn go_default() -> Self {
        Self {
            key: "Space".to_string(),
            modifiers: KeyboardShortcutModifiers::default(),
        }
    }

    fn cue_default() -> Self {
        Self {
            key: "C".to_string(),
            modifiers: KeyboardShortcutModifiers::default(),
        }
    }

    fn normalized_or(mut self, fallback: Self) -> Self {
        self.key = self.key.trim().to_string();
        if self.key.is_empty() { fallback } else { self }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcutModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TimeDisplayFormat {
    TwelveHour,
    TwentyFourHour,
}

impl Default for TimeDisplayFormat {
    fn default() -> Self {
        Self::TwentyFourHour
    }
}
```

Create `src-tauri/src/settings/mod.rs`:

```rust
mod types;

pub use types::{
    AppSettings, KeyboardShortcut, KeyboardShortcutModifiers, KeyboardShortcutSettings,
    TimeDisplayFormat,
};
```

Modify `src-tauri/src/lib.rs` to include:

```rust
pub mod settings;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p advanced-show-control settings::types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/settings/mod.rs src-tauri/src/settings/types.rs
git commit -m "feat: add app settings model"
```

---

### Task 2: Settings Actor, App-Config Persistence, And Actor Tests

**Files:**
- Create: `src-tauri/src/settings/commands.rs`
- Create: `src-tauri/src/settings/events.rs`
- Create: `src-tauri/src/settings/handle.rs`
- Create: `src-tauri/src/settings/state.rs`
- Create: `src-tauri/src/settings/actor.rs`
- Modify: `src-tauri/src/settings/mod.rs`
- Modify: `src-tauri/src/runtime/events.rs`

**Interfaces:**
- Consumes: `AppSettings::normalized(self) -> AppSettings` from Task 1.
- Produces: `build_settings_actor(settings_dir: PathBuf, event_bus: AppEventBus) -> (SettingsHandle, SettingsActorTask)`.
- Produces: `SettingsCommand::{GetSettings, ReplaceSettings}`.
- Produces: `SettingsEvent::StateChanged { settings: AppSettings }`.
- Produces: `AppEvent::Settings(SettingsEvent)`.

- [ ] **Step 1: Write actor tests through mailbox and event bus only**

Add these tests in `src-tauri/src/settings/actor.rs` while creating the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEvent;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::oneshot;

    fn temp_settings_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("asc-settings-{name}-{unique}"))
    }

    async fn get_settings(handle: &SettingsHandle) -> AppSettings {
        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetSettings { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    #[tokio::test]
    async fn actor_loads_defaults_when_file_is_missing() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("missing");
        let (handle, task) = build_settings_actor(dir, event_bus);
        task.spawn();

        assert_eq!(get_settings(&handle).await, AppSettings::default());
    }

    #[tokio::test]
    async fn actor_normalizes_replacement_saves_file_and_publishes_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("replace");
        let (handle, task) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings {
                    auto_save_sessions: true,
                    fader_override_sensitivity: 99,
                    ..Default::default()
                },
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(rx.await.unwrap().unwrap(), SettingsCommandResult { changed: true });
        let saved = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(saved.contains("autoSaveSessions"));
        assert!(saved.contains("\"faderOverrideSensitivity\": 10"));

        let received = events.recv().await.unwrap();
        assert!(matches!(
            received,
            AppEvent::Settings(SettingsEvent::StateChanged { settings })
                if settings.auto_save_sessions && settings.fader_override_sensitivity == 10
        ));
    }

    #[tokio::test]
    async fn actor_does_not_save_or_publish_when_normalized_settings_are_unchanged() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("unchanged");
        let (handle, task) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings::default(),
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(rx.await.unwrap().unwrap(), SettingsCommandResult { changed: false });
        assert!(!dir.join("settings.json").exists());
        assert!(tokio::time::timeout(std::time::Duration::from_millis(50), events.recv()).await.is_err());
    }
}
```

- [ ] **Step 2: Run actor tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control settings::actor`

Expected: FAIL because actor, commands, events, and `AppEvent::Settings` are not implemented yet.

- [ ] **Step 3: Implement commands, events, handle, state, actor, and event bus variant**

Create `src-tauri/src/settings/commands.rs`:

```rust
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::AppSettings;

pub enum SettingsCommand {
    GetSettings {
        reply: oneshot::Sender<AppSettings>,
    },
    ReplaceSettings {
        settings: AppSettings,
        reply: Option<oneshot::Sender<Result<SettingsCommandResult, String>>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsCommandResult {
    pub changed: bool,
}
```

Create `src-tauri/src/settings/events.rs`:

```rust
use super::AppSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEvent {
    StateChanged { settings: AppSettings },
}
```

Create `src-tauri/src/settings/handle.rs`:

```rust
use tokio::sync::mpsc;

use super::SettingsCommand;

#[derive(Clone)]
pub struct SettingsHandle {
    tx: mpsc::Sender<SettingsCommand>,
}

impl SettingsHandle {
    pub(crate) fn new(tx: mpsc::Sender<SettingsCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: SettingsCommand,
    ) -> Result<(), mpsc::error::SendError<SettingsCommand>> {
        self.tx.send(command).await
    }
}
```

Create `src-tauri/src/settings/state.rs`:

```rust
use std::path::{Path, PathBuf};

use super::AppSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    settings: AppSettings,
    file_path: PathBuf,
}

impl SettingsState {
    pub fn load(settings_dir: PathBuf) -> Self {
        let file_path = settings_dir.join("settings.json");
        let settings = load_settings_file(&file_path);
        Self { settings, file_path }
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.clone()
    }

    pub fn replace_settings(&mut self, settings: AppSettings) -> Result<bool, String> {
        let normalized = settings.normalized();
        if normalized == self.settings {
            return Ok(false);
        }
        write_settings_file(&self.file_path, &normalized)?;
        self.settings = normalized;
        Ok(true)
    }
}

fn load_settings_file(file_path: &Path) -> AppSettings {
    std::fs::read_to_string(file_path)
        .ok()
        .and_then(|contents| serde_json::from_str::<AppSettings>(&contents).ok())
        .unwrap_or_default()
        .normalized()
}

fn write_settings_file(file_path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create settings directory: {err}"))?;
    }
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("Failed to serialize settings: {err}"))?;
    std::fs::write(file_path, contents)
        .map_err(|err| format!("Failed to write settings: {err}"))
}
```

Create `src-tauri/src/settings/actor.rs`:

```rust
use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::runtime::events::{AppEvent, AppEventBus};

use super::commands::{SettingsCommand, SettingsCommandResult};
use super::events::SettingsEvent;
use super::handle::SettingsHandle;
use super::state::SettingsState;

pub struct SettingsActorTask {
    rx: mpsc::Receiver<SettingsCommand>,
    event_bus: AppEventBus,
    state: SettingsState,
}

impl SettingsActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_settings_actor(self.rx, self.event_bus, self.state));
    }
}

pub fn build_settings_actor(
    settings_dir: PathBuf,
    event_bus: AppEventBus,
) -> (SettingsHandle, SettingsActorTask) {
    let (tx, rx) = mpsc::channel(32);
    let state = SettingsState::load(settings_dir);
    let task = SettingsActorTask { rx, event_bus, state };
    (SettingsHandle::new(tx), task)
}

async fn run_settings_actor(
    mut rx: mpsc::Receiver<SettingsCommand>,
    event_bus: AppEventBus,
    mut state: SettingsState,
) {
    while let Some(command) = rx.recv().await {
        handle_command(command, &event_bus, &mut state).await;
    }
}

async fn handle_command(
    command: SettingsCommand,
    event_bus: &AppEventBus,
    state: &mut SettingsState,
) {
    match command {
        SettingsCommand::GetSettings { reply } => {
            let _ = reply.send(state.settings());
        }
        SettingsCommand::ReplaceSettings { settings, reply } => {
            let result = state.replace_settings(settings).map(|changed| {
                if changed {
                    event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
                        settings: state.settings(),
                    }));
                }
                SettingsCommandResult { changed }
            });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
    }
}
```

Update `src-tauri/src/settings/mod.rs`:

```rust
mod actor;
mod commands;
mod events;
mod handle;
mod state;
mod types;

pub use actor::{SettingsActorTask, build_settings_actor};
pub use commands::{SettingsCommand, SettingsCommandResult};
pub use events::SettingsEvent;
pub use handle::SettingsHandle;
pub use types::{
    AppSettings, KeyboardShortcut, KeyboardShortcutModifiers, KeyboardShortcutSettings,
    TimeDisplayFormat,
};
```

Modify `src-tauri/src/runtime/events.rs` to import and add the variant:

```rust
use crate::settings::SettingsEvent;

pub enum AppEvent {
    Runtime(RuntimeLifecycleEvent),
    Lv1 { generation: u64, event: Lv1Event },
    Fade { generation: u64, event: FadeEvent },
    Scenes { generation: u64, event: ScenesEvent },
    Show(ShowEvent),
    Settings(SettingsEvent),
}
```

- [ ] **Step 4: Run actor tests to verify they pass**

Run: `cargo nextest run -p advanced-show-control settings::actor`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings src-tauri/src/runtime/events.rs
git commit -m "feat: add settings actor"
```

---

### Task 3: Projection, Startup Wiring, And Tauri Replace Command

**Files:**
- Modify: `src-tauri/src/projector/view.rs`
- Modify: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Create: `src-tauri/src/ui/commands/settings.rs`
- Modify: `src-tauri/src/ui/commands.rs`

**Interfaces:**
- Consumes: `SettingsHandle`, `SettingsCommand`, `AppSettings`, `SettingsEvent` from Task 2.
- Produces: `AppViewState.settings: AppSettings`.
- Produces: Tauri command `replace_app_settings(settings: AppSettings) -> Result<SettingsCommandResult, String>`.

- [ ] **Step 1: Write projection and command adapter tests**

Update existing projector tests in `src-tauri/src/projector/runtime.rs` so `ProjectorInputs` includes `initial_settings: AppSettings`. Add this test:

```rust
#[tokio::test]
async fn settings_event_marks_cache_dirty_and_projects_settings() {
    let app = mock_app();
    let handle = app.handle().clone();
    let event_bus = AppEventBus::default();
    let (_log_tx, log_rx) = broadcast::channel(8);
    let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let received_events = received.clone();
    handle.listen_any("app-status-changed", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        received_events.lock().unwrap().push(payload);
    });

    let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

    event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
        settings: AppSettings {
            auto_save_sessions: true,
            ..Default::default()
        },
    }));
    tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

    projector.abort();
    let snapshots = received.lock().unwrap();
    assert!(snapshots.iter().any(|snapshot| {
        snapshot["settings"]["autoSaveSessions"] == true
    }));
}
```

Add a command export assertion to `src-tauri/src/ui/mod.rs` tests:

```rust
let _ = super::commands::settings::replace_app_settings;
```

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control projector::runtime ui::tests::command_adapter_exports_existing_command_names`

Expected: FAIL because `AppViewState.settings`, projector settings handling, and command adapter do not exist yet.

- [ ] **Step 3: Implement projection and command wiring**

Modify `src-tauri/src/projector/view.rs`:

```rust
use crate::settings::AppSettings;

pub struct AppViewState {
    // existing fields...
    pub settings: AppSettings,
    pub state_version: u64,
}
```

Modify `src-tauri/src/projector/cache.rs`:

```rust
use crate::settings::AppSettings;

settings: AppSettings,

// in new/default
settings: AppSettings::default(),

pub fn apply_settings(&mut self, settings: AppSettings) {
    self.settings = settings;
}

// in seed_from_view_state
self.settings = snapshot.settings.clone();

// in build_snapshot
settings: self.settings.clone(),
```

Modify `src-tauri/src/projector/runtime.rs`:

```rust
use crate::settings::{AppSettings, SettingsEvent};

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub generation: u64,
    pub initial_show_state: ShowProjectionState,
    pub initial_settings: AppSettings,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: broadcast::Receiver<UiLogEvent>,
}

cache.apply_settings(initial_settings);

AppEvent::Settings(SettingsEvent::StateChanged { settings }) => {
    cache.apply_settings(settings.clone());
    true
}
```

Modify `src-tauri/src/lifecycle/mod.rs`:

```rust
use crate::settings::{SettingsCommand, SettingsHandle};

pub struct AppLifecycle {
    // existing fields
    settings: SettingsHandle,
}

pub fn new(
    event_bus: AppEventBus,
    show: ShowStateHandle,
    show_peers: ShowActorPeers,
    settings: SettingsHandle,
) -> Self {
    Self { settings, /* existing fields */ }
}

pub async fn current_settings(&self) -> SettingsHandle {
    self.settings.clone()
}
```

When spawning the projector in lifecycle frontend-ready flow, request settings with `SettingsCommand::GetSettings` and pass `initial_settings` into `ProjectorInputs`.

Create `src-tauri/src/ui/commands/settings.rs`:

```rust
use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::settings::{AppSettings, SettingsCommand, SettingsCommandResult};
use tauri::State;
use tokio::sync::oneshot;

#[tauri::command]
pub async fn replace_app_settings(
    lifecycle: State<'_, AppLifecycle>,
    settings: AppSettings,
) -> Result<SettingsCommandResult, String> {
    let settings_handle = lifecycle.current_settings().await;
    let (reply, rx) = oneshot::channel();
    settings_handle
        .send(SettingsCommand::ReplaceSettings {
            settings,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::CommandFailed("Settings unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
```

Modify `src-tauri/src/ui/commands.rs`:

```rust
pub(crate) mod settings;
pub use settings::replace_app_settings;
```

Modify `src-tauri/src/ui/mod.rs` setup to build settings from app config dir:

```rust
let settings_dir = app.path().app_config_dir()?;
let (settings, settings_task) = crate::settings::build_settings_actor(settings_dir, event_bus.clone());
let lifecycle = AppLifecycle::new(event_bus, show.clone(), show_peers, settings.clone());
settings_task.spawn();
app.manage(settings);
```

Register `commands::settings::replace_app_settings` in `tauri::generate_handler!`.

- [ ] **Step 4: Run targeted Rust tests**

Run: `cargo nextest run -p advanced-show-control projector::runtime ui::tests::command_adapter_exports_existing_command_names`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/projector src-tauri/src/lifecycle/mod.rs src-tauri/src/ui/mod.rs src-tauri/src/ui/commands.rs src-tauri/src/ui/commands/settings.rs
git commit -m "feat: project app settings"
```

---

### Task 4: Frontend Settings Types, Command, Controls, And Tests

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/commands.ts`
- Create: `ui/src/components/SettingsTab.tsx`
- Create: `ui/src/components/SettingsTab.test.tsx`
- Create: `ui/src/components/SettingsTab.stories.tsx`
- Modify: `ui/src/components/AppShell.tsx`
- Modify: `ui/src/storybook/mockAppState.ts`

**Interfaces:**
- Consumes: projected `AppViewState.settings` from Task 3.
- Produces: `replaceAppSettings(settings: AppSettings)` frontend command wrapper.
- Produces: `SettingsTab` UI that always sends full-object replacements.

- [ ] **Step 1: Write frontend tests for full-object replacement**

Create `ui/src/components/SettingsTab.test.tsx`:

```tsx
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { SettingsTab } from "./SettingsTab";

const replaceAppSettings = vi.fn();

vi.mock("../commands", async (actual) => ({
  ...(await actual<typeof import("../commands")>()),
  replaceAppSettings: (settings: unknown) => replaceAppSettings(settings),
}));

describe("SettingsTab", () => {
  it("renders projected settings and replaces the full object on toggle", () => {
    const state = {
      ...disconnectedAppViewState,
      settings: {
        autoLoadLastShowFile: false,
        autoSaveSessions: false,
        keyboardShortcuts: {
          go: { key: "Space", modifiers: { shift: false, control: false, alt: false, meta: false } },
          cue: { key: "C", modifiers: { shift: false, control: false, alt: false, meta: false } },
        },
        autoCueNextSceneOnGo: false,
        timeDisplay: "twentyFourHour" as const,
        faderOverrideSensitivity: 9,
      },
    };

    renderWithAppProviders(<SettingsTab />, { appState: state });
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...state.settings,
      autoSaveSessions: true,
    });
  });

  it("sends sensitivity updates as a bounded number", () => {
    renderWithAppProviders(<SettingsTab />, { appState: disconnectedAppViewState });

    fireEvent.change(screen.getByLabelText("Fader override sensitivity"), {
      target: { value: "10" },
    });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      faderOverrideSensitivity: 10,
    });
  });
});
```

- [ ] **Step 2: Run frontend test to verify it fails**

Run: `npm run test -- SettingsTab.test.tsx`

Expected: FAIL because types, command wrapper, and `SettingsTab` do not exist yet.

- [ ] **Step 3: Add TypeScript settings types and default disconnected state**

Modify `ui/src/types.ts`:

```ts
export type TimeDisplayFormat = "twelveHour" | "twentyFourHour";

export type KeyboardShortcutModifiers = {
  shift: boolean;
  control: boolean;
  alt: boolean;
  meta: boolean;
};

export type KeyboardShortcut = {
  key: string;
  modifiers: KeyboardShortcutModifiers;
};

export type KeyboardShortcutSettings = {
  go: KeyboardShortcut;
  cue: KeyboardShortcut;
};

export type AppSettings = {
  autoLoadLastShowFile: boolean;
  autoSaveSessions: boolean;
  keyboardShortcuts: KeyboardShortcutSettings;
  autoCueNextSceneOnGo: boolean;
  timeDisplay: TimeDisplayFormat;
  faderOverrideSensitivity: number;
};
```

Add `settings: AppSettings` to `AppViewState` and add this value to `disconnectedAppViewState`:

```ts
settings: {
  autoLoadLastShowFile: false,
  autoSaveSessions: false,
  keyboardShortcuts: {
    go: { key: "Space", modifiers: { shift: false, control: false, alt: false, meta: false } },
    cue: { key: "C", modifiers: { shift: false, control: false, alt: false, meta: false } },
  },
  autoCueNextSceneOnGo: false,
  timeDisplay: "twentyFourHour",
  faderOverrideSensitivity: 9,
},
```

Modify `ui/src/commands.ts`:

```ts
import type { AppSettings, Lv1SystemIdentity } from "./types";

export async function replaceAppSettings(settings: AppSettings) {
  return invoke<void>("replace_app_settings", { settings });
}
```

- [ ] **Step 4: Implement `SettingsTab` controls**

Create `ui/src/components/SettingsTab.tsx`:

```tsx
import { replaceAppSettings } from "../commands";
import { useAppState } from "../appHooks";
import type { AppSettings, KeyboardShortcut } from "../types";
import { Panel } from "./Panel";

export function SettingsTab() {
  const { appState } = useAppState();
  const settings = appState.settings;

  function replace(next: AppSettings) {
    void replaceAppSettings(next);
  }

  function updateShortcut(action: "go" | "cue", shortcut: KeyboardShortcut) {
    replace({
      ...settings,
      keyboardShortcuts: {
        ...settings.keyboardShortcuts,
        [action]: shortcut,
      },
    });
  }

  return (
    <div className="grid h-full min-h-0 gap-3 overflow-auto">
      <Panel className="p-4">
        <h1 className="text-lg font-semibold text-console-primary">Settings</h1>
        <p className="mt-1 text-sm text-console-muted">
          Settings are saved immediately to this computer. These controls do not change show behavior yet.
        </p>
      </Panel>

      <Panel className="grid gap-4 p-4">
        <SettingCheckbox
          label="Auto load last show file"
          checked={settings.autoLoadLastShowFile}
          onChange={(checked) => replace({ ...settings, autoLoadLastShowFile: checked })}
        />
        <SettingCheckbox
          label="Auto save sessions"
          checked={settings.autoSaveSessions}
          onChange={(checked) => replace({ ...settings, autoSaveSessions: checked })}
        />
        <SettingCheckbox
          label="Auto cue next scene on GO"
          checked={settings.autoCueNextSceneOnGo}
          onChange={(checked) => replace({ ...settings, autoCueNextSceneOnGo: checked })}
        />
      </Panel>

      <Panel className="grid gap-4 p-4">
        <label className="grid gap-2 text-sm text-console-muted">
          Time display
          <select
            className="rounded-console-button border border-console-line bg-console-surface px-3 py-2 text-console-primary"
            value={settings.timeDisplay}
            onChange={(event) => replace({ ...settings, timeDisplay: event.target.value as AppSettings["timeDisplay"] })}
          >
            <option value="twelveHour">12 hour</option>
            <option value="twentyFourHour">24 hour</option>
          </select>
        </label>
        <label className="grid gap-2 text-sm text-console-muted">
          Fader override sensitivity
          <input
            aria-label="Fader override sensitivity"
            type="range"
            min="1"
            max="10"
            value={settings.faderOverrideSensitivity}
            onChange={(event) => replace({ ...settings, faderOverrideSensitivity: Number(event.target.value) })}
          />
          <span className="text-console-primary">{settings.faderOverrideSensitivity}</span>
        </label>
      </Panel>

      <Panel className="grid gap-4 p-4">
        <ShortcutInput
          label="GO keyboard shortcut"
          shortcut={settings.keyboardShortcuts.go}
          onChange={(shortcut) => updateShortcut("go", shortcut)}
        />
        <ShortcutInput
          label="Cue keyboard shortcut"
          shortcut={settings.keyboardShortcuts.cue}
          onChange={(shortcut) => updateShortcut("cue", shortcut)}
        />
      </Panel>
    </div>
  );
}

function SettingCheckbox(props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 text-sm text-console-primary">
      {props.label}
      <input
        aria-label={props.label}
        type="checkbox"
        checked={props.checked}
        onChange={(event) => props.onChange(event.target.checked)}
      />
    </label>
  );
}

function ShortcutInput(props: {
  label: string;
  shortcut: KeyboardShortcut;
  onChange: (shortcut: KeyboardShortcut) => void;
}) {
  return (
    <label className="grid gap-2 text-sm text-console-muted">
      {props.label}
      <input
        className="rounded-console-button border border-console-line bg-console-surface px-3 py-2 text-console-primary"
        value={props.shortcut.key}
        onChange={(event) => props.onChange({ ...props.shortcut, key: event.target.value })}
      />
    </label>
  );
}
```

Modify `ui/src/components/AppShell.tsx`:

```tsx
import { SettingsTab } from "./SettingsTab";

{props.activeTab === "settings" && <SettingsTab />}
```

Create `ui/src/components/SettingsTab.stories.tsx`:

```tsx
import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { mockAppState } from "../storybook/mockAppState";
import { SettingsTab } from "./SettingsTab";

const meta = {
  title: "Components/SettingsTab",
  component: SettingsTab,
} satisfies Meta<typeof SettingsTab>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <MockAppProviders appState={mockAppState}>
      <SettingsTab />
    </MockAppProviders>
  ),
};
```

Update `ui/src/storybook/mockAppState.ts` to include `settings`, matching `disconnectedAppViewState.settings` unless the file already derives from that object.

- [ ] **Step 5: Run frontend tests**

Run: `npm run test -- SettingsTab.test.tsx`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/types.ts ui/src/commands.ts ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx ui/src/components/SettingsTab.stories.tsx ui/src/components/AppShell.tsx ui/src/storybook/mockAppState.ts
git commit -m "feat: add settings tab"
```

---

### Task 5: Documentation And Final Verification

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: completed settings actor, projection, command, and frontend controls from Tasks 1-4.
- Produces: documentation that describes implemented infrastructure and explicitly leaves behavior wiring deferred.

- [ ] **Step 1: Update architecture docs**

In `docs/architecture.md`, update the component table to include:

```markdown
| `settings` | Maintains app-level preferences, loads/saves app-config `settings.json`, validates normalized settings replacements, and publishes settings projection facts. |
```

Add a settings actor row to the actor model or direct owner discussion:

```markdown
`SettingsActor` is an app-lifetime actor. It is not tied to LV1 connection generation. It owns `AppSettings`, loads `settings.json` from the Tauri app config directory during startup, accepts full-object replacement through `SettingsCommand::ReplaceSettings`, saves changed settings immediately, and publishes `SettingsEvent::StateChanged` through `AppEventBus` for projector consumption.
```

- [ ] **Step 2: Update roadmap docs**

In `docs/roadmap.md`, update completed foundation with:

```markdown
- The Settings tab has app-level controls backed by a startup-loaded app config `settings.json`; settings are saved immediately on change and projected through app state.
```

Update MVP roadmap item 3 to clarify completed infrastructure and deferred behavior wiring:

```markdown
3. Wire Settings behavior.
   - Use the existing app-level settings infrastructure for auto-session recall behavior.
   - Apply auto-save, keyboard shortcuts, auto-cue, time display, and fader override sensitivity behavior in focused follow-up slices.
```

- [ ] **Step 3: Run focused Rust and UI verification**

Run: `cargo nextest run -p advanced-show-control settings projector::runtime ui::tests`

Expected: PASS.

Run: `npm run test -- SettingsTab.test.tsx`

Expected: PASS.

- [ ] **Step 4: Run formatting and type/lint checks for touched areas**

Run: `make rust-fmt`

Expected: PASS.

Run: `make rust-lint`

Expected: PASS.

Run: `make ui-fmt`

Expected: PASS.

Run: `make ui-lint`

Expected: PASS.

Run: `make ui-typecheck`

Expected: PASS.

- [ ] **Step 5: Run final CI-style verification**

Run: `make check`

Expected: PASS.

- [ ] **Step 6: Commit docs and verification fixes**

```bash
git add docs/architecture.md docs/roadmap.md
git commit -m "docs: document app settings"
```

If verification required code fixes, stage and commit those intended files with the docs in the same commit only if they are directly related to settings verification. Otherwise create a separate `fix:` commit.

---

## Self-Review

- Spec coverage: the plan covers app-level JSON persistence, startup load, immediate save on replacement, full-object frontend replace, projection, frontend controls, docs, and final verification.
- Rust testing rule coverage: pure model tests are side-effect-free; persistence behavior is tested through actor mailbox and `AppEventBus` only.
- Placeholder scan: no unresolved placeholders remain; deferred runtime behavior is intentionally out of scope and named as deferred docs content.
- Type consistency: `AppSettings`, `SettingsCommand`, `SettingsEvent`, `SettingsHandle`, and `replace_app_settings` names match across Rust projection, Tauri, and frontend tasks.
