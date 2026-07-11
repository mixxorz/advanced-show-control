# Keyboard Shortcuts

Use fixed shortcuts for session files and configurable shortcuts for prepared cue actions. `CmdOrCtrl` means Command on macOS and Control on other supported platforms.

## Fixed File Shortcuts

These shortcuts cannot be changed in **Settings**.

| Command | Shortcut | Result |
| --- | --- | --- |
| **New Session** | `CmdOrCtrl+N` | Creates a new session from the current LV1 scene list. |
| **Open Session...** | `CmdOrCtrl+O` | Opens an `.ascs` session. |
| **Save Session** | `CmdOrCtrl+S` | Saves to the current session path. |
| **Save As...** | `CmdOrCtrl+Shift+S` | Saves the session to a new path. |

## Configurable GO And CUE Shortcuts

| Action | Default shortcut | Result |
| --- | --- | --- |
| **GO** | `Space` | Requests recall of the current valid cue. |
| **CUE** | `C` | Cues the selected entry in the active cue list. |

Change these shortcuts in [Settings](../settings.md#keyboard-shortcuts). A configurable shortcut cannot duplicate the other action shortcut or a fixed file shortcut. Letter keys match without regard to case.

While a shortcut control displays `...`, press the required non-modifier key with any Shift, Control, Alt, or Meta modifier. Press `Escape` to cancel. A modifier by itself is not recorded. `Tab` is a valid shortcut.

CUE and GO do not run while focus is in a text input, text area, select control, editable content, or dialog. They also do not bypass an unavailable cue or **SAFE**. If a cue is unavailable, correct the cue state before you use GO.
