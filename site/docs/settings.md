# Settings

Use **Settings** to change same-scene recall behavior, shortcuts, and diagnostic detail available in v2. If a setting cannot be saved, the screen restores the last confirmed value and shows an error.

![The Settings screen.](assets/screenshots/settings.png)

## Active in v2

### Same-scene recall

**Same scene recall finishing** is on by default. When it is on, recalling an app-managed scene again while that exact scene still owns active fade targets completes those targets after LV1's post-recall readiness check. When it is off, matching fades continue from their current values over the full configured scene duration instead of completing immediately.

**Same scene recall threshold** defaults to `500 ms` and accepts values from `0 ms` through `5000 ms` in `100 ms` steps. Repeated identical LV1 scene notifications below the threshold are ignored. A notification at the threshold is eligible for normal recall validation. The threshold does not change connection arming, scene-list-edit suppression, exact scene matching, SAFE, or disconnect behavior.

Use these settings with **Extensive diagnostics** when investigating unexpected immediate fade completion. If disabling finishing removes the jump, same-scene finishing was selected. Increase the threshold to test whether delayed duplicate LV1 scene notifications are being accepted. Rehearse any changed value before show use.

### Extensive diagnostics

Enable **Extensive diagnostics** only while you investigate a problem. When it is off, diagnostic files include `INFO`, `WARN`, and `ERROR`. When it is on, they also include `DEBUG`, so files can grow quickly. Disable it after you collect the information you need.

### Keyboard shortcuts

**GO** recalls the current cued entry. **CUE** prepares the selected cue-list entry. The defaults are `Space` for GO and `C` for CUE.

1. Select the GO or CUE shortcut control. It displays `...`.
2. Press the key combination you want.
3. Confirm the displayed shortcut.

Press `Escape` to cancel. Hold Shift, Control, Alt, or Meta with the key when needed. Pressing a modifier by itself keeps capture open. `Tab` can be used as a shortcut. A shortcut cannot duplicate the other action or **New Session**, **Open Session...**, **Save Session**, or **Save As...**.

Shortcuts do not operate while you are entering text or working in a dialog. Holding a shortcut does not repeat CUE or GO. A shortcut does not bypass **SAFE**, connection checks, or the LV1 scene number-and-name check.

### Time display

Choose **24 hour** or **12 hour** to control the clock format in the bottom status bar. The default is **24 hour**.

## Not active in v2

In v2, **Auto load last show file** does not open a session, **Auto save sessions** does not save changes, and **Fader override sensitivity** does not change manual override.

| Setting | Default | What to do instead |
| --- | --- | --- |
| **Auto load last show file** | Off | Open the required session yourself. |
| **Auto save sessions** | Off | Use **File > Save Session** after each intended change. |
| **Fader override sensitivity** | `9` | During a fade, a fader you move follows your move while other scoped controls may continue. Rehearse this response before show use. |

**Fader override sensitivity** accepts `1` through `10`, but it is inactive in v2.
