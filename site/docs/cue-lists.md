# Cue lists

Cue lists put your scene fades in show order without changing the scene order in LV1. Use them to prepare the next scene, confirm it, and recall it with **GO**.

![A cue list with repeated scene entries.](assets/screenshots/cue-list.png)

## Build a cue list

1. Select **Manage Cue Lists**.
2. Create a list and make it active.
3. Drag scene fades from the **Scene library** into the list.
4. Drag entries into show order.

The same scene can appear more than once. Removing an entry removes it only from the cue list; the scene fade and LV1 scene are unchanged.

If no list is active, select or create one before adding entries.

## Manage lists

![The Manage Cue Lists dialog.](assets/screenshots/manage-cue-lists.png)

Use **Manage Cue Lists** to create, select, rename, reorder, or delete lists. Deleting a list does not delete its scenes or fade settings.

## Prepare the next cue

Select an entry, then select **Cue**. You can also double-click the entry. The next row receives a green arrow, and its scene name appears under **Next** in the bottom status bar.

Selecting a row does not cue it. This lets you inspect or edit the list without changing the scene prepared for **GO**.

## Use GO

Before pressing **GO**:

1. Confirm that **SAFE** is off.
2. Check the scene shown under **Next**.
3. Confirm that it is the transition you want to run next.

After a successful **GO**, the executed cue receives a blue arrow and the following entry becomes **Next**. After the last entry, **Next** returns to `---`.

When you press **GO**, the submitted cue immediately receives an amber arrow and the green **Next** arrow moves to the following cue. Each additional deliberate **GO** press queues that next cue in order and advances the green arrow again. Up to eight GO commands can remain unsettled at once; **GO** becomes available again as commands settle. A pointer double-click counts as one press, and holding the keyboard shortcut does not repeat it.

**GO** is unavailable when there is no active list, no cued entry, or the cue refers to a scene fade that no longer exists. An unlinked scene fade can still appear under **Next**, but **GO** will refuse the recall. Link the fade to the intended LV1 scene, confirm the scene number and name, then try again.

## Cue entry states

| Indicator | Meaning |
| --- | --- |
| Blue arrow | The current, successfully executed cue. |
| Green arrow | The next cue prepared for **GO**. |
| Amber arrow | A cue pending in the ordered execution queue. |
| Blue dot beside the scene number | The cue references the scene currently active in LV1. More than one row can show this dot. |
| Selected row | The entry you are working with. |
| Missing scene | The scene fade used by this entry no longer exists. |

Scene numbers remain neutral so their color does not imply cue state.

![A cued entry whose scene fade is missing.](assets/screenshots/missing-cue-scene.png)

## Keyboard operation

The default shortcut for **Cue** is `C`. The default shortcut for **GO** is `Space`.

Shortcuts are paused while you enter text or work in a dialog. Holding either key does not repeat the action. You can change both shortcuts in [Settings](settings.md#keyboard-shortcuts).

## Troubleshooting

- [GO Is Disabled Or Refused](troubleshooting.md#go-is-disabled-or-refused)
- [Cue Displays Missing Scene](troubleshooting.md#cue-displays-missing-scene)
