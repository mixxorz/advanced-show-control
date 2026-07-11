# Getting Started

Use this procedure to install Advanced Show Control, connect to LV1, save a session, and hear one configured scene fade. You need a Waves eMotion LV1 or LV1 Classic system that the computer can reach on the network.

LV1 remains responsible for creating scenes, recalling normal console state, routing, processing, mutes, and live mixer state. Advanced Show Control stores the fade settings that it adds to linked LV1 scenes. Rehearse the complete workflow on the intended system before using it in a show.

## Download And Install

1. Download [Advanced Show Control v2 for Windows](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_Windows_x64_Setup.zip) or [Advanced Show Control v2 for macOS](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_macOS_universal.dmg).
2. On Windows, extract the ZIP archive and run the included installer.
3. On macOS, open the disk image and install the application from it.
4. Approve the application through your operating system if it is blocked. The v2 Windows installer and macOS disk image are not signed, and the macOS image is not notarized.

## Connect To LV1

On first launch, **Connect to LV1** opens and searches for consoles. The dialog can remain open while discovery continues.

![The Connect to LV1 dialog lists available and unavailable systems.](assets/screenshots/connection-systems-found.png)

1. Wait for the intended console to appear as **Available**.
2. Select its row.
3. Confirm that the top bar reports **Connected** and shows the console name.

The dialog shows each console's host name, network address, port, availability, and TCP latency. An **Unavailable** console cannot be selected. If you close the dialog or need to change consoles later, select the console control in the top bar to open it again.

If no connection is active, the top bar shows **Offline** and the bottom status bar shows **Offline**. Do not attempt a scene recall until the intended console is connected.

## Create And Save A Session

A session is the Advanced Show Control document that stores scene fade configurations and cue lists. It does not replace the LV1 show file.

1. Connect to the intended LV1 system.
2. Choose **File > New Session**. The application reads the current LV1 scene list and clears any cue lists from the new session.
3. Choose **File > Save Session**.
4. Enter a name, choose a location, and save the `.ascs` file.

Save again after you change scene settings. The application does not currently ask you to save a dirty session before creating a new session, opening another session, or closing the application.

## Configure One Fade

Create and save the LV1 scene you want to use before you configure its fade. Then prepare the Advanced Show Control overlay.

1. Open the **Scenes** tab and select the required scene in the **Scene library**.
2. Set LV1 to the target mix you want that scene to reach.
3. Select **Store**. This records the current mixer targets and makes channel-scope controls available.
4. Select the required channels in the scope grid. Use **All** to include every available channel or **None** to clear the selection.
5. Enable **FADER**. Enable **PAN** only when you also want available pan-family values to move.
6. Set **X-Fade** to a timed value from `0.1` through `120` seconds. Enter `0` when you want the scoped targets to cut instead.
7. Choose **File > Save Session**.

Only the scoped parameter families on scoped channels can move. If you store new targets later, review channel scope and parameter scope again before recall.

## Recall And Observe The Result

1. Confirm that **SAFE** is not active.
2. Check the displayed scene number and name against the intended LV1 scene.
3. Select **Recall**.
4. Watch the bottom status bar. During a timed fade, **Mode** shows **Fading**. When the fade completes, it returns to **Ready**.

LV1 recalls the scene first. Advanced Show Control then checks that the recalled LV1 scene number and name exactly match the linked configuration, that LV1 is connected, and that current live channel data is available. When those checks pass, the scoped parameters move from their current live values to the targets you stored.

If **SAFE** is active, LV1 is offline, the scene is unlinked, or the recalled scene identity does not match, the fade will not start. Correct the indicated condition, confirm the intended scene again, and retry. A blocked or skipped recall does not stop a fade that is already running.

## Next Steps

- Read [Application Shell](application-shell.md) to orient yourself around the main controls.
- Read [Scenes](scenes.md) before preparing additional fade configurations.
- Read [Cue Lists](cue-lists.md) when you need a prepared recall sequence and **GO** operation.
