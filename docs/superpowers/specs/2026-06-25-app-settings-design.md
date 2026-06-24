# App Settings Design

## Scope

Add app-level settings infrastructure and frontend controls. Settings are application preferences, not show/session data. They are stored in the Tauri app config directory as `settings.json`, loaded on app startup, and saved immediately after each changed settings replacement.

This slice establishes the final Rust data model and editing surface. The settings do not need to affect runtime behavior yet.

## Settings Model

`AppSettings` contains:

- `auto_load_last_show_file: bool`
- `auto_save_sessions: bool`
- `keyboard_shortcuts: KeyboardShortcutSettings`
- `auto_cue_next_scene_on_go: bool`
- `time_display: TimeDisplayFormat`
- `fader_override_sensitivity: u8`

`KeyboardShortcutSettings` contains:

- `go: KeyboardShortcut`
- `cue: KeyboardShortcut`

`KeyboardShortcut` contains:

- `key: String`
- `modifiers: KeyboardShortcutModifiers`

`KeyboardShortcutModifiers` contains:

- `shift: bool`
- `control: bool`
- `alt: bool`
- `meta: bool`

`TimeDisplayFormat` is an enum:

- `TwelveHour`
- `TwentyFourHour`

Defaults:

- `auto_load_last_show_file: false`
- `auto_save_sessions: false`
- `keyboard_shortcuts.go: Space`
- `keyboard_shortcuts.cue: C`
- `auto_cue_next_scene_on_go: false`
- `time_display: TwentyFourHour`
- `fader_override_sensitivity: 9`

## Rust Module

Add `src-tauri/src/settings/` with the same actor-oriented conventions as existing core modules:

- `actor.rs`
- `commands.rs`
- `events.rs`
- `handle.rs`
- `state.rs`
- `types.rs`
- `mod.rs`

The module root exports only the public interface:

- `build_settings_actor`
- `SettingsActorTask`
- `SettingsCommand`
- `SettingsCommandResult`
- `SettingsEvent`
- `SettingsHandle`
- settings model types

## Actor Interface

The settings actor owns all settings state and file persistence.

Commands:

```rust
pub enum SettingsCommand {
    GetSettings {
        reply: oneshot::Sender<AppSettings>,
    },
    ReplaceSettings {
        settings: AppSettings,
        reply: Option<oneshot::Sender<Result<SettingsCommandResult, String>>>,
    },
}
```

Result:

```rust
pub struct SettingsCommandResult {
    pub changed: bool,
}
```

Events:

```rust
pub enum SettingsEvent {
    StateChanged {
        settings: AppSettings,
    },
}
```

The frontend always replaces the full settings object. There are no per-setting public commands.

## Persistence

The settings actor loads `<app config dir>/settings.json` when it starts. If the file is missing, unreadable, corrupt, or contains invalid values, the actor normalizes to safe defaults.

On `ReplaceSettings`, the actor normalizes the submitted settings, compares them with current state, and writes the full settings JSON immediately when changed. After a successful write, it publishes `SettingsEvent::StateChanged`.

Normalization rules:

- Clamp `fader_override_sensitivity` to `1..=10`.
- Trim shortcut keys.
- Use the action default for empty shortcut keys.
- Ignore unknown JSON fields during load.
- Use defaults for missing settings fields.

Settings persistence is independent from `.ascs` session files. Settings changes do not mark the current session dirty.

## Projection And Tauri Boundary

`AppViewState` gains a `settings: AppSettings` field. The projector obtains initial settings with `SettingsCommand::GetSettings` and updates its cache from `SettingsEvent::StateChanged`.

Add one Tauri command for frontend editing:

```rust
#[tauri::command]
pub async fn replace_app_settings(settings: AppSettings) -> Result<SettingsCommandResult, String>
```

The command is a thin adapter. It obtains the app-lifetime settings handle, sends `SettingsCommand::ReplaceSettings`, awaits the reply, and maps errors into frontend-safe strings.

## Frontend Controls

Replace the Settings placeholder tab with a real settings tab showing controls for:

- Auto load last show file
- Auto save sessions
- GO keyboard shortcut
- Cue keyboard shortcut
- Auto cue next scene on GO
- Time display, 12 or 24 hour
- Fader override sensitivity, 1 through 10

Each control derives a full next `AppSettings` object and calls `replace_app_settings`. The controls edit and persist settings, but the settings do not yet alter runtime behavior.

## Testing

Rust tests cover:

- Defaults, including sensitivity `9`.
- Normalization of sensitivity and shortcuts.
- Loading missing, partial, and invalid settings files.
- Immediate save on changed replacement.
- No save/event when replacement normalizes to the current state.

Rust tests must use only these two test styles:

- Pure unit tests that call functions directly and have no side effects.
- Actor tests that interact only through the actor mailbox and `AppEventBus`.

Tests must not directly mutate actor internals, inspect private actor state, or test side-effecting behavior except through actor commands and published events.

Frontend tests cover:

- Settings tab renders current projected settings.
- Each control calls the full-object replace command with the expected next settings object.
- Sensitivity control is bounded to 1 through 10.

## Documentation

Update project documentation as part of implementation:

- `docs/architecture.md` should list the settings actor as an app-lifetime state owner, describe its mailbox interface, app-config JSON persistence, projection path, and startup loading behavior.
- `docs/roadmap.md` should reflect that the Settings tab and app-level settings persistence slice has been implemented, while behavior wiring such as auto-load, auto-save, shortcuts, auto-cue, time display usage, and sensitivity enforcement remains future work unless completed in a later slice.
