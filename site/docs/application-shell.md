# Application Shell

Use the application shell to connect to the intended LV1 system, select the work area, confirm operating state, and run a prepared cue with **GO**. The top bar controls the application-wide state; the center area changes with the selected tab; the bottom status bar shows the scene and fade state you need before a recall.

![The application shell with the Scenes tab active.](assets/screenshots/application-shell.png)

## Normal Workflow

1. Select the console control in the top bar and connect to the intended LV1 system.
2. Open **Scenes** to configure and store fade targets, or open **Cue Lists** to prepare a recall sequence.
3. Check **Cued**, **Current**, and **Mode** in the bottom status bar before you use **GO**.
4. Enable **SAFE** whenever application-initiated recalls must not run.
5. Save the session after an intentional change.

## Interface Overview

| Area | Purpose |
| --- | --- |
| Top bar | Selects the work area, displays connection state, opens **Connect to LV1**, and toggles **SAFE**. |
| Work area | Shows the selected tab. |
| Bottom status bar | Shows the prepared cue, current LV1 scene, operating mode, and local time. It also contains **GO**. |

Select a top-bar tab to change the work area.

| Tab | Use it to |
| --- | --- |
| **Scenes** | Store targets, set **X-Fade**, and define channel and parameter scope. |
| **Cue Lists** | Build ordered recall sequences and prepare a cue for **GO**. |
| **Events** | View placeholder content. This workflow is not available in the current build. |
| **Logs** | Review operating messages, warnings, and errors. |
| **Settings** | Set application preferences and keyboard shortcuts. |

Sessions are not a tab. Use the native **File** menu to create, open, and save them.

## Connect To LV1

The connection control shows the connected console name when a connection is active. Select it to open **Connect to LV1** while connected or offline. The top bar shows **Connected**, **Connecting**, or **Offline**.

The dialog lists discovered systems. Select an **Available** system to connect. An **Unavailable** system is shown for reference but cannot start a connection. Use **Disconnect** in the dialog before selecting another console.

If the connection drops, the application displays **Reconnecting...** while it retries. Do not recall a scene during this state because no active LV1 connection is available. If reconnection does not complete, use **Connect to LV1** to select an available system, then confirm the current scene before recalling again.

## SAFE

**SAFE** is the application lockout. When it is active, the button appears pressed and **Mode** shows **Safe**.

![The SAFE control when active.](assets/screenshots/safe-active.png)

Enable **SAFE** before a rehearsal, console check, or other operation where Advanced Show Control must not initiate a recall. SAFE blocks recalls started by **Recall** and **GO**, but it does not disable LV1 controls. If a recall is blocked because SAFE is active, leave SAFE enabled until you are ready to operate, then disable it and confirm the intended scene before trying again.

## Bottom Status Bar

The bottom status bar gives you the final check before a cue recall.

| Item | Meaning |
| --- | --- |
| **GO** | Requests recall of the valid cued entry. It is unavailable when no valid cue exists or while a recall is pending. |
| **Cued** | Identifies the scene assigned to the current cue. |
| **Current** | Identifies the current LV1 scene. |
| **Mode** | Shows **Offline**, **Ready**, **Safe**, or **Fading**. |
| **Time** | Shows the local system time. |

`---` means no current scene or valid cued scene is available. **Safe** takes precedence over **Fading** in the display, so confirm the SAFE state before judging fade status.

## Sessions And File Commands

A session is an `.ascs` document containing Advanced Show Control scene configurations and cue lists. It does not replace an LV1 show file.

| Command | Result |
| --- | --- |
| **New Session** | Creates a session from the current LV1 scene list and clears its cue lists. Shortcut: `CmdOrCtrl+N`. |
| **Open Session...** | Opens an `.ascs` session from the native file picker. Shortcut: `CmdOrCtrl+O`. |
| **Save Session** | Saves to the current path. If no path exists, opens the save dialog with `Untitled.ascs` as the suggested name. Shortcut: `CmdOrCtrl+S`. |
| **Save As...** | Saves to a new `.ascs` path through the native save dialog. Shortcut: `CmdOrCtrl+Shift+S`. |

The window title identifies the active session as `Advanced Show Control - Session Name`. The `.ascs` extension is omitted. An asterisk marks an unsaved change, for example `Advanced Show Control - Tour Prep *`.

The current build does not ask you to save before you create a new session, open a session, or close the application. If you leave a dirty session without saving, the changes may be lost. Save the session before you take any of those actions.

## Troubleshooting

- [No LV1 Systems Found](troubleshooting.md#no-lv1-systems-found)
- [Connection Fails Or Reconnects](troubleshooting.md#connection-fails-or-reconnects)
- [Session Cannot Be Opened Or Saved](troubleshooting.md#session-cannot-be-opened-or-saved)
