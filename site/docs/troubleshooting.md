# Troubleshooting

Use these checks to identify the condition preventing the result you expect. Correct it, then return to the named page before changing console state.

## No LV1 Systems Found

If the console does not appear in **Connect to LV1**, you cannot connect or recall. Confirm the LV1 system is reachable, leave **Connect to LV1** open, and select it only after it shows **Available**. See [Getting Started](getting-started.md#connect-to-lv1).

## Connection Fails Or Reconnects

If the top bar shows **Offline**, **Connecting**, or **Reconnecting...**, a scene fade cannot start. Wait for **Connected**. If it does not return, open [Connect To LV1](application-shell.md#connect-to-lv1), select the intended available console, and confirm **Current**.

## Scene Is Unlinked

An unlinked scene fade setting shows `---`; **Store** and **Recall** are unavailable. Select the intended LV1 scene in **LV1 Scene**, select **Link to scene**, then compare its number and name. See [Scenes](scenes.md#link-or-relink-a-scene).

## Recall Is Disabled Or Does Not Fade

If **Recall** is unavailable, the setting is unlinked. If LV1 recalls but no fade follows, confirm **SAFE** is off, LV1 is connected, the setting has stored values, and **FADER** or **PAN** is on for scoped channels. Compare the LV1 scene number and name with the setting, correct any difference, then retry. See [Recall A Scene](scenes.md#recall-a-scene).

## GO Is Disabled Or Refused

If **GO** is disabled, select an active cue list, cue an entry linked to an available scene fade setting, and confirm **Cued**. Wait for a running recall to finish and confirm **SAFE** is off.

If **GO** is enabled but the setting is unlinked, the recall is refused and the cue remains. Link the setting to the intended LV1 scene, compare its number and name, then use **GO** again. See [Cue Lists](cue-lists.md#cue-and-go).

## Cue Displays Missing Scene

**Missing scene** and `---` mean the cue points to a scene fade setting that is no longer available. Restore that setting before cueing it. Until then, it cannot supply **Cued** for **GO**.

## Session Cannot Be Opened Or Saved

Open an `.ascs` file through **File > Open Session...**. Use **Save Session** or **Save As...** to choose a path for a new session. If the title has an asterisk, save before you open, create, or close a session.

## No Visible Log Explains A Failure

If **Logs** has no message for the problem, inspect the timestamp and severity around the operation. The screen excludes `DEBUG` messages. Enable **Extensive diagnostics** if you need more detail, then inspect the diagnostic file described in [Logs](logs.md#diagnostic-files).

## A Setting Does Not Change Application Behavior

**Auto load last show file**, **Auto save sessions**, **Time display**, and **Fader override sensitivity** are inactive in v2. Use the manual session-save workflow and rehearse manual fader override. See [Not Active In v2](settings.md#not-active-in-v2).
