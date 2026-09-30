# Inline cue editing and insertion

Tracked in [issue #84](https://github.com/mixxorz/advanced-show-control/issues/84).

## Goal

Make precise cue-list edits without recalling scenes or interrupting drag-and-drop. This replaces the originally discussed modal with an inline editor. The implementation remains native GPUI.

## Interaction

Each cue row has a bare Edit icon beside a bare red Delete icon. Their click targets have rounded corners and use the standard button-hover background; they are not bordered buttons. Accessible labels retain the cue identity.

Edit replaces the complete cue row with a search field and up to three result rows. There is no current-cue header. The search starts with the existing scene name selected. The search field and results use the normal cue row typography, with search icons sized to match. The panel has an orange left border and occupies the row's position inside the existing scrollable list; it is not an overlay, popover, or modal.

Insertion controls appear before the first cue, between cues, and after the last cue. They occupy zero height in the list: a thin invisible hover target overlays each cue border. Hovering reveals an orange glowing line directly on the border and a small `Insert cue` control over it, without moving any cue rows. The entire visible line is clickable through the thin border hit strip, not just the label. An empty active list exposes its single insertion point. The old `DROP SCENE HERE TO APPEND` row is removed. These gaps accept scene drops, including appending at the end.

Insert opens the same inline search panel at the chosen gap with an empty query. Only one Edit or Insert panel can be open. Opening another cancels the previous draft. Escape or the search field's × icon cancels; clicking elsewhere does not. Choosing a result by click or Enter submits once, closes after success, and retains the editor with a visible error after failure. Repeated Enter cannot submit twice while pending. Normal row selection, cueing, and global recall shortcuts must not consume editor typing or confirmation keys.

Edit preserves the cue entry UUID, order, and cue selection. It changes only the scene reference, not LV1 state. Choosing the original scene is a no-op. Insert creates one new cue entry. Editing or inserting does not recall, advance a cue, or alter a fade. All scenes in the app library are offered, even offline and without fades; absent scene references are not selectable results.

## Search and layout

Use an established Rust fuzzy matcher, not a handwritten similarity heuristic. Ranking has two levels: number-led queries first match displayed scene numbers, then fuzzy scene names; text queries rank by fuzzy names. Exact displayed numbers precede number prefixes. Displayed numbers use the existing one-based, zero-padded format: `010` identifies displayed scene 010; `01` matches its prefix; `10` is not an alias for 010. An optional leading `#` can identify a number query. Stable library order resolves equal ranks. Empty queries show the first three library scenes; editing initially prefers its current scene among identical names so Enter alone cannot change to a duplicate.

Query changes highlight the first/best result. Up/Down move the highlight within the visible results without wrapping; Enter chooses it. No match displays an empty state and Enter does nothing. Scene numbers align with the cue pane's existing `#` column and names with its scene-name column. The editor, gaps, icons, and hover states use shared theme/layout tokens and the app's existing fonts, dark surfaces, and orange accent.

## Drag behavior and insertion position

The editor does not disable scene drops or cue reordering. Edit is identified by cue UUID and renders wherever that cue moves. Dropping a scene on an edited row retains the existing insert-before-row behavior rather than replacing its scene.

An Insert panel behaves as a temporary, non-persisted item in the displayed sequence, not a fixed numeric slot or an attachment to one cue. Apply list insertions, removals, and moves around this marker. For `[Song 1, panel, Song 2]`, inserting Song 0 at the start yields `[Song 0, Song 1, panel, Song 2]`. Moving Song 1 elsewhere removes Song 1 from its old place but does not take the panel with it. Moving Song 2 elsewhere similarly leaves the panel in its local position. A drop at the panel has a defined side (before it), keeping the marker after the dropped item. The resolved count of real cues preceding the marker determines the eventual insertion gap. No synthetic cue is persisted.

Keep this sequence transformation in one focused presentation helper, covered with concrete examples. Reconcile accepted projection changes by entry identity; cancel the editor on active-list/session replacement or loss of its edited cue. Commands validate the intended session revision, list, and current ordering to reject stale submissions rather than silently inserting in an obsolete numeric slot. While an Insert panel awaits a submitted drag's projected order, it shows `Updating cue order…` and briefly holds further ordering changes and result submission. This prevents overlapping drag intentions from moving the insertion point incorrectly. Unrelated updates must not reset search or selection.

## Ownership and safety

The native view owns draft query, highlighted result, editor mode, presentation marker, and pending-command correlation. A focused search/editor component hides ranking and input behavior from the cue-list renderer. The existing Scenes/Cue Lists owner validates and applies mutations; native adapters stay thin. New edit/insert commands identify the intended cue list and scene. Insertion includes sufficient order context to reject stale targets. Existing drag commands and recall paths retain their contracts.

Mutations use the persisted-edit dispatcher and publish through existing owner events/projector snapshots so dirty tracking, session guards, and save/load continue to work. No optimistic replacement of authoritative documents occurs. Current cue identity remains identity-based; LV1's current-scene indicator remains based on live scene identity. Existing lockout, recall queue serialization, generation checks, and exact-scene validation are unchanged.

## Verification

Use pure unit tests for ranked scene search and marker transformations, with literal inputs/expected outputs. Use actor-mailbox tests for edit identity/order/selection, no-op behavior, stale/unknown targets, insertion, persisted publication, and unchanged live behavior. Do not inspect side-effecting actor internals. Use native GPUI interaction tests for the app's input handling, editor replacement/cancellation, repeated-submit guard, and drag integration; do not test generic framework guarantees. Inspect native visual output for inline layout, three-result limit, number alignment, icon hover, and gap highlight. Update existing tests whose expected behavior changes instead of duplicating them.

Run targeted nextest checks during development, then `make check` and native visual checks. No hardware smoke is required for this document-edit feature; report any unavailable tooling or verification limitations.

## Scope

No modal redesign, general search framework, recall changes, new persistence schema, or unrelated cue-manager cleanup. A first testable version includes edit, insert, fuzzy search, keyboard control, drag continuity, and native styling.
