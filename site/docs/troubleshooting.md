# Troubleshooting

Perform only the immediate checks in this section during an operational
problem. Use the linked guide for the full procedure before changing scene
configuration or console state.

## No LV1 Systems Found

Confirm that the LV1 or LV1 Classic system is reachable from this computer and
that the **Connect to LV1** dialog remains open while discovery runs. Verify
that the required system appears as **Available**, then follow
[Connect To LV1](getting-started.md#connect-to-lv1).

## Connection Fails Or Reconnects

Check the top-bar connection state and confirm that the intended system remains
available in the connection dialog. During reconnection, wait for the
**Reconnecting...** overlay to complete before attempting recall. Follow
[Reconnection States](application-shell.md#reconnection-states) if the
application returns to the connection workflow.

## Scene Is Unlinked

An unlinked configuration displays scene number `---`; **Store** and
**Recall** are unavailable. Select the intended LV1 scene, link the
configuration, then verify both its number and name. See
[Link A Scene](scenes.md#link-a-scene).

## Duplicate Scene Name Warning

Use the scene number together with the scene name to identify the intended
configuration. Do not rely on a matching name alone. Review
[Duplicate Scene Names](scenes.md#duplicate-scene-names) before overwriting
existing fade settings.

## Recall Is Disabled Or Blocked

Confirm that the selected configuration is linked, LV1 is connected, and
**SAFE** is not active. Verify the displayed scene number and name against the
intended LV1 scene. See [Recall A Scene](scenes.md#recall-a-scene).

## Fade Does Not Start

Confirm that the recalled configuration has stored channel data, at least one
scoped channel, and an active **FADER** or **PAN** parameter scope. Confirm
that the recall passed the validation checks described in
[Recall A Scene](scenes.md#recall-a-scene). A blocked or skipped recall does
not stop an existing fade.

## GO Is Disabled

Confirm that an active cue list, a cued entry, and that entry's linked scene
configuration are available. Wait for any pending GO request to finish, and
confirm that **SAFE** is not active. See
[Recall With GO](cue-lists.md#recall-with-go).

## Cue Displays Missing Scene

The cue entry remains in the list, but its referenced application scene
configuration is unavailable. Restore or relink that configuration before
using the cue. See [Missing Scenes](cue-lists.md#missing-scenes).

## Session Cannot Be Opened Or Saved

Use the native **File** menu and select an `.ascs` session file when opening.
For a new session, choose a save location through **Save Session** or **Save
As...**. See [File Menu And Sessions](application-shell.md#file-menu-and-sessions).

## No Visible Log Explains A Failure

Open the **Logs** tab and review the timestamp, severity, and message around
the failure. The screen does not display `DEBUG` diagnostics. If no relevant
entry is visible, inspect the diagnostic file path and reporting guidance in
[Logs](logs.md#operational-and-diagnostic-logs).

## A Setting Does Not Change Application Behavior

**Auto load last show file**, **Auto save sessions**, **Time display**, and
**Fader override sensitivity** are displayed and stored settings that are not
currently wired to change runtime behavior. Review
[General Settings](settings.md#general-settings) for their current behavior.
