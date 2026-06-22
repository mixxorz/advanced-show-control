# Native Session Menu Design

## Purpose

Replace the in-app Sessions workflow with normal desktop session-file commands. The app will keep using its current session data format, and the saved file extension is `.ascs`.

## Scope

- Remove the `Sessions` top-level tab.
- Add native application `File` menu commands for session file actions.
- Show the current session name and dirty state in the window title bar.
- Rename the app-owned session file extension to `.ascs` without changing the file schema or JSON format.
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
- `Advanced Show Control - Tour Prep`
- `Advanced Show Control - Tour Prep *`

The top-right LV1 connection control should remain focused on console connection state and system management. Connection workflows such as going offline or switching LV1 systems should continue through the existing connection modal or a later connection-control improvement, not through the File menu.

## Backend Behavior

The existing session schema stays unchanged. Only filenames, dialog filters, default names, backup names, tests, and user-facing copy change to `.ascs` and session terminology.

Opening old session files is not part of this scope. The new file dialog filter should prefer `.ascs`. No migration or compatibility code should be added unless a later requirement asks for it.

Save and open behavior remains otherwise unchanged:

- `New Session` clears the current app session state using the existing new-show behavior.
- `Open Session...` uses the existing open workflow and validation behavior.
- `Save Session` saves to the current path, or behaves like `Save As...` when no current path exists.
- `Save As...` prompts for a destination path.
- Listen Mode and safety-related blocks remain unchanged.

## Frontend And Tauri Integration

The React shell should remove `sessions` from the main tab type, tab list, shell rendering, and tests/stories that expect the Sessions tab. The existing `SessionsTab` component and related stories can be deleted if unused after the tab removal.

The Tauri app menu should be implemented in the backend Tauri adapter layer during `build_app` setup. Add a small menu module under `src-tauri/src/ui/` that builds the native `File` menu and handles menu events by command ID.

Menu-triggered commands should not go through React. Menu handlers should obtain `AppLifecycle` from Tauri managed state, open the same `rfd` dialogs where needed, and send the same `ShowCommand` mailbox messages used by the existing Tauri command adapters. Do not extract shared functions for this work; a small amount of repeated adapter code is preferable to extra indirection here.

Native menu command IDs should be stable constants, for example:

- `session:new`
- `session:open`
- `session:save`
- `session:save-as`

Menu event handlers should spawn async work on the Tauri async runtime and log failures through tracing. They do not need to return errors to the menu system. User-visible command failures should still become visible through the existing app log/projection path where the underlying show command already emits state or logs. If a command only fails before reaching show state, such as dialog creation failure, log it with enough context for diagnosis.

The window title should be updated from the React runtime. `AppRuntime` already receives the projected `showFileName`, `showFilePath`, and `showFileDirty` values through `AppViewState`, so it can derive the title without creating a second state source. Add a small title-formatting helper and call `getCurrentWindow().setTitle(title)` from `@tauri-apps/api/window` in an effect when those projection fields change. Tests can cover the pure formatting helper without requiring a Tauri window.

The current runtime initializes `activeTab` in memory and does not persist selected tabs, so no removed-tab migration or fallback behavior is required. Remove `sessions` from the `MainTab` type, tab list, shell rendering, tests, stories, and fixtures.

## Technical Decisions

- Native `File` menu is the canonical location for session actions.
- No in-app session dropdown or top-bar session control will be added.
- React owns title formatting because it already observes projected session state.
- Backend menu handlers own native menu events because Tauri menu events are backend events.
- Menu actions and React commands use the same show actor mailbox commands without adding a shared helper layer.
- Existing `rfd` dialogs remain the file picker implementation for both menu and command paths.
- `.ascs` is the only preferred extension. Old extension compatibility is out of scope.

## Testing

Add or update tests for:

- Top tab bar no longer rendering `Sessions`.
- Shell tab typing and stories/fixtures after removing `sessions`.
- `.ascs` dialog filters/default filenames and backup filename behavior.
- Window title formatting for untitled, saved, and dirty sessions.
- Menu command IDs and menu construction if the app menu logic is testable without launching the full Tauri app.
- Menu handlers send the intended `ShowCommand` variants and use `.ascs` dialogs.

Run the relevant frontend and Rust checks after implementation.

## Non-Goals

- No in-app session dropdown.
- No show-file schema change.
- No old extension migration or compatibility support.
- No redesign of the connection modal.
- No changes to LV1 scene/session handling.
