# Scene Tracking

LV1 remains authoritative for scene creation, ordering, names, and current recall state. ASC stores a separate app-lifetime scene document and reconciles its LV1 links only when the current generation's scene library is available.

## Identity and Reconciliation

Each ASC config has a durable internal UUID. A linked config stores its current LV1 `scene_index` and `scene_name`; an unlinked config has no index. The UUID is the ASC and cue-list identity. The index/name locator is used only to follow and validate the linked LV1 scene.

Reconciliation preserves a config UUID and fade data only for an unambiguous association:

- exact current index and name;
- a name unique in both remaining old configs and the new LV1 list; or
- exactly one same-index rename in an otherwise matching list.

New LV1 scenes receive default linked configs. Deleted or ambiguous old links become unlinked and retain their data and order; existing unlinked configs are never automatically relinked. ASC deliberately does **not** FIFO-guess duplicate names, multi-renames, or other ambiguous changes. Exact matches remain linked; uncertain new entries get fresh defaults and uncertain old configs become unlinked.

Selection and the settings clipboard survive reconnect. The LV1-derived library does not: `AwaitingPeers`, `AwaitingSceneList`, and `Ready` gate live operations. Link, capture/store, and recall require `Ready`; document-only edits do not.

## Cue-List Coordination

Cue entries reference scene config UUIDs, never LV1 locators. For ordinary scene-document updates, Cue Lists reconciles against the current valid UUID set and retains entries while clearing invalid active/cued references. For `FileReplacement`, Show coordinates the transaction: it passes the replacement Cue List document and valid scene UUID set directly to Cue Lists, because Cue Lists intentionally ignores the replacement Scenes event for reconciliation.

## Recall Coordination

Scene tracking supplies the durable identity and exact LV1 locator used by both recall paths. The complete recall algorithm—including ASC-originated and LV1-originated recalls, dispatch correlation, fresh-state validation, Fade readiness, overlapping fades, timing, and the configurable ASC recall interval—is documented in [Scene Recall Coordination](scene-recall.md).
