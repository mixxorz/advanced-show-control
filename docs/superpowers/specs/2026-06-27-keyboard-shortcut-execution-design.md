# Keyboard Shortcut Execution Design

## Context

The app already captures, displays, persists, and projects configurable keyboard shortcuts for `GO` and `Cue`. The previous shortcut-capture work intentionally deferred shortcut execution. The app also already owns session file actions through the native Tauri File menu, but those menu items do not yet expose standard keyboard accelerators.

This change wires shortcut behavior without changing the persisted settings shape or adding new safety paths.

## Goals

- Execute the saved `GO` keyboard shortcut from the React keyboard layer.
- Execute the saved `Cue` keyboard shortcut from the React keyboard layer.
- Add standard native keyboard accelerators for New Session, Open Session, Save Session, and Save As.
- Keep shortcut capture higher priority than execution so editing a shortcut never triggers an app action.
- Reuse existing app commands and menu handlers instead of introducing parallel command paths.

## Non-Goals

- Do not add shortcut conflict detection or warnings.
- Do not add global OS-level shortcuts that fire while the app is unfocused.
- Do not change Rust settings types or settings persistence.
- Do not implement Cue Lists behavior for auto-advance or cue-list-specific GO semantics.

## Recommended Approach

Split responsibilities by command type.

User-configurable show-operation shortcuts stay in React. `GO` and `Cue` are settings-backed app actions, and the existing `KeyboardProvider` already centralizes focused-window keydown handling, priority dispatch, and shortcut capture.

Standard file shortcuts stay in the native Tauri File menu. The menu already owns New, Open, Save, and Save As behavior, including dialogs and actor command routing. Adding accelerators there preserves desktop expectations and avoids duplicating file command logic in React.

## User Interaction

`GO` uses the saved `settings.keyboardShortcuts.go` shortcut. When pressed while the app window is focused, it recalls the currently cued scene if one exists and recall is available. This matches the bottom status bar `GO` button.

`Cue` uses the saved `settings.keyboardShortcuts.cue` shortcut. When pressed while the app window is focused, it cues the currently selected scene if one exists, the scene is linked to an LV1 scene, and cue is available. This matches the selected scene header's Cue button behavior.

Shortcut capture remains modal within the keyboard layer. While a shortcut input is capturing, the capture handler consumes delivered key events before the execution handler sees them.

The native File menu exposes these accelerators:

- New Session: `CmdOrCtrl+N`
- Open Session: `CmdOrCtrl+O`
- Save Session: `CmdOrCtrl+S`
- Save As: `CmdOrCtrl+Shift+S`

## Frontend Architecture

Add a focused-window shortcut execution hook near the app runtime where both projected state and app commands are available.

The handler should:

- Register with `KeyboardProvider` at a priority below shortcut capture.
- Compare keydown events against the projected `AppSettings.keyboardShortcuts` values using the same comparable key labels that shortcut capture stores.
- Return `handled` only when it actually dispatches an app action.
- Return `ignored` for unmatched shortcuts or unavailable actions so other handlers and normal browser behavior can continue.

The handler should not call Tauri commands directly. It should call the existing `AppCommands` methods that already route through `AppRuntime` error handling.

## Matching Rules

A keyboard event matches a saved shortcut when the comparable key label and all four modifier booleans are equal. The implementation should share key-label normalization with shortcut capture so execution uses the same conventions as stored settings: `Space`, `Enter`, uppercase letters, and unshifted digit keys with `shift: true`.

If two configured shortcuts are identical, `GO` should take precedence over `Cue` because it is the primary show-operation action. Conflict detection is deferred.

## Native Menu Architecture

Update `src-tauri/src/ui/menu.rs` to pass accelerators to the existing `MenuItem::with_id` calls. The menu event handlers should remain unchanged and continue to route through the current show actor commands and file dialogs.

This keeps file operations native and avoids a second React implementation of menu-owned behavior.

## Error Handling And Safety

Unavailable shortcut actions are ignored instead of forcing a command. For example, `GO` does nothing when there is no cued scene, and `Cue` does nothing when the selected scene is unlinked or missing.

Any command failure from a dispatched shortcut flows through the same `AppRuntime` command error state used by button clicks. Shortcut execution must not bypass lockout, scene identity validation, stale-state checks, generation guards, or backend command validation.

Native file accelerator failures continue to be logged through the existing menu handler warnings.

## Testing

Add frontend tests for:

- Pressing the configured `GO` shortcut recalls the cued scene.
- Pressing the configured `GO` shortcut does nothing when no scene is cued.
- Pressing the configured `Cue` shortcut cues the selected linked scene.
- Pressing the configured `Cue` shortcut does nothing for no selection or an unlinked selected scene.
- Shortcut capture preempts shortcut execution.
- `GO` wins when `GO` and `Cue` are configured to the same shortcut.

Add Rust pure unit coverage for the File menu item accelerators if the existing menu tests can inspect the constructed accelerator values without launching the app. If not, keep the Rust change minimal and rely on existing stable menu-id tests plus manual code inspection.

Run targeted frontend tests first, then `npm --prefix ui run typecheck`, `npm --prefix ui run test`, and the smallest relevant Rust check for any menu test changes.
