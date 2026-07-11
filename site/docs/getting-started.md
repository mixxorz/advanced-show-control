# Getting Started

## Requirements

Advanced Show Control requires a Waves eMotion LV1 or LV1 Classic system that is reachable from the computer running the application. LV1 remains responsible for scene creation, scene recall, routing, processing, mutes, and live console state. Advanced Show Control stores application-managed fade settings for linked LV1 scenes and moves only parameters placed in scope.

Release artifacts currently provide a Windows x64 installer package and a universal macOS disk image. The artifacts are unsigned, and the macOS artifact is not notarized. Your operating system may require explicit approval before it opens the application. Rehearse the complete session workflow before using the application in a show.

## Installation

1. Download the current release artifact for the required operating system from the project's GitHub Releases page.
2. On Windows, extract the release archive and run the included installer.
3. On macOS, open the disk image and install the application from it.
4. If the operating system blocks the application because the artifact is unsigned or not notarized, use the operating system's explicit approval procedure before continuing.

## First Launch

On startup, the **Connect to LV1** dialog opens and starts searching for systems. The dialog may remain open while discovery continues. Select a system to connect, or close the dialog to continue with the application offline. The footer reports **Offline** while no LV1 connection is active.

## Connect To LV1

The connection dialog lists discovered systems with their host name, address, port, availability, and current TCP latency. Discovery refreshes while the dialog is open.

![The Connect to LV1 dialog lists available and unavailable systems.](assets/screenshots/connection-systems-found.png)

1. Wait for the required LV1 system to appear as **Available**.
2. Select the available system row.
3. Confirm that the top bar reports **Connected** and displays the console name.

Systems marked **Unavailable** cannot be selected. To open the dialog after startup, select the console control in the top bar. This control remains available while connected, so it can be used to inspect systems or select another console.

## Connection States

The top bar reports one of these connection states:

| State | Meaning |
| --- | --- |
| **Connected** | The application has an active LV1 connection. |
| **Connecting** | A connection attempt is in progress. |
| **Offline** | No LV1 connection is active. |

The connection dialog identifies each discovered system as **Available**, **Unavailable**, or **Connected**. An unavailable system is shown for information only and cannot start a connection.

## Create A Session

Create a session after connecting to the intended LV1 system.

1. Choose **File > New Session**.
2. The application creates a new session from the current LV1 scene list and clears the session cue lists.
3. Use **File > Save Session** to choose a name and location for the new `.ascs` session file.

Use **File > Open Session...** to load an existing `.ascs` session file.

## Configure A First Fade

1. Open the **Scenes** tab and select a scene in the scene library.
2. In the selected-scene area, use **Scope** to enable faders and, when required, pan. Use **All**, **None**, or the individual channel controls to define the channels included in the fade.
3. Set **X-Fade** to the required duration in seconds. The control accepts values from 0 through 120 seconds.
4. Set the required live LV1 values, then select **Store** to store the scene configuration.
5. Save the session.

Only scoped parameters are managed by the application. Confirm the scope and stored values during rehearsal before recalling the scene in production.

## Current Limitations

- Advanced Show Control is an LV1 fade overlay; it does not create or own LV1 scenes.
- The application stores session data in `.ascs` files. It does not replace LV1 show-file management.
- The **Events** tab is currently a placeholder.
- Release artifacts are unsigned, and the macOS artifact is not notarized.
