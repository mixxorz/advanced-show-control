# Cue Lists

Use **Cue Lists** to arrange app scene configurations into a prepared recall sequence. A cue list is an ordered list of references to your Advanced Show Control scene configurations. It does not create, rename, delete, or otherwise change LV1 scenes.

## Build And Run A Cue List

1. Select **Manage Cue Lists**, then create and select a list.
2. Drag the required configurations from the **Scene library** into the list.
3. Drag entries into the recall order you need.
4. Select an entry and choose **Cue**, or double-click it.
5. Confirm **Cued** in the bottom status bar, then select **GO** when you are ready to recall.

You can add the same configuration more than once. Use separate entries when the same scene is needed at more than one point in the show.

## Screen Overview

The **Scene library** on the left contains configured application scenes. The active cue-list pane on the right contains the ordered entries for the selected list.

![An active cue list with repeated scene entries.](assets/screenshots/cue-list.png)

Drag a scene from the library into the active list to create an entry. Drop over an entry to insert before it, or drop in the remaining pane area to append it. Drag an existing cue entry vertically to reorder it.

## Manage Cue Lists

Select **Manage Cue Lists** to create, select, rename, reorder, or delete lists.

![The Manage Cue Lists dialog.](assets/screenshots/manage-cue-lists.png)

| Control | Use it to | Result |
| --- | --- | --- |
| List row | Select a list. | Makes that list active and closes the dialog. Selecting the active list only closes the dialog. |
| **New Cue List** | Create a list. | Opens a name dialog, then creates the list when you confirm. |
| Rename control | Change a list name. | Opens a name dialog for that list. |
| Drag handle | Change list order. | Reorders the app-managed lists. |
| Delete control | Remove a list. | Opens **Delete Cue List** confirmation. |

Deleting a cue list removes only that app-managed list. Its scene configurations and LV1 scenes remain available. Review the list name in **Delete Cue List** before selecting **Delete**.

## Prepare A Cue

Selecting an entry only selects it. It does not prepare a recall until you use **Cue** or double-click the entry.

1. Confirm that the intended cue list is active.
2. Select the required entry once.
3. Select **Cue**, or double-click the entry.
4. Confirm that the row is marked as cued and that **Cued** in the bottom status bar names the intended scene.

**Cue** is unavailable until an entry is selected. Changing the active list clears the local selection, so select an entry in the new list before cueing it.

## Recall With GO

**GO** requests recall of the valid cued entry from the active cue list. It does not replace normal LV1 controls.

1. Confirm that **SAFE** is not active.
2. Confirm that the active list contains a valid cued entry with a linked scene configuration.
3. Compare **Cued** and **Current** in the bottom status bar with the intended transition.
4. Select **GO**, or use the configured GO shortcut.

After a successful recall, the application cues the next entry in the active list. After the final entry recalls successfully, it clears the cue and **Cued** shows `---`.

If SAFE is active, the recall is blocked. If the active list, cued entry, or linked scene configuration is unavailable, **GO** is disabled. If a request is pending, **GO** remains disabled until it resolves. Correct the condition, confirm the cued scene, and try again. Repeated activation cannot send a second request while the first is pending.

## Cue Entry States

Each entry references one app scene configuration. Its row shows the referenced scene name and number.

| State | What you see | Meaning |
| --- | --- | --- |
| Selected | Orange highlight and indicator | The entry is ready for **Cue**. |
| Cued | Blue text and indicator | This entry supplies **Cued** in the bottom status bar. |
| Current | Green text and indicator | Its referenced scene matches the current LV1 scene. |
| Missing scene | Warning text, indicator, and `---` | The referenced app scene configuration is unavailable. |

Cued state takes precedence over missing-scene styling. A cued missing entry remains blue, but **Missing scene** and `---` identify the unresolved reference.

Select the remove control at the end of an entry row to remove that entry immediately. There is no confirmation dialog. Removing an entry does not delete its scene configuration or its LV1 scene.

## Missing Scenes

Cue lists preserve a reference when its app scene configuration is no longer available. This makes the problem visible instead of silently changing your show order.

![A cued entry whose referenced scene configuration is missing.](assets/screenshots/missing-cue-scene.png)

Restore or relink the required configuration before you use that entry for recall. Until the reference resolves, it cannot supply a valid cue and **GO** remains unavailable for it.

## Keyboard Operation

The configured CUE shortcut cues the selected entry. The configured GO shortcut recalls the current valid cue. The defaults are `C` for CUE and `Space` for GO.

Action shortcuts do not run while focus is in a dialog text input. This lets you enter cue-list names without triggering a cue or recall. Repeated keydown events do not create repeated CUE or GO requests. A GO shortcut for an unavailable cue is consumed without starting a recall.

## Troubleshooting

- [GO Is Disabled](troubleshooting.md#go-is-disabled)
- [Cue Displays Missing Scene](troubleshooting.md#cue-displays-missing-scene)
