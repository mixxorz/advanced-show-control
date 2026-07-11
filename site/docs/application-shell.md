# Application Shell

## Screen Overview

The application shell consists of a top navigation bar, a central work area, and a bottom status bar. The active tab determines the content of the work area.

![The application shell with the Scenes tab active.](assets/screenshots/application-shell.png)

## Navigation

Select a top-bar tab to change the central work area:

| Tab | Visible content |
| --- | --- |
| **Scenes** | Scene library, selected-scene controls, fade duration, and scope controls. |
| **Cue Lists** | Cue-list management and cue entries. |
| **Events** | Placeholder content. |
| **Logs** | Application log entries. |
| **Settings** | Application settings. |

Sessions are not a shell-navigation tab. Use the native **File** menu to create, open, and save sessions.

## Connection Control

The top bar displays the current connection state and a console control. When connected, the control displays the connected console name. Select the control to open the **Connect to LV1** dialog. The dialog can be opened while connected or offline.

The top-bar status is **Connected**, **Connecting**, or **Offline**. Use the connection dialog to select an available system, inspect discovered systems, or disconnect from the current system.

## SAFE

The **SAFE** control in the top bar toggles the application's lockout state. When active, it is shown as pressed and the bottom status bar reports **Safe**.

![The SAFE control when active.](assets/screenshots/safe-active.png)

Active **SAFE** prevents application-initiated recalls. It does not disable LV1 controls. Use **SAFE** before a rehearsal or other operation where application recall automation must not run.

## Bottom Status Bar

The bottom status bar contains these visible controls and indicators:

| Item | Description |
| --- | --- |
| **GO** | Recalls the cued cue-list entry. It is disabled when no valid cue is available or while a recall is pending. |
| **Cued** | Identifies the scene assigned to the current cue. |
| **Current** | Identifies the current LV1 scene. |
| **Mode** | Reports **Offline**, **Ready**, **Safe**, or **Fading**. |
| **Time** | Displays the local time. |

When no current scene or valid cued scene is available, the corresponding status value is displayed as `---`.

## File Menu And Sessions

The native **File** menu provides session commands. Session files use the `.ascs` extension.

| Command | Operation |
| --- | --- |
| **New Session** | Creates a new session from the current LV1 scene list and clears session cue lists. Shortcut: `CmdOrCtrl+N`. |
| **Open Session...** | Opens a native file picker filtered for `.ascs` session files. Shortcut: `CmdOrCtrl+O`. |
| **Save Session** | Saves to the current session path. If the session has no path, opens the save dialog with `Untitled.ascs` as the suggested name. Shortcut: `CmdOrCtrl+S`. |
| **Save As...** | Opens the native save dialog and saves the session to the selected `.ascs` path. Shortcut: `CmdOrCtrl+Shift+S`. |

The current build does not provide a dirty-session confirmation prompt before creating a new session, opening a session, or closing the application. Save intentional changes before taking those actions.

## Window Title And Unsaved Changes

The window title identifies the active session. It uses the form `Advanced Show Control - Session Name`. The `.ascs` extension is omitted from the displayed session name.

An asterisk marks unsaved changes. For example, a dirty `Tour Prep.ascs` session is displayed as `Advanced Show Control - Tour Prep *`. An unsaved new session is displayed as `Advanced Show Control - Untitled` until its state changes.

## Reconnection States

When the application detects a reconnecting state, it displays a **Reconnecting...** overlay while it retries the connection. The top bar continues to report the connection state, and the footer reports **Offline** whenever no active connection is available.

If reconnection does not complete, the application returns control to the connection workflow so that an engineer can select an available system manually.
