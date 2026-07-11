# Scenes

Use **Scenes** to prepare a scene fade setting for an LV1 scene. The setting stores the targets, scope, and **X-Fade** time that Advanced Show Control uses after LV1 recalls the matching scene.

## Prepare A Scene Fade Setting

Scope selects the channels and controls that can change during the fade. If scope is empty when you first select **Store**, every current channel enters scope and **FADER** turns on. The first store can therefore include all current channel faders. Review the channel buttons immediately, remove any channels you do not want to move, then recall the scene.

1. Select the setting in the **Scene library**.
2. Set LV1 to the mix you want to reach.
3. Select **Store**.
4. Select the channels and controls that may move.
5. Set **X-Fade** and save the session.

After later stores, confirm that the selected channels and controls still suit the transition.

## Screen Overview

The **Scene library** lists scene fade settings. The editor shows the selected setting and its recall, storage, duration, and scope controls.

![A selected stored scene with its fade and channel-scope controls.](assets/screenshots/scenes-selected.png)

## Controls

| Control | Result |
| --- | --- |
| **Recall** | Recalls the linked LV1 scene. A fade follows only when LV1 recalls the matching scene. |
| **Store** | Records the current fader and pan values. On the first store with empty scope, it includes every current channel and turns **FADER** on. |
| **Copy** | Is available, but it has no effect in v2. |
| **Paste** | Is disabled in v2. |
| **X-Fade** | Sets the transition time. |

Enter **X-Fade** and press `Enter` or move away from the field. A trailing `s` is accepted. Set `0` for a cut or `0.1` through `120` seconds for a timed transition. Invalid, empty, or negative entries return to the previous value.

![A populated channel-scope grid.](assets/screenshots/channel-scope.png)

| Control | Result |
| --- | --- |
| **FADER** | Allows stored fader values to move on scoped channels. |
| **PAN** | Allows available pan-family values to move on scoped channels. |
| Channel button | Includes or removes that channel. |
| **All** | Includes every available channel. |
| **None** | Removes every channel. |

If no values have been stored, select **Store** before setting scope. If both **FADER** and **PAN** are off, LV1 can still recall the scene, but Advanced Show Control does not move any controls.

## Recall A Scene

1. Confirm LV1 is **Connected** and **SAFE** is off.
2. Select the linked scene fade setting.
3. Compare its LV1 scene number and name with the intended scene.
4. Select **Recall**.

The setting must be linked, and LV1 must recall the same number and name shown in the setting. When those match, scoped controls move from their positions at recall to the stored values. If the setting is unlinked, LV1 is offline, or the name or number differs, no fade starts. Correct the condition, then confirm the scene again.

If you move a fader during a fade, your adjustment takes control of that fader. If LV1 disconnects, the fade stops. Reconnect, confirm the console state, and rehearse before you use the setting again.

## Link Or Relink A Scene

An unlinked scene fade setting shows `---`. It retains its fade time and scope, but **Store** and **Recall** are unavailable.

![An unlinked scene fade setting with LV1 scene selection, link, and delete controls.](assets/screenshots/scenes-unlinked.png)

1. Select the unlinked setting.
2. Select the intended LV1 scene in **LV1 Scene**.
3. Select **Link to scene**.
4. Confirm the displayed LV1 scene number and name.

If **Overwrite Existing Fade Settings?** appears, select **Overwrite** only when you intend to replace that setting. **Delete** removes only the scene fade setting. It does not delete the LV1 scene.

## Duplicate Names

**Duplicate scene names** means more than one scene fade setting has the same name. Use the LV1 scene number with the name before you store, link, or recall. This prevents a familiar name from selecting the wrong scene.

![The duplicate-scene-name warning in the Scene library.](assets/screenshots/scenes-duplicate-warning.png)

## Troubleshooting

- [Scene Is Unlinked](troubleshooting.md#scene-is-unlinked)
- [Recall Is Disabled Or Does Not Fade](troubleshooting.md#recall-is-disabled-or-does-not-fade)
