# Getting Started

This guide takes you from installation to your first scene fade. Before you begin, connect the computer running Advanced Show Control to the same network as your Waves eMotion LV1 or LV1 Classic system.

## 1. Download And Install

Download [Advanced Show Control v2 for Windows](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_Windows_x64_Setup.zip) or [Advanced Show Control v2 for macOS](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_macOS_universal.dmg).

On Windows, extract the ZIP file and run the installer. On macOS, open the disk image and install the app.

The downloads are not signed, and the macOS version is not notarized. If your computer blocks the app, approve it in your operating-system security settings and open it again.

## 2. Connect To LV1

Advanced Show Control searches for LV1 systems when it opens.

![The Connect to LV1 dialog lists available and unavailable systems.](assets/screenshots/connection-systems-found.png)

1. Wait for your console to appear as **Available**.
2. Select the console.
3. Confirm that the top bar shows **Connected** and the correct console name.

If the console appears as **Unavailable**, check the network connection and wait for discovery to update. You can reopen this window at any time by selecting the console name in the top bar.

## 3. Create A Session

A session stores your scene fades and cue lists in an `.ascs` file.

1. Choose **File > New Session**.
2. Choose **File > Save Session**.
3. Name the session and save it.

Advanced Show Control does not yet warn you before closing or replacing a session with unsaved changes. Save after each change you want to keep.

## 4. Create Your First Fade

Create the scene in LV1 first, then use Advanced Show Control to add the fade.

1. Open **Scenes** and select the scene.
2. Set the faders and pans in LV1 to the values you want the scene to reach.
3. Select **Store**.
4. In **Scope**, keep only the channels you want Advanced Show Control to move.
5. Leave **FADER** on. Turn on **PAN** if pan controls should move as well.
6. Set **X-Fade** to the transition time you want.
7. Save the session.

!!! warning "Check scope before recall"
    If no channels are in scope when you select **Store**, all current channels are added. Remove any channels that should not move. **Store** keeps the current **FADER** and **PAN** selections, so check those as well.

Set **X-Fade** to `0` for an immediate cut. For a timed transition, use a value from `0.1` to `120` seconds.

## 5. Recall The Scene

1. Confirm that the top bar shows **Connected** and **SAFE** is off.
2. Check the scene number and name.
3. Select **Recall**.
4. Watch the scoped controls move to the stored values.

The fade begins at the current fader and pan positions, not at the values that were present when you stored the scene. This allows the transition to begin smoothly from the mix that is live at recall time.

During a normal fade, **Mode** shows **Fading** and returns to **Ready** afterward. If you move a fader during the fade, that fader follows your movement while the other scoped controls may continue. Watch the controls themselves before you consider the transition complete.

If the fade does not start, confirm that LV1 is connected, **SAFE** is off, and the scene number and name match. See [Recall Is Disabled Or Does Not Fade](troubleshooting.md#recall-is-disabled-or-does-not-fade) for additional checks.

## Continue Learning

- [Scenes](scenes.md) explains every part of a scene fade.
- [Cue Lists](cue-lists.md) shows how to arrange scenes in show order.
- [Application Shell](application-shell.md) explains **SAFE**, **GO**, and the status bar.
