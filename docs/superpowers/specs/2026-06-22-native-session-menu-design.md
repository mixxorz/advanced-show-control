# Native Session Menu Design

## Purpose

Replace the in-app Sessions workflow with normal desktop session-file commands. The app will keep using its current show-file data format, but the released user-facing concept is a session and the saved file extension is `.adsc`.

## Scope

- Remove the `Sessions` top-level tab.
- Add native application `File` menu commands for session file actions.
- Show the current session name and dirty state in the window title bar.
- Rename the app-owned session file extension from `.lv1show` to `.adsc` without changing the file schema or JSON format.
- Keep LV1 connection controls in the app UI.
- Do not add an in-app session dropdown in this scope.

## User Experience

The top tab bar should no longer include `Sessions`. The remaining app navigation should continue to use the existing large, high-contrast console style.

Session file actions should live in the native desktop `File` menu:

- `New Session`
- `Open Session...`
- `Save Session`
- `Save As...`

The title bar should show the current session name so the session is visible without adding another control to the top bar. The dirty marker should match existing dirty-state behavior.

Examples:

- `Advanced Show Control - Untitled *`
- `Advanced Show Control - Tour Prep.adsc`
- `Advanced Show Control - Tour Prep.adsc *`

The top-right LV1 connection control should remain focused on console connection state and system management. Connection workflows such as going offline or switching LV1 systems should continue through the existing connection modal or a later connection-control improvement, not through the File menu.

## Backend Behavior

The existing show/session file schema stays unchanged. Only filenames, dialog filters, default names, backup names, tests, and user-facing copy change from `.lv1show` and show-file terminology to `.adsc` and session terminology where appropriate.

Opening old `.lv1show` files is not part of this scope. The new file dialog filter should prefer `.adsc`. No migration or compatibility code should be added unless a later requirement asks for it.

Save and open behavior remains otherwise unchanged:

- `New Session` clears the current app session state using the existing new-show behavior.
- `Open Session...` uses the existing open workflow and validation behavior.
- `Save Session` saves to the current path, or behaves like `Save As...` when no current path exists.
- `Save As...` prompts for a destination path.
- Listen Mode and safety-related blocks remain unchanged.

## Frontend And Tauri Integration

The React shell should remove `sessions` from the main tab type, tab list, shell rendering, and tests/stories that expect the Sessions tab. The existing `SessionsTab` component and related stories can be deleted if unused after the tab removal.

The Tauri app menu should dispatch the same commands used by the existing React command adapter layer. Menu-triggered commands must not duplicate business logic. They should route through the same Tauri command or command-bus path as UI-triggered session operations.

The window title should update when the projected session file name, current path, or dirty state changes. The title update can live on the frontend side if it can call the Tauri window API from projected app state, or on the backend/projector side if that fits the existing Tauri setup better. The key requirement is that the title remains projection-driven and does not invent separate session state.

## Testing

Add or update tests for:

- Top tab bar no longer rendering `Sessions`.
- Shell tab typing and fallback behavior after removing `sessions`.
- `.adsc` dialog filters/default filenames and backup filename behavior.
- Window title formatting for untitled, saved, and dirty sessions.
- Menu command dispatch if the app menu logic is testable in the current Tauri setup.

Run the relevant frontend and Rust checks after implementation.

## Non-Goals

- No in-app session dropdown.
- No show-file schema change.
- No `.lv1show` migration or compatibility support.
- No redesign of the connection modal.
- No changes to LV1 scene/session handling.
