# Troubleshooting

Use this page when a connection, recall, cue, save, or fade does not do what you expect. Correct the stated problem, then follow the linked steps before you try the task again.

## No LV1 systems found

If the console does not appear in **Connect to LV1**, you cannot connect or recall. Confirm the LV1 system is reachable, leave **Connect to LV1** open, and select it only after it shows **Available**. See [Getting Started](getting-started.md#2-connect-to-lv1).

## Connection fails or reconnects

A scene fade cannot start while LV1 is disconnected or connecting. The top bar shows **Connected**, **Connecting**, or **Offline**. Use the [Top Bar](application-shell.md#top-bar) to open **Connect to LV1**, select the intended available console, then confirm **Current** after the status shows **Connected**.

## Scene is unlinked

An unlinked scene fade setting shows `---`; **Store** and **Recall** are unavailable. Select the intended LV1 scene in **LV1 Scene**, select **Link to scene**, then compare its number and name. See [Link A Missing Scene](scenes.md#link-a-missing-scene).

## Recall is disabled or does not fade

If **Recall** is unavailable, the setting is unlinked. If LV1 recalls but no fade follows, confirm **SAFE** is off, LV1 is connected, the setting has stored values, and **FADER** or **PAN** is on for scoped channels. Compare the LV1 scene number and name with the setting, correct any difference, then retry. See [Recall A Scene](scenes.md#recall-a-scene).

## GO is disabled or refused

If **GO** is disabled, select an active cue list, cue an entry linked to an available scene fade setting, and confirm **Cued**. Wait for a running recall to finish and confirm **SAFE** is off.

If **GO** is enabled but the setting is unlinked, the recall is refused and the cue remains. Link the setting to the intended LV1 scene, compare its number and name, then use **GO** again. See [Use GO](cue-lists.md#use-go).

## Cue displays Missing scene

**Missing scene** and `---` mean the cue points to its original scene fade setting, which is no longer available. Restore that original setting if possible. Otherwise, remove the missing cue entry, add the replacement or restored scene fade setting to the cue list, then cue it again. If you need to relink a setting, select it, choose the intended LV1 scene in **LV1 Scene**, and select **Link to scene**. See [Link A Missing Scene](scenes.md#link-a-missing-scene) and [Build A Cue List](cue-lists.md#build-a-cue-list). Until you replace the entry, it cannot supply **Cued** for **GO**.

## Session cannot be opened or saved

Open an `.ascs` file through **File > Open Session...**. Use **Save Session** or **Save As...** to choose a path for a new session. If the title has an asterisk, save before you open, create, or close a session.

## No visible log explains a failure

If **Logs** has no message for the problem, inspect the timestamp and severity around the operation. The screen excludes `DEBUG` messages. Enable **Extensive diagnostics** if you need more detail, then inspect the diagnostic file described in [Logs](logs.md#diagnostic-files).

## An inactive setting does not produce a result

**Auto load last show file**, **Auto save sessions**, and **Fader override sensitivity** are inactive in v2. Open and save the session yourself. During rehearsal, move a fading fader to confirm that your move takes control of it. See [Not Active In v2](settings.md#not-active-in-v2).
