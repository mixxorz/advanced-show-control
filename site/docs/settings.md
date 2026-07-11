# Settings

Use **Settings** to set application preferences, choose the displayed time format, control diagnostic detail, and assign the CUE and GO shortcuts. Changes are stored when you make them. If a setting cannot be saved, the screen shows the error and restores the last confirmed values.

![The Settings screen.](assets/screenshots/settings.png)

## Set A Preference

1. Open **Settings**.
2. Select or adjust the required control.
3. Confirm that the control still shows the value you chose.
4. Read the notes below before depending on a stored-only preference during a show.

Settings do not override recall safety checks. A shortcut cannot make an unavailable cue valid or bypass **SAFE**, LV1 connection requirements, or exact scene identity validation.

## General Settings

| Setting | Use it to | Current result |
| --- | --- | --- |
| **Auto load last show file** | Choose whether the application should open the last session at startup. | Stored, default off. It does not currently open a session at startup. |
| **Auto save sessions** | Choose whether session changes should save automatically. | Stored, default off. It does not currently save after changes; use **File > Save Session**. |
| **Time display** | Choose **12 hour** or **24 hour** time. | Stored, default **24 hour**. It does not currently change the clock in the application. |
| **Fader override sensitivity** | Choose the intended manual-override sensitivity from `1` through `10`. | Stored, default `9`. The current fade engine does not use this setting. |
| **Extensive diagnostics** | Include `DEBUG` events in diagnostic files. | Active. When off, diagnostic files retain `INFO`, `WARN`, and `ERROR`; when on, they also retain `DEBUG`. |

The Fader override sensitivity help describes the intended scale: `10` should react to very small movements, while `1` should require a larger movement. The current manual-override behavior does not change when you adjust this stored setting. If a live fade must yield to a manual fader move, rehearse the existing behavior rather than relying on this control.

Enable **Extensive diagnostics** only while you investigate a problem. `DEBUG` entries can make diagnostic files grow quickly. Disable it after you collect the information you need.

## Keyboard Shortcuts

**GO** recalls the current valid cue. **CUE** prepares the selected cue-list entry. Their default shortcuts are `Space` for GO and `C` for CUE.

1. Select the GO or CUE shortcut control. It displays `...` while it waits for a key.
2. Press the required key combination.
3. Confirm that the control displays the new combination.

Capture records one non-modifier key with any held Shift, Control, Alt, or Meta modifier. Press `Escape` to cancel. Pressing a modifier by itself keeps capture active. `Tab` is a valid shortcut and does not move focus while capture is active.

The application compares letter keys without regard to case. It rejects a shortcut already assigned to the other action or to a fixed file command. The fixed commands are **New Session** (`CmdOrCtrl+N`), **Open Session...** (`CmdOrCtrl+O`), **Save Session** (`CmdOrCtrl+S`), and **Save As...** (`CmdOrCtrl+Shift+S`).

Action shortcuts do not run while focus is in a text input, text area, select control, editable content, or dialog. Repeated keydown events do not create repeated CUE or GO requests.

## Troubleshooting

If a preference is stored but does not change application behavior, see [A Setting Does Not Change Application Behavior](troubleshooting.md#a-setting-does-not-change-application-behavior). For the full shortcut reference, see [Keyboard Shortcuts](reference/keyboard-shortcuts.md).
