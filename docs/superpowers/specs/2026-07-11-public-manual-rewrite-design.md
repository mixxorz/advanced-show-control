# Public Manual Rewrite Design

## Goal

Rewrite the Advanced Show Control public manual so an experienced live-sound engineer can install the application, configure a scoped scene fade, and operate cue lists without needing internal implementation knowledge.

## Editorial Approach

The manual will use the current Zensical navigation and screenshots. Each primary page will open with the operator's purpose, then cover the normal workflow, visible interface, and conditions that change the result. The copy will use exact control labels and measured technical language. Warnings will name the condition, consequence, and corrective action.

The home page will act as a product introduction and route readers to the stable v2 download assets, the first-success tutorial, and the guide. The tutorial will follow one complete session from installation through a stored fade and observed recall result. Screen guides will retain their existing factual coverage but present purpose and common use before control tables and exceptional states.

## Facts To Preserve

- LV1 and LV1 Classic create and own scenes; Advanced Show Control stores the scoped fade overlay and session data.
- Fades use current live values as their starting point and stored targets as their destination after a validated recall.
- `X-Fade` accepts `0` for a cut and `0.1` through `120` seconds for a timed fade.
- SAFE blocks application-initiated recalls but does not disable LV1 controls. The manual must not describe an Abort All control.
- Validation requires the linked LV1 scene number and name to match exactly, an LV1 connection, and current live channel data. Blocked, skipped, disabled, or disconnected recalls do not start a fade or stop an existing one.
- A manual fader adjustment yields control of that fader target; disconnecting cancels active fade activity.
- Cue lists are app-managed ordered references. GO recalls a valid cued entry, advances after a successful recall, and is unavailable when its required cue state is unavailable or a request is pending.
- Settings marked as stored-only must remain described as stored-only. Keyboard defaults are `Space` for GO and `C` for CUE.
- Stable v2 assets are `Advanced-Show-Control_v2_Windows_x64_Setup.zip` and `Advanced-Show-Control_v2_macOS_universal.dmg`.

## Verification

Run the documentation build and relevant frontend tests after the rewrite. Review each public Markdown page against the tone guide, check release URLs, and inspect the rendered documentation output for broken links or structure errors.
