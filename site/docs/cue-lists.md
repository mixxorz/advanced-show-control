# Cue Lists

Use **Cue Lists** to arrange scene fade settings in recall order. A cue list points to scene fade settings; it does not change LV1 scenes.

## Build And Run A Cue List

1. Select **Manage Cue Lists**, create a list, and select it.
2. Drag scene fade settings from the **Scene library** into the list.
3. Drag entries into the order you need.
4. Select an entry and choose **Cue**, or double-click it.
5. Confirm **Cued**, then use **GO** when you are ready.

The same scene fade setting can appear more than once.

![An active cue list with repeated scene entries.](assets/screenshots/cue-list.png)

If no list is active, the pane shows **No active cue list.** Select or create a list before adding entries.

## Manage Cue Lists

![The Manage Cue Lists dialog.](assets/screenshots/manage-cue-lists.png)

Use **Manage Cue Lists** to create, select, rename, reorder, or delete lists. Deleting a list removes only that cue list. Its scene fade settings and LV1 scenes remain unchanged.

## Cue And GO

Selecting an entry highlights it. Select **Cue** or double-click the entry to make it the next cue. The row turns blue and **Cued** identifies its scene.

Before **GO**, confirm **SAFE** is off and compare **Cued** with the intended LV1 scene. After a successful recall, the next entry is cued. After the last entry, **Cued** shows `---`.

**GO** is available when **Cued** points to a scene fade setting. It remains available when that setting is unlinked. If you use **GO** for an unlinked setting, the recall is refused and the cue remains in place. Link the setting to the intended LV1 scene, confirm its number and name, then use **GO** again.

**GO** is unavailable when there is no active list, no cued entry, the referenced scene fade setting is missing, or a recall is still running. Wait until the recall completes before using **GO** again. Holding the GO shortcut does not repeat the recall.

## Cue Entry States

| State | Meaning |
| --- | --- |
| Selected | The entry is ready for **Cue**. |
| Cued | The entry supplies **Cued** in the bottom status bar. |
| Current | Its LV1 scene matches the current LV1 scene. |
| Missing scene | The referenced scene fade setting is unavailable. |

![A cued entry whose referenced scene fade setting is missing.](assets/screenshots/missing-cue-scene.png)

Select the remove control to remove an entry. This does not delete its scene fade setting or LV1 scene.

## Keyboard Operation

The default CUE shortcut is `C`. The default GO shortcut is `Space`. Shortcuts do not operate while you are entering a cue-list name or working in a dialog. Holding either shortcut does not repeat the action.

## Troubleshooting

- [GO Is Disabled Or Refused](troubleshooting.md#go-is-disabled-or-refused)
- [Cue Displays Missing Scene](troubleshooting.md#cue-displays-missing-scene)
