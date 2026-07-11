# Settings

Use **Settings** to change the shortcuts and diagnostic detail available in v2. If a setting cannot be saved, the screen restores the last confirmed value and shows an error.

![The Settings screen.](assets/screenshots/settings.png)

## Active in v2

### Extensive diagnostics

Enable **Extensive diagnostics** only while you investigate a problem. When it is off, diagnostic files include `INFO`, `WARN`, and `ERROR`. When it is on, they also include `DEBUG`, so files can grow quickly. Disable it after you collect the information you need.

### Keyboard shortcuts

**GO** recalls the current cued entry. **CUE** prepares the selected cue-list entry. The defaults are `Space` for GO and `C` for CUE.

1. Select the GO or CUE shortcut control. It displays `...`.
2. Press the key combination you want.
3. Confirm the displayed shortcut.

Press `Escape` to cancel. Hold Shift, Control, Alt, or Meta with the key when needed. Pressing a modifier by itself keeps capture open. `Tab` can be used as a shortcut. A shortcut cannot duplicate the other action or **New Session**, **Open Session...**, **Save Session**, or **Save As...**.

Shortcuts do not operate while you are entering text or working in a dialog. Holding a shortcut does not repeat CUE or GO. A shortcut does not bypass **SAFE**, connection checks, or the LV1 scene number-and-name check.

## Not active in v2

In v2, **Auto load last show file** does not open a session, **Auto save sessions** does not save changes, **Time display** does not change the clock, and **Fader override sensitivity** does not change manual override.

| Setting | Default | What to do instead |
| --- | --- | --- |
| **Auto load last show file** | Off | Open the required session yourself. |
| **Auto save sessions** | Off | Use **File > Save Session** after each intended change. |
| **Time display** | **24 hour** | Read the clock as displayed. |
| **Fader override sensitivity** | `9` | During a fade, a fader you move follows your move while other scoped controls may continue. Rehearse this response before show use. |

**Fader override sensitivity** accepts `1` through `10`, but it is inactive in v2.
