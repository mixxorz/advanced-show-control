# Scenes

Use **Scenes** to add a controlled fader or pan transition to an LV1 scene. LV1 creates and owns the scene. Advanced Show Control stores the targets, scope, and **X-Fade** time that it may apply after that exact LV1 scene has recalled successfully.

## Prepare A Scene Fade

1. Select the required configuration in the **Scene library**.
2. Set LV1 to the mix you want the transition to reach.
3. Select **Store** to record the current mixer targets.
4. Select the channels and parameter families that may move.
5. Set **X-Fade**.
6. Save the session and rehearse the recall.

Storing targets does not automatically include every channel or parameter. Scope is the permission you give Advanced Show Control to apply particular stored values. Review it after every store, especially when the console state has changed.

## Screen Overview

The **Scene library** at left lists app scene configurations. The editor at right shows the selected configuration, its recall and storage controls, its fade duration, and its channel scope.

![A selected stored scene with its fade and channel-scope controls.](assets/screenshots/scenes-selected.png)

Select a library row before editing it. If nothing is selected, the editor displays **Select a scene to edit its scoped channels.** If no configurations are available, the library displays **No scenes loaded.**

## Scene Library

Each row identifies an app scene configuration rather than every LV1 console setting.

| Item | Meaning |
| --- | --- |
| Scene number | The linked LV1 scene number. `---` means the configuration is unlinked. |
| Scene name | The stored scene name. |
| **X-Fade** | The configured transition time. |
| Direction indicator | Identifies the selected, current, or cued configuration. |

| State | What you see | What it means |
| --- | --- | --- |
| Selected | Orange row highlight and indicator | This configuration is open in the editor. |
| Current | Green text and indicator | Its number and name match the current LV1 scene. |
| Cued | Blue text and indicator | The active cue list points to this configuration. |
| Unlinked | Yellow text and `---` | No available LV1 scene is linked. **Store** and **Recall** are unavailable. |
| Duplicate name | **Duplicate scene names** warning | More than one app configuration uses the same stored name. |

![The duplicate-scene-name warning in the Scene library.](assets/screenshots/scenes-duplicate-warning.png)

## Selected Scene Controls

The selected-scene header shows the stored scene number and name.

| Control | Use it to | Result |
| --- | --- | --- |
| **Recall** | Request the selected linked scene. | LV1 recalls the scene; a scoped fade starts only after validation succeeds. |
| **Store** | Capture the current live mixer targets. | Updates the targets available to the selected configuration. |
| **Copy** | No current workflow. | Unavailable in the current build. |
| **Paste** | No current workflow. | Disabled in the current build. |
| **X-Fade** | Set transition time. | Applies to the selected configuration, including an unlinked configuration. |

### Set Fade Duration

Enter a value in **X-Fade**, then press `Enter` or move focus away. A trailing `s` is accepted. The arrow controls change the value by one second.

Set `0` for a cut. Set a timed transition from `0.1` through `120` seconds. Empty, negative, or invalid entries return to the stored value, so verify the displayed duration before you recall.

### Set Parameter And Channel Scope

Parameter scope defines which stored parameter families may move. Channel scope defines which channels may use those families.

![A populated channel-scope grid.](assets/screenshots/channel-scope.png)

| Control | When active |
| --- | --- |
| **FADER** | Stored fader targets can move on scoped channels. |
| **PAN** | Available pan-family targets can move on scoped channels. |
| Channel button | The channel is included in the selected configuration's scope. Hover text shows its stored values. |
| **All** | Includes every available channel. |
| **None** | Removes every channel from scope. |

The grid groups channels by console section. If no stored channel data exists, it displays **Store the current mixer state to choose scoped channels.** When both **FADER** and **PAN** are off, no scoped fade targets are available, so recall does not start a fade.

## Recall A Scene

1. Confirm that LV1 is connected and **SAFE** is not active.
2. Select the required linked configuration.
3. Compare its displayed scene number and name with the intended LV1 scene.
4. Select **Recall**.
5. Watch **Mode** in the bottom status bar while the transition runs.

The application validates the recall before it starts a fade. The LV1 scene number and name after recall must exactly match the linked configuration. LV1 must be connected, and current live channel data must be available. When validation succeeds, each scoped target moves from its current live value to its stored value.

If SAFE is active, the configuration is unlinked, LV1 is offline, or scene identity does not match, the fade will not start. Correct the reported condition, verify the intended scene, and recall again. A blocked, skipped, or disabled recall does not stop a fade that is already running.

If you move a fader during a fade, your manual adjustment takes control of that target. The remaining eligible targets can continue, but the adjusted fader no longer follows the stored target. If LV1 disconnects, active fade activity stops. Reconnect, confirm the console state, and rehearse before you use the transition again.

## Link Or Relink A Scene

An unlinked configuration retains its duration and scope but cannot store targets or recall. Use linking when the LV1 scene list has changed or when you need to assign a configuration to an available LV1 scene.

![An unlinked configuration with LV1 scene selection, link, and delete controls.](assets/screenshots/scenes-unlinked.png)

1. Select the unlinked configuration.
2. Confirm that its stored targets, duration, and scope suit the intended LV1 scene.
3. Select the LV1 scene in **LV1 Scene**.
4. Select **Link to scene**.
5. Confirm the displayed number and name before you recall.

The selector initially chooses the first LV1 scene without an app configuration when one is available. Exact matching uses both number and name. A matching name by itself is not enough to identify a safe replacement.

If you select an LV1 scene that already has an app configuration, the application shows **Overwrite Existing Fade Settings?**. Select **Overwrite** only when replacing that configuration is intentional. Otherwise, cancel and choose another scene.

To remove an unlinked configuration that you no longer need, select it, review its name and scope, then select **Delete**. This removes only the Advanced Show Control configuration. It does not delete or change the LV1 scene.

## Work With Duplicate Names

Duplicate names can describe different scenes, so use the scene number together with the name when you identify a configuration. If **Duplicate scene names** appears, compare both values before you store, relink, or recall. This prevents a familiar name from being mistaken for the intended LV1 scene.

## Troubleshooting

- [Scene Is Unlinked](troubleshooting.md#scene-is-unlinked)
- [Duplicate Scene Name Warning](troubleshooting.md#duplicate-scene-name-warning)
- [Recall Is Disabled Or Blocked](troubleshooting.md#recall-is-disabled-or-blocked)
- [Fade Does Not Start](troubleshooting.md#fade-does-not-start)
