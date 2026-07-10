# Cue List Scene ID Preservation Design

## Context

Cue-list entries reference scenes by `scene_internal_id`. Session export writes every scene config, including default configs with no fade metadata, but session import currently removes those blank configs before scene alignment. Alignment then creates replacement configs with new UUIDs for the corresponding LV1 scenes while cue-list entries retain their saved UUIDs. The entries consequently appear as missing after reopening the session.

This design addresses GitHub issue #5. It does not recover already-damaged sessions whose cue references no longer have corresponding scene configs.

## Design

Remove blank scene-config pruning from `import_show_file`. All scene configs stored in a session become part of the session's durable scene identity set, whether or not they contain app-managed fade metadata.

The existing load flow remains otherwise unchanged:

1. Import every stored scene config and preserve its `internal_scene_id`.
2. Pass the imported configs to `align_scene_configs`.
3. Reuse existing safe alignment rules for exact index-and-name matches, unique-name moves, and the supported single-rename case.
4. Leave ambiguous or missing stored scenes unlinked rather than guessing.
5. Replace the cue-list document using IDs from the aligned scene document as the valid scene set.

No cue-list ID remapping is needed because alignment preserves the stored scene UUID when it can safely identify the current LV1 scene.

## Boundaries And Safety

The change remains in backend-owned show-file import and reconciliation. It does not change the frontend contract, show-file schema, cue-list types, or actor ownership.

Cue recall continues to send `ScenesCommand::RecallScene` with the cue entry's `scene_internal_id`. Existing scene lookup, fresh LV1 state checks, exact scene identity validation, lockout checks, and fade safety behavior remain unchanged.

Ambiguous alignment continues to produce an unlinked stored config and a newly identified current LV1 config. A cue referencing the unlinked config remains unavailable; the backend does not guess which LV1 scene it should recall.

## Persistence Behavior

Default scene configs are no longer treated as disposable during import. They are durable identity records because other session-owned data may reference their UUIDs.

Session files may retain more default scene configs than before. This is intentional and matches current export behavior, which already writes the complete scene document.

No backward-compatibility migration or recovery heuristic will be added.

## Testing

Use pure unit tests for show-file import behavior:

- Import preserves a blank scene config and its UUID.
- Import still generates an ID when a stored config has no `internal_scene_id` under the currently supported schema behavior.

Use an actor test through actor mailboxes for the session-load regression:

- Load a session containing multiple cue entries that reference default scene configs against the same LV1 scene library.
- Assert the aligned scene document retains each referenced UUID.
- Assert the cue-list document retains each entry and its cued selection because all referenced IDs remain valid.

Update or replace the existing test that expects blank imported configs to be dropped. Existing scene-alignment ambiguity tests and cue recall routing tests continue to cover the safety boundaries.

## Non-Goals

- Recovering cue references from sessions where the corresponding scene config is absent.
- Adding scene locator fields to cue entries.
- Changing scene alignment heuristics.
- Changing cue-list recall routing or validation.
- Changing the show-file schema.
