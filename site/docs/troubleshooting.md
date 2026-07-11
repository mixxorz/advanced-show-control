# Troubleshooting

Use these checks to identify the immediate condition that is blocking your work. Correct the condition, then return to the linked guide before you change a scene configuration or console state.

## No LV1 Systems Found

If the required console does not appear in **Connect to LV1**, the application cannot connect and recalls remain unavailable. Confirm that the LV1 or LV1 Classic system is reachable from this computer, then leave **Connect to LV1** open while discovery runs. Select the system only after it appears as **Available**. See [Connect To LV1](getting-started.md#connect-to-lv1).

## Connection Fails Or Reconnects

If the top bar shows **Offline**, **Connecting**, or **Reconnecting...**, the application does not have a ready LV1 connection for recall. Confirm that the intended system remains **Available** in **Connect to LV1**. Wait for **Reconnecting...** to finish before you attempt recall. If the connection workflow returns, select the available console again, confirm **Connected**, then verify the current scene. See [Connect To LV1](application-shell.md#connect-to-lv1).

## Scene Is Unlinked

An unlinked configuration shows `---` as its scene number, and **Store** and **Recall** are unavailable. Select the intended LV1 scene in **LV1 Scene**, select **Link to scene**, then verify both the displayed number and name. A matching name alone is not enough. See [Link Or Relink A Scene](scenes.md#link-or-relink-a-scene).

## Duplicate Scene Name Warning

**Duplicate scene names** means that more than one app configuration has the same stored name. Recalling or overwriting by name alone may affect the wrong configuration. Compare both the scene number and name before you store, relink, or select **Overwrite**. See [Work With Duplicate Names](scenes.md#work-with-duplicate-names).

## Recall Is Disabled Or Blocked

If **Recall** is unavailable or the recall does not start a fade, check the linked configuration, LV1 connection, and **SAFE** state. Verify the displayed scene number and name against LV1. Disable **SAFE** only when you are ready to operate, then retry the intended recall. See [Recall A Scene](scenes.md#recall-a-scene).

## Fade Does Not Start

If LV1 recalls but no transition follows, the configuration may have no eligible targets or the recall may have failed validation. Confirm that targets have been stored, at least one channel is scoped, and **FADER** or **PAN** is active. Then confirm that the recalled scene number and name exactly match the linked configuration. A blocked or skipped recall does not stop a fade already in progress. See [Recall A Scene](scenes.md#recall-a-scene).

## GO Is Disabled

If **GO** is unavailable, the active list may have no valid cued entry, its configuration may be missing or unlinked, or another GO request may be pending. Confirm the active list, **Cued** scene, and linked configuration. Wait for a pending request to finish, and confirm that **SAFE** is not active. See [Recall With GO](cue-lists.md#recall-with-go).

## Cue Displays Missing Scene

**Missing scene** and `---` mean that the cue entry still exists but its app scene configuration is unavailable. The entry cannot provide a valid cue for **GO** until you restore or relink that configuration. See [Missing Scenes](cue-lists.md#missing-scenes).

## Session Cannot Be Opened Or Saved

If you cannot open or save a session, use the native **File** menu and select an `.ascs` file when opening. For a new session, choose a path through **Save Session** or **Save As...**. Check the window title for an asterisk after changes, then save before you open, create, or close a session. See [Sessions And File Commands](application-shell.md#sessions-and-file-commands).

## No Visible Log Explains A Failure

If **Logs** does not explain an operation, review the timestamp, severity, and message around the event. The screen does not show `DEBUG` entries. When the visible entries are insufficient, inspect the relevant diagnostic file and collect a small, relevant excerpt. See [Diagnostic Files](logs.md#diagnostic-files).

## A Setting Does Not Change Application Behavior

**Auto load last show file**, **Auto save sessions**, **Time display**, and **Fader override sensitivity** are stored preferences that do not currently change runtime behavior. Use the manual session-save workflow and rehearse the existing fader-override behavior instead. See [General Settings](settings.md#general-settings).
