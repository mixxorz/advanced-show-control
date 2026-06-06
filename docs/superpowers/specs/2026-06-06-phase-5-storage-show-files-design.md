# Phase 5 Storage And Show Files Design

## Purpose

Phase 5 persists captured fade setups as portable show files. The app remains a fader-fade overlay, not an LV1 scene manager.

The MVP storage rule is intentionally conservative: a loaded show file must match the currently connected LV1 scene list and channel topology exactly. Saved entries that do not match are deleted during load and logged. Remapping, scene rename handling, channel rename handling, scene reorder handling, duplicate-name handling, durable identity matching, and autosave are deferred to a later phase.

## Terminology

Use **show file** in the UI and user-facing docs. The file contents are JSON, but the extension should be app-specific, such as `.lv1show`.

Avoid **project file** in new UI copy unless referring to internal implementation concepts.

## Architecture

`ShellState` remains the Rust-owned source of truth. React renders `AppViewState` and sends commands through Tauri. React does not serialize or validate show files.

Phase 5 adds a Rust storage layer that can:

- Convert in-memory scene fade configs to a versioned show file DTO.
- Parse a show file DTO from disk.
- Validate loaded scene configs and targets against the current LV1 mirror.
- Delete non-exact scene configs and targets during load.
- Track current show file path and dirty state.
- Save by writing JSON to a temporary file in the destination directory, then renaming it over the target path after the write succeeds.
- Create backup copies before overwriting existing show files.

## Default Locations

Native dialogs should default to a platform-aware user-facing app folder under Documents:

- macOS: `~/Documents/LV1 Scene Fade Utility/`
- Windows: `%USERPROFILE%\Documents\LV1 Scene Fade Utility\`
- Linux: `$XDG_DOCUMENTS_DIR/LV1 Scene Fade Utility/`, falling back to `~/Documents/LV1 Scene Fade Utility/`, then home if needed

The folder may be created lazily when Open or Save As first needs it. Users can still open and save show files anywhere.

Internal app data should use platform app data locations:

- macOS: `~/Library/Application Support/LV1 Scene Fade Utility/`
- Windows: `%APPDATA%\LV1 Scene Fade Utility\`
- Linux: `$XDG_DATA_HOME/lv1-scene-fade-utility/`, falling back to `~/.local/share/lv1-scene-fade-utility/`

Backups live under internal app data, for example `backups/`. They are not written beside the user's show file.

## Show File Shape

The MVP show file should store only what is needed to restore, validate, and safely run fade setups.

Example:

```json
{
  "schemaVersion": 1,
  "appVersion": "0.1.0",
  "savedAt": "1780750000000",
  "safety": {
    "lockout": false
  },
  "sceneFadeConfigs": [
    {
      "sceneIndex": 12,
      "sceneName": "Song 3",
      "fadeEnabled": true,
      "durationMs": 4000,
      "fadeTargets": [
        {
          "group": 0,
          "channel": 4,
          "channelName": "Lead Vox",
          "targetDb": -8.5,
          "enabled": true,
          "updatedAt": "1780750000000"
        }
      ]
    }
  ]
}
```

No curve field is stored in Phase 5 because the implemented fade engine currently supports only `FadeCurve::Linear`, which means linear movement in measured LV1 fader-position space.

Do not store logs, live LV1 connection state, current LV1 scene, live fader values, selected tab, transient warnings, or generated UI selection IDs.

## State Model

`AppViewState` should gain show-file status fields for rendering and command feedback:

- current show file name or `Untitled Show`
- current show file path if available
- dirty state
- last saved timestamp if available

`SceneFadeConfig` should gain `durationMs`, defaulting to `4000` for existing in-memory configs and newly reconciled LV1 scenes.

`FadeTarget` should gain `channelName`, captured from the LV1 channel mirror when Listen Mode creates the target. Existing UI display can still show current channel names from the LV1 mirror, but saved validation uses captured `channelName`.

## Commands

Phase 5 adds these Tauri commands:

- `new_show_file() -> AppViewState`
- `open_show_file_dialog() -> AppViewState`
- `save_show_file() -> AppViewState`
- `save_show_file_as_dialog() -> AppViewState`

Command behavior:

- `new_show_file` clears fade configs and show file path, resets dirty state, and then reconciles default empty configs from the current LV1 scene list if connected.
- `open_show_file_dialog` uses a native dialog starting in the default show folder, parses the selected `.lv1show` file, validates it against current LV1 state, applies kept configs, logs deletions, stores the path, and sets dirty if anything was deleted.
- `save_show_file` writes to the current path. If no current path exists, it behaves like Save As.
- `save_show_file_as_dialog` uses a native dialog starting in the default show folder and stores the chosen path as the current show file path.

Opening a show file should be blocked if no LV1 scene list or channel list is available, because strict validation cannot run safely without both.

All commands that mutate fade setup state should mark the show file dirty after Phase 5, including Listen Mode capture, target removal, target enabled toggles, scene fade enabled toggles, and duration changes.

## Load Validation

Loading uses strict exact matching:

- Keep a saved scene config only when current LV1 has the same scene index and scene name.
- Delete a saved scene config when the scene index is missing, the name differs, or the matching scene is otherwise not exact.
- Keep a saved target only when current LV1 has the same group, channel, and channel name.
- Delete a saved target when the group/channel is missing or the channel name differs.

Every deletion should be logged with enough detail to diagnose what was removed. A separate load report UI is not required for Phase 5.

If any deletion occurs during load, mark the show file dirty. The original file is not overwritten unless the engineer explicitly saves.

## Saving And Backups

Saving writes a fresh JSON file using the current in-memory fade setup.

Before overwriting an existing show file, create a backup under the internal app data `backups/` directory. Backup names should include:

- timestamp
- show file stem
- a collision-safe suffix if needed

No autosave is included in Phase 5. Backup pruning can be omitted for MVP unless implementation makes a simple retention policy cheap and safe.

## UI Design

Add compact show file controls near the header:

- show file name or `Untitled Show`
- dirty indicator
- `New`
- `Open`
- `Save`
- `Save As`

Use existing command error display for failures. Load-time deletion details are visible through the Logs tab.

The Scene tab should expose a simple per-scene duration control in milliseconds or seconds. Updating it changes `durationMs` and marks the show file dirty.

## Safety

Show file load must never send commands to LV1.

Strict deletion on load is destructive only to the in-memory copy. It does not alter the file on disk until the user explicitly saves.

If Listen Mode is active, opening a show file, creating a new show file, or saving should fail until Listen Mode is stopped. This avoids writing or replacing configs while fader notifications are mutating them.

## Testing

Rust tests should cover:

- show file DTO serialization and deserialization
- default duration and captured channel name persistence
- load blocked without LV1 scenes and channels
- exact scene match keeps config
- scene name mismatch deletes config and logs it
- missing scene deletes config and logs it
- exact channel target match keeps target
- channel name mismatch deletes target and logs it
- missing channel deletes target and logs it
- load marks dirty when deletions occur
- load does not mark dirty when no deletions occur
- save writes valid JSON
- save creates a backup before overwriting an existing file
- setup mutations mark dirty

Frontend verification should include TypeScript build and a manual check that native dialogs are wired, header show-file status updates, and command errors remain visible.

## Out Of Scope

Phase 5 does not include:

- autosave
- remapping
- scene rename handling
- channel rename handling
- scene reorder handling
- duplicate-name handling
- durable scene identity beyond exact index/name matching
- storing fade curves
- SQLite
- automatic fade triggering on LV1 recall
