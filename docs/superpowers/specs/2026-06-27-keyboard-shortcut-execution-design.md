# Keyboard Shortcut Execution Design

## Context

The app already captures, displays, persists, and projects configurable keyboard shortcuts for `GO` and `Cue`. The previous shortcut-capture work intentionally deferred shortcut execution. The app also already owns session file actions through the native Tauri File menu, but those menu items do not yet expose standard keyboard accelerators.

This change wires shortcut behavior without changing the persisted settings shape or adding new safety paths.

## Goals

- Execute the saved `GO` keyboard shortcut from the React keyboard layer.
- Execute the saved `Cue` keyboard shortcut for the selected cue-list entry from the React keyboard layer.
- Detect shortcut assignment conflicts when capturing `GO` or `Cue` shortcuts, including conflicts with fixed File menu accelerators.
- Add standard native keyboard accelerators for New Session, Open Session, Save Session, and Save As.
- Keep shortcut capture higher priority than execution so editing a shortcut never triggers an app action.
- Reuse existing app commands and menu handlers instead of introducing parallel command paths.
- Treat one physical GO key press as at most one recall, including after a fast recall completes while the key remains held.
- Do not execute GO or Cue while the keyboard event originates from an editable control or dialog.
- Canonicalize persisted shortcut key labels so manually edited settings match capture, display, and conflict semantics.

## Non-Goals

- Do not add shortcut conflict resolution beyond rejecting a newly captured duplicate.
- Do not add global OS-level shortcuts that fire while the app is unfocused.
- Do not change Rust settings types or settings persistence.
- Do not implement Cue Lists auto-advance behavior.

## Recommended Approach

Split responsibilities by command type.

User-configurable show-operation shortcuts stay in React. `GO` and `Cue` are settings-backed app actions, and the existing `KeyboardProvider` already centralizes focused-window keydown handling, priority dispatch, and shortcut capture.

Standard file shortcuts stay in the native Tauri File menu. The menu already owns New, Open, Save, and Save As behavior, including dialogs and actor command routing. Adding accelerators there preserves desktop expectations and avoids duplicating file command logic in React.

## User Interaction

`GO` uses the saved `settings.keyboardShortcuts.go` shortcut. When pressed while the app window is focused, it recalls the currently cued entry from the active cue list if that entry resolves to a scene. This matches the bottom status bar `GO` button and calls the existing `recallCuedCue` command.

`Cue` uses the saved `settings.keyboardShortcuts.cue` shortcut. When pressed while the Cue Lists tab is active, it cues the currently selected cue-list entry if one exists. This matches the Cue Lists tab's Cue button and calls the existing `cueEntry` command.

Shortcut capture remains modal within the keyboard layer. While a shortcut input is capturing, the capture handler consumes delivered key events before the execution handler sees them.

Action shortcuts are inactive when the key event originates from an `input`, `textarea`, `select`, content-editable element, or an element inside a dialog. Shortcut capture remains active in those contexts because it has higher priority and is an explicit assignment mode.

OS-generated repeat keydown events are consumed when they match GO or Cue but do not dispatch another action. The existing GO in-flight guard separately prevents multiple non-repeat keydowns from overlapping before command completion.

If the user captures a shortcut that is already assigned to another configurable shortcut action or a fixed File menu accelerator, Settings rejects the new assignment, leaves the existing shortcut value unchanged, and shows red inline text to the right of that row's keyboard input capture box. The message should name the conflicting action, such as `Already assigned to GO`, `Already assigned to Cue`, `Already assigned to Save Session`, or `Already assigned to Save As`. The message clears when the user starts another capture, successfully saves a non-conflicting shortcut, or leaves the Settings tab.

The native File menu exposes these accelerators:

- New Session: `CmdOrCtrl+N`
- Open Session: `CmdOrCtrl+O`
- Save Session: `CmdOrCtrl+S`
- Save As: `CmdOrCtrl+Shift+S`

## Frontend Architecture

Split shortcut execution according to existing UI state ownership. `AppRuntime` owns the global `GO` handler because projected cue-list state and `recallCuedCue` are available there. `CueListsTab` owns the `Cue` handler because cue-list entry selection is local UI state there; selection should not be lifted solely for shortcut execution.

The handlers should:

- Register with `KeyboardProvider` at a priority below shortcut capture.
- Give `GO` a higher priority than `Cue`.
- Compare keydown events against the projected `AppSettings.keyboardShortcuts` values using the same comparable key labels that shortcut capture stores.
- Consume a matching `GO` shortcut even when GO is unavailable so a duplicate Cue binding can never run instead.
- Return `handled` only when `Cue` actually dispatches; unmatched or unavailable Cue shortcuts remain ignored.

The handler should not call Tauri commands directly. It should call the existing `AppCommands` methods that already route through `AppRuntime` error handling.

Settings should perform conflict detection before calling `replaceAppSettings`. The check compares the shortcut being captured against other configurable shortcut actions and a small hardcoded React list of fixed File menu accelerators. The fixed list exists only for validation and messaging; native menu execution remains owned by Tauri.

The hardcoded fixed shortcuts for conflict validation are:

- New Session: `CmdOrCtrl+N`
- Open Session: `CmdOrCtrl+O`
- Save Session: `CmdOrCtrl+S`
- Save As: `CmdOrCtrl+Shift+S`

For matching, `CmdOrCtrl` means `meta: true` on macOS and `control: true` on non-macOS platforms, with the other command modifier false. This mirrors the display/platform split already used by shortcut formatting.

## Matching Rules

A keyboard event matches a saved shortcut when the comparable key label and all four modifier booleans are equal. The implementation should share key-label normalization with shortcut capture so execution uses the same conventions as stored settings: `Space`, `Enter`, uppercase letters, and unshifted digit keys with `shift: true`.

Rust settings normalization trims shortcut labels, uppercases one-character labels, and canonicalizes known named labels such as `space`, `enter`, and arrow keys. Empty labels still fall back to the action default. Frontend display, matching, and conflict detection consume this canonical projected representation rather than presenting a lowercase value as valid while comparing it case-sensitively.

If two configured shortcuts are identical because of a pre-existing or manually edited settings file, `GO` takes strict precedence over `Cue` because it is the primary show-operation action. A key matching GO is consumed even when no valid cue is available, so Cue never runs as a fallback. New duplicates or fixed-file-shortcut conflicts captured through Settings are rejected before persistence.

## Native Menu Architecture

Update `src-tauri/src/ui/menu.rs` to pass accelerators to the existing `MenuItem::with_id` calls. The menu event handlers should remain unchanged and continue to route through the current show actor commands and file dialogs.

This keeps file operations native and avoids a second React implementation of menu-owned behavior.

## Error Handling And Safety

Unavailable shortcut actions do not force a command. `GO` consumes its matching key without dispatching when the active cue list, cued entry, or referenced scene is unavailable. `Cue` is ignored when the Cue Lists tab has no selected entry.

Any command failure from a dispatched shortcut flows through the same `AppRuntime` command error state used by button clicks. Shortcut execution must not bypass lockout, scene identity validation, stale-state checks, generation guards, or backend command validation.

Native file accelerator failures continue to be logged through the existing menu handler warnings.

## Testing

Add frontend tests for:

- Pressing the configured `GO` shortcut recalls the cued cue-list entry.
- Pressing the configured `GO` shortcut does nothing when the active cue list, cued entry, or referenced scene is unavailable.
- Holding the configured `GO` shortcut dispatches at most one recall even when the first recall completes before OS key repeat begins.
- Pressing the configured `Cue` shortcut in the Cue Lists tab cues the selected entry.
- Pressing the configured `Cue` shortcut does nothing when no cue-list entry is selected.
- Typing a matching shortcut in an editable control or dialog does not execute GO or Cue.
- Shortcut capture preempts shortcut execution.
- `GO` wins when `GO` and `Cue` are configured to the same shortcut, including when GO is unavailable.
- Lowercase and case-variant shortcut labels loaded from settings are canonicalized before projection and execute consistently with their displayed value.
- Capturing a shortcut already assigned to the other action leaves settings unchanged and shows inline red conflict text beside the capture control.
- Capturing a shortcut reserved by a fixed File menu accelerator leaves settings unchanged and shows inline red conflict text naming the file action.
- Conflict text clears after a successful non-conflicting capture.

Add Rust pure unit coverage for the File menu item accelerators if the existing menu tests can inspect the constructed accelerator values without launching the app. If not, keep the Rust change minimal and rely on existing stable menu-id tests plus manual code inspection.

Run targeted frontend tests first, then `npm --prefix ui run typecheck`, `npm --prefix ui run test`, and the smallest relevant Rust check for any menu test changes.
