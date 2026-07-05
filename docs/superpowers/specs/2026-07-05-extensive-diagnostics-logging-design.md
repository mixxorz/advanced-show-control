# Extensive Diagnostics Logging Setting Design

## Purpose

Add an app setting that controls high-volume diagnostic logging. Diagnostic log files may still be created when the setting is off, but after settings load they should only receive `INFO`, `WARN`, and `ERROR` events by default. `DEBUG` events should be written during bootstrap and then continue only when the setting is enabled.

## Scope

This change affects app settings, logging setup, and the Settings UI. It does not change LV1 command routing, fade execution, scene recall safety behavior, frontend log projection, or show-file storage.

## Behavior

- Add `enableExtensiveDiagnostics` to frontend settings and `enable_extensive_diagnostics` to Rust settings.
- Default the setting to `false` for new installs and existing partial `settings.json` files.
- Persist the setting in the existing app-config `settings.json` file through the current full-object settings replacement flow.
- Start diagnostic file logging at `DEBUG` during bootstrap so startup and settings-load failures are captured.
- After settings load completes, keep diagnostic file logging active for `INFO`, `WARN`, and `ERROR` events regardless of the setting.
- After settings load completes, write `DEBUG` events to the diagnostic file only while the setting is enabled.
- Keep frontend Logs tab behavior unchanged: frontend-facing log state continues to receive `INFO`, `WARN`, and `ERROR` through the existing UI log sink.
- Keep stdout logging unchanged unless implementation discovers a direct conflict; this setting is specifically for diagnostic file volume.

## Architecture

`settings` remains the owner of persisted app preferences. The setting is added to `AppSettings` and normalized through the existing settings path. Existing settings files remain valid because `AppSettings` already deserializes missing fields from defaults.

`logging` remains the owner of tracing setup. `init_logging` should install a diagnostic file filter that starts at `DEBUG` during bootstrap, then changes between `INFO` and `DEBUG` behavior after settings load completes. The logging runtime should listen for `SettingsEvent::StateChanged` facts on `AppEventBus` and update the dynamic file-log gate when the setting changes.

Startup setup should initialize logging before settings load, then apply the loaded settings value to the dynamic file-log gate as soon as settings construction completes. A small logging runtime API for applying the current diagnostics setting is acceptable if it keeps the actor handle dumb and avoids duplicating settings-file reads.

## Data Flow

1. App startup initializes logging with the diagnostic file gate set to `DEBUG`.
2. App startup loads settings from the app-config settings file.
3. Tauri setup applies the loaded `enable_extensive_diagnostics` value to the logging runtime.
4. Logging keeps the file sink and UI sink installed once for the app lifetime.
5. After settings are applied, the file sink writes `INFO+` events when extensive diagnostics is off and `DEBUG+` events when it is on.
6. When the Settings tab toggles the setting, the frontend submits a full `AppSettings` replacement.
7. The settings actor persists the normalized settings and publishes `SettingsEvent::StateChanged`.
8. The logging settings watcher receives the event and updates the dynamic diagnostics gate immediately.

## Error Handling

Existing logging initialization errors should continue to fail startup. If settings loading fails, the app should use normalized default settings and lower the file-log gate to `INFO+` after that load attempt. Settings persistence failures should continue to surface through the existing settings command error path. If the logging settings watcher lags or exits, it must not affect mixer control behavior; the worst acceptable outcome is that the file-log debug gate remains at its previous value until restart.

## UI

Add a Settings tab toggle for extensive diagnostics. The help text should make the volume trade-off explicit, for example: enabling this writes detailed debug diagnostics to disk and can create larger log files. The control should preserve the existing Settings tab layout and interaction model.

## Testing

Rust behavior should use pure unit tests where possible:

- Settings default includes `enable_extensive_diagnostics == false`.
- Partial settings files deserialize with the diagnostics setting defaulted to `false`.
- The diagnostic file filter starts at `DEBUG+`, allows `INFO+` after settings load when the setting is off, and allows `DEBUG+` when the setting is on.

Actor-style tests are not required unless the logging watcher cannot be tested through a small isolated function. Frontend coverage should update existing Settings tab/story fixtures or add a focused Vitest assertion that the new toggle submits the expected full settings object.

## Non-Goals

- Do not disable diagnostic file creation when the setting is off.
- Do not change frontend Logs tab severity behavior.
- Do not add trace-level logging.
- Do not change LV1 protocol logging call sites except through the file-log level gate.
- Do not add show-file fields for this app-level preference.
