# Getting started

This guide takes you from installation to your first scene fade. Before you begin, connect the computer running Advanced Show Control to the same network as your Waves eMotion LV1 or LV1 Classic system.

## 1. Download and install

Download the appropriate archive from the [latest Advanced Show Control release](https://github.com/mixxorz/advanced-show-control/releases/latest). The native app requires macOS 15 or newer or Windows 10 or newer.

On Windows, extract the x64 ZIP archive and run **Advanced Show Control.exe**. On macOS, extract the universal ZIP archive, move **Advanced Show Control.app** to Applications if desired, and open it.

The Windows download is unsigned. The macOS app is ad-hoc signed, not Developer ID signed or notarized. If your computer blocks the app, approve it in your operating-system security settings and open it again.

## 2. Connect to LV1

Advanced Show Control searches for LV1 systems when it opens.

![The Connect to LV1 dialog lists available and unavailable systems.](assets/screenshots/connection-systems-found.png)

1. Wait for your console to appear as **Available**.
2. Select the console.
3. Confirm that the top bar shows **Connected** and the correct console name.

If the console appears as **Unavailable**, check the network connection and wait for discovery to update. You can reopen this window at any time by selecting the console name in the top bar.

## 3. Create a session

A session stores your scene fades and cue lists in an `.ascs` file.

1. Choose **File > New Session**.
2. Choose **File > Save Session**.
3. Name the session and save it.

Advanced Show Control does not yet warn you before closing or replacing a session with unsaved changes. Save after each change you want to keep.

## 4. Create your first fade

Create the scene in LV1 first, then use Advanced Show Control to add the fade.

1. Open **Scenes** and select the scene.
2. Set the faders and pans in LV1 to the values you want the scene to reach.
3. In **Scope**, select only the channels you want Advanced Show Control to move.
4. Turn on **FADER**. Turn on **PAN** if pan controls should move as well.
5. Select **Store**.
6. Set **X-Fade** to the transition time you want.
7. Save the session.

!!! warning "Check scope before recall"
    A new scene fade has no selected channels, and **FADER** and **PAN** are off. Select the intended channels and enable the controls before you select **Store**. If no channels are in scope when you select **Store**, all current channels are added. **Store** keeps the current **FADER** and **PAN** selections.

Set **X-Fade** to `0` for an immediate cut. For a timed transition, use a value from `0.1` to `120` seconds.

## 5. Recall the scene

1. Confirm that the top bar shows **Connected** and **SAFE** is off.
2. Check the scene number and name.
3. Select **Recall**.
4. Watch the scoped controls move to the stored values.

The fade begins at the current fader and pan positions, not at the values that were present when you stored the scene. This allows the transition to begin smoothly from the mix that is live at recall time.

During a normal fade, **Mode** shows **Fading** and returns to **Ready** afterward. If you move a fader during the fade, that fader follows your movement while the other scoped controls may continue. Watch the controls themselves before you consider the transition complete.

If the fade does not start, confirm that LV1 is connected, **SAFE** is off, and the scene number and name match. See [Recall Is Disabled Or Does Not Fade](troubleshooting.md#recall-is-disabled-or-does-not-fade) for additional checks.

## Continue learning

- [Scenes](scenes.md) explains every part of a scene fade.
- [Cue Lists](cue-lists.md) shows how to arrange scenes in show order.
- [Application Shell](application-shell.md) explains **SAFE**, **GO**, and the status bar.
