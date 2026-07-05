# Extensive Diagnostics Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persisted Settings toggle that keeps diagnostic files at `INFO+` by default while allowing `DEBUG+` file logging during bootstrap and when extensive diagnostics is enabled.

**Architecture:** Logging initializes before settings load with the diagnostic file gate at `DEBUG`. After settings load, setup applies the persisted setting to a shared dynamic file-log gate, and a logging watcher updates the gate from later `SettingsEvent::StateChanged` events. Settings remains the owner of persisted preferences; logging remains the owner of tracing sinks and filtering.

**Tech Stack:** Rust/Tauri, `tracing`, `tracing-subscriber`, Tokio broadcast, React/TypeScript, Vitest/Testing Library.

## Global Constraints

- Diagnostic file logging starts at `DEBUG` during bootstrap.
- After settings load, diagnostic file logging writes `INFO+` when `enable_extensive_diagnostics` is false and `DEBUG+` when it is true.
- Frontend Logs tab behavior remains `INFO/WARN/ERROR` only.
- Stdout logging remains unchanged.
- Existing `settings.json` files remain valid through serde defaults.
- Do not change LV1 protocol logging call sites except through the file-log level gate.
- Rust test style: use pure unit tests for settings defaults, settings deserialization, and logging filter behavior; use actor-style tests only if watcher behavior cannot be isolated.

---

## File Structure

- `src-tauri/src/settings/types.rs`: add `enable_extensive_diagnostics` to `AppSettings`, defaults, and settings unit tests.
- `src-tauri/src/settings/actor.rs`: include the new setting in the existing settings-updated structured log only.
- `src-tauri/src/settings/state.rs`: expose the initially loaded settings through the existing `SettingsState::settings()` method; no new persistence path.
- `src-tauri/src/settings/mod.rs`: no new exports expected beyond the existing `AppSettings` export.
- `src-tauri/src/logging.rs`: add the dynamic file-log gate, update `init_logging`, add a settings watcher, and test the gate behavior.
- `src-tauri/src/ui/mod.rs`: initialize logging before settings load, apply loaded settings, and pass an event-bus subscription to the logging watcher.
- `src-tauri/src/ui/debug.rs`: mirror normal app setup order for the debug smoke app.
- `ui/src/types.ts`: add `enableExtensiveDiagnostics` to `AppSettings` and disconnected fixture defaults.
- `ui/src/components/SettingsTab.tsx`: add the diagnostics toggle using the existing `SettingRow` and `ToggleControl` pattern.
- `ui/src/components/SettingsTab.test.tsx`: create focused tests if none currently exist for settings replacement behavior.
- `ui/src/storybook/mockAppState.ts` and settings stories: update fixtures if typechecking requires the new field.

---

### Task 1: Persist The Extensive Diagnostics Setting

**Files:**
- Modify: `src-tauri/src/settings/types.rs`
- Modify: `src-tauri/src/settings/actor.rs`

**Interfaces:**
- Consumes: existing `AppSettings::default()`, `AppSettings::normalized()`, serde `#[serde(default)]` behavior.
- Produces: `AppSettings { enable_extensive_diagnostics: bool, ... }` for logging and projector consumers.

- [ ] **Step 1: Write failing Rust settings tests**

Edit `src-tauri/src/settings/types.rs` tests so they assert the new default and partial-file behavior. Add these assertions inside existing tests where possible:

```rust
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
    assert!(!settings.enable_extensive_diagnostics);
}

#[test]
fn partial_shortcut_settings_deserialize_with_agreed_defaults() {
    let settings: AppSettings =
        serde_json::from_str(r#"{"keyboardShortcuts":{"cue":{"key":"C"}}}"#)
            .expect("settings should deserialize");

    assert_eq!(settings.keyboard_shortcuts.go.key, "Space");
    assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
    assert!(!settings.enable_extensive_diagnostics);
}
```

- [ ] **Step 2: Run targeted Rust settings tests and verify failure**

Run: `cargo nextest run -p advanced-show-control settings::types`

Expected: compile failure mentioning `no field enable_extensive_diagnostics on type AppSettings`.

- [ ] **Step 3: Implement the setting field and default**

Edit `src-tauri/src/settings/types.rs` `AppSettings` and default implementation:

```rust
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
    pub enable_extensive_diagnostics: bool,
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
            enable_extensive_diagnostics: false,
        }
    }
}
```

- [ ] **Step 4: Include the field in settings-updated logging**

Edit `src-tauri/src/settings/actor.rs` `log_settings_updated`:

```rust
fn log_settings_updated(settings: &AppSettings) {
    tracing::info!(
        event = "settings_updated",
        auto_load_last_show_file = settings.auto_load_last_show_file,
        auto_save_sessions = settings.auto_save_sessions,
        auto_cue_next_scene_on_go = settings.auto_cue_next_scene_on_go,
        time_display = time_display_label(&settings.time_display),
        fader_override_sensitivity = settings.fader_override_sensitivity,
        enable_extensive_diagnostics = settings.enable_extensive_diagnostics,
        go_shortcut = %shortcut_label(&settings.keyboard_shortcuts.go),
        cue_shortcut = %shortcut_label(&settings.keyboard_shortcuts.cue),
        "Settings updated"
    );
}
```

- [ ] **Step 5: Run targeted Rust settings tests and verify pass**

Run: `cargo nextest run -p advanced-show-control settings::types settings::actor`

Expected: PASS for settings tests.

- [ ] **Step 6: Commit Task 1**

Run:

```bash
git status --short
git diff -- src-tauri/src/settings/types.rs src-tauri/src/settings/actor.rs
git add src-tauri/src/settings/types.rs src-tauri/src/settings/actor.rs
git commit -m "feat: add diagnostics logging setting"
```

---

### Task 2: Add Dynamic Diagnostic File Log Gating

**Files:**
- Modify: `src-tauri/src/logging.rs`

**Interfaces:**
- Consumes: existing `logging::init_logging(app) -> Result<LoggingRuntime, Box<dyn Error>>`.
- Produces: `LoggingRuntime::set_extensive_diagnostics_enabled(&self, enabled: bool)` while preserving the existing `init_logging(app)` call shape.

- [ ] **Step 1: Write failing pure unit tests for file gate behavior**

Add tests to `src-tauri/src/logging.rs` test module:

```rust
#[test]
fn diagnostic_file_gate_starts_with_debug_enabled() {
    let gate = DiagnosticFileGate::bootstrap_debug();

    assert_eq!(gate.min_level(), LevelFilter::DEBUG);
}

#[test]
fn diagnostic_file_gate_drops_debug_after_settings_disable_it() {
    let gate = DiagnosticFileGate::bootstrap_debug();

    gate.set_extensive_diagnostics_enabled(false);

    assert_eq!(gate.min_level(), LevelFilter::INFO);
}

#[test]
fn diagnostic_file_gate_allows_debug_after_settings_enable_it() {
    let gate = DiagnosticFileGate::bootstrap_debug();

    gate.set_extensive_diagnostics_enabled(true);

    assert_eq!(gate.min_level(), LevelFilter::DEBUG);
}
```

- [ ] **Step 2: Run logging tests and verify failure**

Run: `cargo nextest run -p advanced-show-control logging`

Expected: compile failure mentioning `DiagnosticFileGate` is not defined.

- [ ] **Step 3: Implement `DiagnosticFileGate`**

Add near the top of `src-tauri/src/logging.rs`:

```rust
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
```

Add this type before `LoggingRuntime`:

```rust
#[derive(Clone)]
pub struct DiagnosticFileGate {
    extensive_diagnostics_enabled: Arc<AtomicBool>,
}

impl DiagnosticFileGate {
    fn bootstrap_debug() -> Self {
        Self {
            extensive_diagnostics_enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_extensive_diagnostics_enabled(&self, enabled: bool) {
        self.extensive_diagnostics_enabled
            .store(enabled, Ordering::Relaxed);
    }

    fn min_level(&self) -> LevelFilter {
        if self.extensive_diagnostics_enabled.load(Ordering::Relaxed) {
            LevelFilter::DEBUG
        } else {
            LevelFilter::INFO
        }
    }
}

impl<S> tracing_subscriber::layer::Filter<S> for DiagnosticFileGate {
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        _ctx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.min_level().enabled(meta)
    }
}
```

- [ ] **Step 4: Add gate to `LoggingRuntime` and file layer**

Update `LoggingRuntime` and `init_logging`:

```rust
pub struct LoggingRuntime {
    pub guard: WorkerGuard,
    pub ui_logs: broadcast::Sender<UiLogEvent>,
    diagnostic_file_gate: DiagnosticFileGate,
}

impl LoggingRuntime {
    pub fn set_extensive_diagnostics_enabled(&self, enabled: bool) {
        self.diagnostic_file_gate
            .set_extensive_diagnostics_enabled(enabled);
    }
}
```

Inside `init_logging`, before `file_layer`:

```rust
let diagnostic_file_gate = DiagnosticFileGate::bootstrap_debug();
```

Change file layer filter from `LevelFilter::DEBUG` to the gate clone:

```rust
let file_layer = fmt::layer()
    .json()
    .with_writer(non_blocking)
    .with_filter(diagnostic_file_gate.clone());
```

Return the gate:

```rust
Ok(LoggingRuntime {
    guard,
    ui_logs: ui_tx,
    diagnostic_file_gate,
})
```

- [ ] **Step 5: Run logging tests and verify pass**

Run: `cargo nextest run -p advanced-show-control logging`

Expected: PASS.

- [ ] **Step 6: Commit Task 2**

Run:

```bash
git status --short
git diff -- src-tauri/src/logging.rs
git add src-tauri/src/logging.rs
git commit -m "feat: gate diagnostic debug logging"
```

---

### Task 3: Wire Logging Startup And Settings Events

**Files:**
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/settings/actor.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/debug.rs`

**Interfaces:**
- Consumes: `LoggingRuntime::set_extensive_diagnostics_enabled(enabled: bool)` from Task 2.
- Produces: `LoggingRuntime::spawn_settings_watcher(&self, events: broadcast::Receiver<AppEvent>)` and `build_settings_actor(settings_dir, event_bus) -> (SettingsHandle, SettingsActorTask, AppSettings)` so setup can apply the loaded setting before spawning actors.

- [ ] **Step 1: Write failing unit test for settings event application**

Add to `src-tauri/src/logging.rs` tests:

```rust
#[test]
fn diagnostic_file_gate_tracks_extensive_diagnostics_setting() {
    let gate = DiagnosticFileGate::bootstrap_debug();
    let disabled = crate::settings::AppSettings {
        enable_extensive_diagnostics: false,
        ..Default::default()
    };
    let enabled = crate::settings::AppSettings {
        enable_extensive_diagnostics: true,
        ..Default::default()
    };

    apply_settings_to_diagnostic_file_gate(&gate, &disabled);
    assert_eq!(gate.min_level(), LevelFilter::INFO);

    apply_settings_to_diagnostic_file_gate(&gate, &enabled);
    assert_eq!(gate.min_level(), LevelFilter::DEBUG);
}
```

- [ ] **Step 2: Run logging tests and verify failure**

Run: `cargo nextest run -p advanced-show-control logging::tests::diagnostic_file_gate_tracks_extensive_diagnostics_setting`

Expected: compile failure mentioning `apply_settings_to_diagnostic_file_gate` is not defined.

- [ ] **Step 3: Implement settings application helper and watcher**

Add imports in `src-tauri/src/logging.rs`:

```rust
use crate::runtime::events::{AppEvent, log_lagged_subscriber};
use crate::settings::{AppSettings, SettingsEvent};
```

Add helper near `DiagnosticFileGate`:

```rust
fn apply_settings_to_diagnostic_file_gate(gate: &DiagnosticFileGate, settings: &AppSettings) {
    gate.set_extensive_diagnostics_enabled(settings.enable_extensive_diagnostics);
}
```

Add method to `impl LoggingRuntime`:

```rust
pub fn apply_settings(&self, settings: &AppSettings) {
    apply_settings_to_diagnostic_file_gate(&self.diagnostic_file_gate, settings);
}

pub fn spawn_settings_watcher(&self, mut events: broadcast::Receiver<AppEvent>) {
    let gate = self.diagnostic_file_gate.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Settings(SettingsEvent::StateChanged { settings })) => {
                    apply_settings_to_diagnostic_file_gate(&gate, &settings);
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("logging-settings", count);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
        tracing::debug!(
            event = "logging_settings_watcher_stopped",
            "Logging settings watcher stopped"
        );
    });
}
```

- [ ] **Step 4: Return initial settings from settings actor construction**

Edit `src-tauri/src/settings/actor.rs` imports and `build_settings_actor`:

```rust
pub fn build_settings_actor(
    settings_dir: PathBuf,
    event_bus: AppEventBus,
) -> (SettingsHandle, SettingsActorTask, AppSettings) {
    let (tx, rx) = mpsc::channel(32);
    let state = SettingsState::load(settings_dir);
    let initial_settings = state.settings();
    let task = SettingsActorTask {
        rx,
        event_bus,
        state,
    };
    (SettingsHandle::new(tx), task, initial_settings)
}
```

Update all Rust call sites that destructure `build_settings_actor` to accept the third value. In tests or defaults that do not need it, bind it as `_initial_settings`.

- [ ] **Step 5: Reorder normal Tauri setup**

Edit `src-tauri/src/ui/mod.rs` setup so logging starts before settings load and then applies loaded settings:

```rust
.setup(|app| {
    let event_bus = AppEventBus::default();
    let logging_runtime = logging::init_logging(app.handle())?;
    logging_runtime.spawn_settings_watcher(event_bus.subscribe());
    let (show, show_task, show_peers) = build_show_actor(event_bus.clone());
    let settings_dir = app.path().app_config_dir()?;
    let (settings, settings_task, initial_settings) =
        build_settings_actor(settings_dir, event_bus.clone());
    logging_runtime.apply_settings(&initial_settings);
    let lifecycle =
        AppLifecycle::new(event_bus, show.clone(), show_peers, settings.clone());
    show_task.spawn();
    settings_task.spawn();
    app.manage(show);
    app.manage(lifecycle);
    app.manage(settings);
    app.manage(logging_runtime.guard);
    app.manage(logging_runtime.ui_logs);
    menu::install_session_menu(app)?;
    tracing::info!(event = "app_started", "Starting Advanced Show Control");
    Ok(())
})
```

- [ ] **Step 6: Reorder debug app setup**

Apply the same ordering to `src-tauri/src/ui/debug.rs`: initialize logging, spawn watcher, build settings, apply initial settings, then spawn tasks and manage runtime state.

- [ ] **Step 7: Run targeted Rust checks**

Run: `cargo nextest run -p advanced-show-control logging ui::tests::build_app_constructs_builder`

Expected: PASS.

- [ ] **Step 8: Commit Task 3**

Run:

```bash
git status --short
git diff -- src-tauri/src/logging.rs src-tauri/src/settings/actor.rs src-tauri/src/ui/mod.rs src-tauri/src/ui/debug.rs
git add src-tauri/src/logging.rs src-tauri/src/settings/actor.rs src-tauri/src/ui/mod.rs src-tauri/src/ui/debug.rs
git commit -m "feat: apply diagnostics setting to file logs"
```

---

### Task 4: Add The Settings UI Toggle

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/components/SettingsTab.tsx`
- Modify: `ui/src/storybook/mockAppState.ts` if typecheck requires fixture updates
- Modify: `ui/src/components/AppShell.stories.tsx` if story fixtures construct full settings objects
- Create: `ui/src/components/SettingsTab.test.tsx` if no suitable test exists

**Interfaces:**
- Consumes: backend-projected `settings.enableExtensiveDiagnostics: boolean`.
- Produces: full-object settings replacements containing `enableExtensiveDiagnostics`.

- [ ] **Step 1: Add failing TypeScript type usage**

Edit `ui/src/types.ts` `AppSettings` and fixture:

```ts
export type AppSettings = {
  autoLoadLastShowFile: boolean;
  autoSaveSessions: boolean;
  keyboardShortcuts: KeyboardShortcutSettings;
  autoCueNextSceneOnGo: boolean;
  timeDisplay: TimeDisplayFormat;
  faderOverrideSensitivity: number;
  enableExtensiveDiagnostics: boolean;
};
```

Add to `disconnectedAppViewState.settings`:

```ts
enableExtensiveDiagnostics: false,
```

- [ ] **Step 2: Run typecheck and identify fixture failures**

Run: `npm --prefix ui run typecheck`

Expected: FAIL if any full settings fixtures are missing `enableExtensiveDiagnostics`; otherwise PASS for this step.

- [ ] **Step 3: Update Settings tab UI**

Add this row in `ui/src/components/SettingsTab.tsx` under the General settings section after fader override sensitivity:

```tsx
<SettingRow
  help="Write detailed debug diagnostics to disk. Enable only while troubleshooting because log files can grow quickly."
  label="Extensive diagnostics"
  onHelpChange={setActiveHelp}
>
  <ToggleControl
    label="Extensive diagnostics"
    checked={settings.enableExtensiveDiagnostics}
    onChange={(checked) =>
      update((current) => ({
        ...current,
        enableExtensiveDiagnostics: checked,
      }))
    }
  />
</SettingRow>
```

- [ ] **Step 4: Add or update UI test**

If `ui/src/components/SettingsTab.test.tsx` does not exist, create it with a focused test using the project’s existing React test setup. The test should render `SettingsTab` with an `onReplaceSettings` spy and a mocked app state, click the `Extensive diagnostics` toggle, and assert the replacement includes `enableExtensiveDiagnostics: true` while preserving other settings.

Use this assertion shape:

```ts
expect(onReplaceSettings).toHaveBeenCalledWith(
  expect.objectContaining({
    enableExtensiveDiagnostics: true,
    autoLoadLastShowFile: false,
    autoSaveSessions: false,
  }),
);
```

- [ ] **Step 5: Fix typecheck fixture failures**

For each TypeScript error about missing `enableExtensiveDiagnostics`, add:

```ts
enableExtensiveDiagnostics: false,
```

to that full `AppSettings` object unless the story specifically needs the enabled state.

- [ ] **Step 6: Run targeted UI checks**

Run:

```bash
npm --prefix ui run typecheck
npm --prefix ui run test -- SettingsTab
```

Expected: PASS.

- [ ] **Step 7: Commit Task 4**

Run:

```bash
git status --short
git diff -- ui/src/types.ts ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx ui/src/storybook/mockAppState.ts ui/src/components/AppShell.stories.tsx
git add ui/src/types.ts ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx ui/src/storybook/mockAppState.ts ui/src/components/AppShell.stories.tsx
git commit -m "feat: add extensive diagnostics toggle"
```

---

### Task 5: Final Verification And Documentation Alignment

**Files:**
- Modify: `docs/coding-conventions.md` only if implementation changes the documented delivery rule semantics.
- Modify: `docs/roadmap.md` only if the completed-foundation logging bullet needs the new behavior called out.

**Interfaces:**
- Consumes: all previous task commits.
- Produces: verified implementation ready for review.

- [ ] **Step 1: Review docs for necessary updates**

Read these sections:

```text
docs/coding-conventions.md Logging Delivery
docs/roadmap.md Completed Foundation logging bullet
```

If no docs change is needed, leave docs untouched. If a docs update is needed, keep it factual and limited to the new setting.

- [ ] **Step 2: Run formatting checks**

Run: `make fmt`

Expected: PASS.

- [ ] **Step 3: Run standard verification**

Run: `make check`

Expected: PASS.

- [ ] **Step 4: Inspect final worktree**

Run:

```bash
git status --short
git log --oneline -10
```

Expected: only intentional changes are committed; worktree is clean or contains only user-owned unrelated changes.

- [ ] **Step 5: Commit docs alignment if changed**

If Step 1 changed docs, run:

```bash
git diff -- docs/coding-conventions.md docs/roadmap.md
git add docs/coding-conventions.md docs/roadmap.md
git commit -m "docs: update diagnostics logging behavior"
```

If no docs changed, do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: Tasks cover persisted settings, bootstrap `DEBUG`, post-settings `INFO+` default, dynamic `DEBUG+` setting, unchanged UI log behavior, UI toggle, and verification.
- Placeholder scan: No implementation step relies on an undefined future task. The one branch in Task 3 gives an explicit fallback if `SettingsHandle` lacks blocking startup access.
- Type consistency: Rust uses `enable_extensive_diagnostics`; TypeScript uses `enableExtensiveDiagnostics`, matching serde camelCase behavior.
