# Application Shell

Use the application shell to connect to LV1, prepare scenes or cues, and check the state before a recall. The top bar controls connection and **SAFE**. The centre area shows the selected tab. The bottom bar shows **Cued**, **Current**, and **Mode**.

![The application shell with the Scenes tab active.](assets/screenshots/application-shell.png)

## Status Before Recall

| Item | Meaning |
| --- | --- |
| **Cued** | The scene prepared for **GO**. |
| **Current** | The LV1 scene that is active now. |
| **Mode** | **Offline** means no console is connected; **Ready** means no active fade status is shown, so **Recall** and **GO** can be available when their other conditions pass; **Safe** means SAFE blocks app recalls; **Fading** means a timed transition is moving controls. |
| **GO** | Recalls the cued scene fade setting. |

`---` means the corresponding scene is unavailable. Compare **Cued** and **Current** before a recall.

## Normal Workflow

1. Connect to the intended LV1 system.
2. Prepare a scene fade setting in **Scenes** or a sequence in **Cue Lists**.
3. Compare **Cued** and **Current** before **GO**.
4. Save the session after an intended change.

## Tabs

| Tab | Use it to |
| --- | --- |
| **Scenes** | Store values, set **X-Fade**, and select scope. |
| **Cue Lists** | Build a prepared recall sequence. |
| **Events** | View the Events tab. You cannot create or run event automation in v2. |
| **Logs** | Review messages, warnings, and errors. |
| **Settings** | Change active v2 settings and view inactive controls. |

## Connect To LV1

Select the console control to open **Connect to LV1**. Select an **Available** console and confirm **Connected** in the top bar. An **Unavailable** console cannot be selected.

**Reconnecting...** is a full-screen overlay. Do not start a recall while it is visible because the connection is not ready. Wait for the top bar to show **Connected**. If reconnection fails, open **Connect to LV1**, select the intended available console, and confirm **Current** before recall.

## SAFE And Status

When **SAFE** is on, **Mode** displays **Safe** instead of **Fading**. You cannot use Mode to see a running fade in that state, so wait for a fade to finish before you enable SAFE when you need that confirmation.

![The SAFE control when active.](assets/screenshots/safe-active.png)

**SAFE** blocks **Recall** and **GO** but does not disable LV1 controls or stop a fade already running. Use it before a rehearsal or console check where app recalls must not run.

If you move one fader during a fade, **Mode** can return to **Ready** while other scoped controls continue moving. Watch those controls before you treat the transition as finished.

## Sessions

Sessions are `.ascs` files that store scene fade settings and cue lists.

| Command | Shortcut |
| --- | --- |
| **New Session** | `CmdOrCtrl+N` |
| **Open Session...** | `CmdOrCtrl+O` |
| **Save Session** | `CmdOrCtrl+S` |
| **Save As...** | `CmdOrCtrl+Shift+S` |

An asterisk in the window title marks unsaved changes. v2 does not ask before you create, open, or close a session with unsaved changes. Save before you take those actions.
