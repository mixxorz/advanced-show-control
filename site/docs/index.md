# Advanced Show Control

Advanced Show Control is a scene-fade and cue-list utility for Waves eMotion LV1 and LV1 Classic. Use it when you want a recalled LV1 scene to move selected faders or pan controls to stored targets over a defined time, rather than change those controls immediately.

It is intended for engineers who already build and recall scenes in LV1. You continue to create scenes, set routing, processing, mutes, and the normal scene scope in LV1. Advanced Show Control adds a separate fade configuration to selected LV1 scenes. That configuration records the targets you choose, the channels and parameter families it may move, and the **X-Fade** time.

This keeps the normal console workflow intact while giving you a controlled transition for scene changes such as walk-in-to-show, band changes, and program moves. You can also arrange configured scenes in cue lists, prepare the next cue, and recall it with **GO**.

![Advanced Show Control with the Scenes screen open.](assets/screenshots/application-shell.png)

## Choose Your Next Step

[Download for Windows](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_Windows_x64_Setup.zip){ .md-button .md-button--primary }
[Download for macOS](https://github.com/mixxorz/advanced-show-control/releases/download/v2/Advanced-Show-Control_v2_macOS_universal.dmg){ .md-button .md-button--primary }
[Quick Start](getting-started.md){ .md-button }
[User Guide](application-shell.md){ .md-button }

The current stable download is Advanced Show Control v2. The Windows installer and macOS disk image are not signed; the macOS image is also not notarized. Your operating system may ask you to approve the application before opening it. Complete the approval procedure, then rehearse the session on the intended system before show use.

## What It Controls

An app scene configuration is a fade overlay for one LV1 scene. After a valid application recall, Advanced Show Control moves only the stored targets that are both enabled by parameter scope and included in channel scope. A fade always starts from the current live value, so it can move smoothly from the console state you actually have at recall time.

Use **FADER** to include fader targets and **PAN** to include available pan-family targets. Set **X-Fade** to `0` for a cut, or from `0.1` through `120` seconds for a timed transition. If you need a repeatable sequence, place configured scenes in a cue list and use **Cue** followed by **GO**.

## Work Safely

Use **SAFE** when application-initiated recalls must not run. SAFE does not disable LV1 controls, but it blocks recalls started from Advanced Show Control, including **GO**. Before a show, confirm each scene's number, name, scope, targets, and fade duration against LV1.

If you move a fader during an active fade, your adjustment takes precedence for that fader target. If LV1 disconnects, active fade activity stops. Reconnect, confirm the console state, and rehearse the affected transition before using it again.
