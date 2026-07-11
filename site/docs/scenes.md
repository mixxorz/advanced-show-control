# Scenes

The **Scenes** screen configures the application's fade overlay for LV1 scenes. LV1 remains responsible for scene creation and normal scene recall. This screen stores target values and selects the parameters that the application may move after a valid recall.

## Screen Overview

The screen has two work areas. The **Scene library** at left lists application-managed scene configurations. The selected scene at right provides recall, storage, duration, and scope controls.

![A selected stored scene with its fade and channel-scope controls.](assets/screenshots/scenes-selected.png)

Select a row in the library before editing it. The selected row is highlighted. If no configuration is selected, the editor reports: **Select a scene to edit its scoped channels.**

## Scene Library

The library lists the application's scene configurations, not every console setting. Each row contains the scene number, scene name, and **X-Fade** duration.

| Library item | Meaning |
| --- | --- |
| Scene number | The linked LV1 scene number. `---` means that the configuration is unlinked. |
| Scene name | The name stored with the configuration. |
| X-Fade | The configured fade duration. |
| Direction indicator | Identifies the selected, current, or cued scene. |

Select any row to make it the selected scene. The library reports **No scenes loaded.** when no application-managed configurations are available.

## Scene States

The library and selected-scene header use color and indicators to show the state of a configuration.

| State | Visible indication | Operational meaning |
| --- | --- | --- |
| Selected | Orange row highlight and direction indicator | This configuration is shown in the editor. |
| Current | Green direction indicator and text | The linked scene number and name match the current LV1 scene. |
| Cued | Blue direction indicator and text | The active cue list points to this configuration. |
| Unlinked | Yellow text; scene number is `---` | The stored configuration no longer has an LV1 scene link. Store and Recall are unavailable. |
| Duplicate name | Yellow **Duplicate scene names** warning above the library | More than one configuration has the same stored scene name. |

![The duplicate-scene-name warning in the Scene library.](assets/screenshots/scenes-duplicate-warning.png)

## Selected Scene

The selected-scene header displays the stored scene number and name. Below it are the action controls and the **X-Fade** input.

| Control | Operation | Availability |
| --- | --- | --- |
| **Recall** | Requests recall of the selected linked scene and, when validated, starts its scoped fade. | Disabled for an unlinked configuration. |
| **Store** | Stores current mixer targets for the selected linked configuration. | Disabled for an unlinked configuration. |
| **Copy** | Reserved for a future workflow. | Unavailable in the current build. |
| **Paste** | Reserved for a future workflow. | Disabled in the current build. |
| **X-Fade** | Sets the selected configuration's fade duration. | Available for linked and unlinked configurations. |

## Fade Duration

Enter a duration in the **X-Fade** field, then press `Enter` or move focus away from the field. A trailing `s` is accepted. The arrow controls increase or decrease the duration by one second.

Duration `0` is a cut. A nonzero duration is constrained to the UI range of `0.1` through `120` seconds. Invalid, empty, or negative entries are restored to the stored value.

## Parameter Scope

Parameter scope determines which stored values may be applied for scoped channels.

| Control | When active |
| --- | --- |
| **FADER** | Applies stored fader targets for scoped channels. |
| **PAN** | Applies stored pan-family targets when that parameter is available for the channel. |

Select either control to toggle it. When both controls are inactive, the configuration has no applicable fade targets; recall does not start a scoped fade.

## Channel Scope

Channel scope determines which channels participate in the selected scene's fade. The grid groups channels by console section and presents a button for each available channel. An active channel button is in scope.

![A populated channel-scope grid.](assets/screenshots/channel-scope.png)

| Control | Operation |
| --- | --- |
| Channel button | Adds or removes that channel from the selected scene's scope. Hover text identifies the channel and its stored values. |
| **All** | Adds every available channel to the selected scene's scope. |
| **None** | Removes every channel from the selected scene's scope. |
| **FADER** / **PAN** | Chooses the parameter families applied to all scoped channels. |

If the configuration has no stored channel data, the editor reports: **Store the current mixer state to choose scoped channels.**

## Selected Scene Actions

Use the selected-scene controls in this order when preparing a configuration:

1. Select the scene configuration in the library.
2. Select **Store** to capture current mixer targets.
3. Select the required channels and parameter scope.
4. Set **X-Fade**.
5. Use **Recall** only after confirming that the selected configuration identifies the intended LV1 scene.

## Link A Scene

An unlinked configuration retains its duration and scope settings but cannot be stored or recalled. Select the unlinked row to display the linking controls.

![An unlinked configuration with LV1 scene selection, link, and delete controls.](assets/screenshots/scenes-unlinked.png)

1. In **LV1 Scene**, select the intended LV1 scene.
2. Select **Link to scene**.
3. Confirm that the library now displays the LV1 scene number instead of `---`.

The selector initially chooses the first LV1 scene without an existing application configuration when one is available.

## Store Targets

Select a linked scene configuration, establish the required live mixer state, and select **Store**. Storing provides the channel data used by the channel-scope grid and updates the target values used by subsequent scoped fades. Review the selected channels and parameter scope after storing, particularly when the console state has changed since the last store.

## Recall A Scene

1. Confirm that **SAFE** is not active.
2. Select the intended linked configuration in the Scene library.
3. Verify its displayed scene number and name against the intended LV1 scene.
4. Select **Recall**.

The application validates the recall before starting a fade. The stored scene number and name must exactly match the current LV1 scene identity after recall. LV1 must be connected and current live channel data must be available. When validation succeeds, each scoped parameter fades from its current live value to its stored target; it does not begin from the value captured when the target was stored.

**SAFE** blocks application-initiated recalls. A blocked or skipped recall does not start a new fade and does not stop an existing fade. If the application reports a block, correct the indicated condition before attempting the recall again.

During a fade, a manual fader adjustment is treated as an override. The application cancels control of that fader target so that the engineer's live adjustment takes precedence. A disconnect cancels active fade activity; do not expect pending targets to continue while LV1 is offline.

## Relink A Missing Scene

An LV1 scene can be unavailable after the console scene list changes. Its application configuration remains in the library as **Unlinked**.

1. Select the unlinked configuration.
2. Confirm that its stored targets, duration, and channel scope are still appropriate for the replacement LV1 scene.
3. Select the replacement in **LV1 Scene**.
4. Select **Link to scene**.
5. Reconfirm the displayed scene number and name before recalling it.

Exact scene matching uses both the scene number and name. Do not rely on a matching name alone when relinking.

## Delete An Unlinked Configuration

To remove a configuration that no longer has a useful LV1 counterpart:

1. Select the unlinked configuration.
2. Review its name and stored scope.
3. Select **Delete** in the unlinked-scene panel.

Delete removes the application configuration. It does not delete or modify an LV1 scene.

## Duplicate Scene Names

The library displays **Duplicate scene names** when two or more application configurations have the same stored name. The warning lists the duplicated names.

Use the scene number together with the name to identify the intended configuration. If you link an unlinked configuration to an LV1 scene that already has a configuration, the application displays **Overwrite Existing Fade Settings?** Select **Overwrite** only when replacing the existing configuration is intentional; otherwise, cancel and choose a different LV1 scene.

## Empty And Disconnected States

When there is no selected configuration, select a Scene library row to open its editor. When no configurations are loaded, the library displays **No scenes loaded.**

When LV1 is disconnected, recalls are blocked and no fade is started. Reconnect to LV1, confirm the current scene and scene list, then verify each affected configuration before recall. Unlinked configurations remain editable for duration and scope while their **Store** and **Recall** controls remain disabled.

## Troubleshooting

- [Scene Is Unlinked](troubleshooting.md#scene-is-unlinked)
- [Duplicate Scene Name Warning](troubleshooting.md#duplicate-scene-name-warning)
- [Recall Is Disabled Or Blocked](troubleshooting.md#recall-is-disabled-or-blocked)
- [Fade Does Not Start](troubleshooting.md#fade-does-not-start)
