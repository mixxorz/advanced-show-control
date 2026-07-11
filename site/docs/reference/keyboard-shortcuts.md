# Keyboard Shortcuts

Keyboard shortcuts are divided into fixed file commands and configurable cue
actions. `CmdOrCtrl` means Command on macOS and Control on other supported
platforms.

## Fixed File Shortcuts

These shortcuts cannot be changed in Settings.

| Command | Shortcut |
| --- | --- |
| **New Session** | `CmdOrCtrl+N` |
| **Open Session...** | `CmdOrCtrl+O` |
| **Save Session** | `CmdOrCtrl+S` |
| **Save As...** | `CmdOrCtrl+Shift+S` |

## Configurable GO And CUE Shortcuts

| Action | Default shortcut | Operation |
| --- | --- | --- |
| **GO** | `Space` | Requests recall of the current valid cue. |
| **CUE** | `C` | Cues the selected entry in the active cue list. |

Change these shortcuts in [Settings](../settings.md#keyboard-shortcuts). A
configurable shortcut cannot duplicate the other configurable shortcut or a
fixed file shortcut. CUE and GO do not run when focus is in a dialog text
input, and they do not bypass unavailable-cue or **SAFE** checks.

During shortcut capture, press `Escape` to cancel. A modifier by itself is not
recorded; `Tab` is recorded as a shortcut. See
[Keyboard Shortcuts](../settings.md#keyboard-shortcuts) for the complete
capture procedure.
