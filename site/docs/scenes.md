# Scenes

Use **Scenes** to add a fade to an LV1 scene. For each scene, you can store target values, choose the channels and controls that may move, and set the transition time.

![A selected scene with its fade and scope controls.](assets/screenshots/scenes-selected.png)

## Create A Scene Fade

1. Select the scene in the **Scene library**.
2. Set LV1 to the mix you want the scene to reach.
3. Select **Store** to capture the current fader and pan values.
4. Choose the channels that may move.
5. Turn **FADER** and **PAN** on or off as needed.
6. Set **X-Fade** and save the session.

Scope is the combination of selected channels and enabled controls. Advanced Show Control moves only the faders and pans in scope.

!!! warning "Store with an empty scope"
    If no channels are in scope when you select **Store**, all current channels are added. Remove any channels that should not move before recall. **Store** preserves the current **FADER** and **PAN** selections.

## Scene Library

The library shows the LV1 scene number, scene name, and **X-Fade** time for each scene fade. Select a row to edit it.

The row indicators identify the selected, current, and cued scenes. An unlinked scene shows `---` instead of an LV1 scene number.

**Duplicate scene names** means more than one scene fade uses the same name. Check the LV1 scene number as well as the name before storing or recalling.

![The duplicate-scene-name warning in the Scene library.](assets/screenshots/scenes-duplicate-warning.png)

## Store And Scope

**Store** captures the current fader and pan values as targets. Storing again replaces those targets with the current LV1 values.

![A populated scope grid.](assets/screenshots/channel-scope.png)

| Control | What it does |
| --- | --- |
| **FADER** | Includes fader targets for the selected channels. |
| **PAN** | Includes available pan-family targets for the selected channels. |
| Channel button | Adds or removes one channel from scope. |
| **All** | Adds every available channel. |
| **None** | Removes every channel. |

If both **FADER** and **PAN** are off, LV1 can still recall the scene, but Advanced Show Control will not move any controls.

## Fade Time

Set **X-Fade** to `0` for an immediate cut or from `0.1` to `120` seconds for a timed transition. Press `Enter` or move away from the field to apply the value. You can include a trailing `s`.

Invalid, empty, or negative values return to the previous setting.

## Recall A Scene

1. Confirm that LV1 is **Connected** and **SAFE** is off.
2. Select the scene fade.
3. Check its LV1 scene number and name.
4. Select **Recall**.

LV1 recalls the scene first. When the recalled number and name match, Advanced Show Control moves the controls in scope from their current positions to the stored targets.

If the number or name does not match, the fade does not start. Correct the scene link or select the intended scene, then try again.

Moving a fader during a fade gives you control of that fader; the other scoped controls may continue moving. If LV1 disconnects, the fade stops. Reconnect and confirm the console state before recalling again.

## Link A Missing Scene

When an LV1 scene can no longer be found, its scene fade becomes unlinked. Its fade time and scope are retained, but **Store** and **Recall** are unavailable.

![An unlinked scene fade with linking controls.](assets/screenshots/scenes-unlinked.png)

1. Select the unlinked scene fade.
2. Choose the intended scene from **LV1 Scene**.
3. Select **Link to scene**.
4. Confirm the scene number and name.

If **Overwrite Existing Fade Settings?** appears, continue only when you intend to replace the fade already linked to that LV1 scene.

**Delete** removes the scene fade from Advanced Show Control. It does not delete the scene in LV1.

## Unavailable Controls

In version 2, **Copy** does not copy or change a scene fade, and **Paste** is disabled.

## Troubleshooting

- [Scene Is Unlinked](troubleshooting.md#scene-is-unlinked)
- [Recall Is Disabled Or Does Not Fade](troubleshooting.md#recall-is-disabled-or-does-not-fade)
