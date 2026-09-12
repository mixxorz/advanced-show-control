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

## Recall Paths

An **explicit ASC recall** queues a requested config, dispatches LV1 recall only after validation, and holds its caller reply until dispatch. The resulting later matching LV1 scene observation provides the post-dispatch boundary before Fade readiness and the next queued request.

An **event-driven fade** starts from an LV1 `SceneChanged` observation, including an operator recall performed outside ASC. It does not send an LV1 recall command. After settle and policy gates, it obtains fresh LV1 state and must validate generation, connection, lockout, exact index/name, linked config, scope/targets, and live topology before Fade admission.

Both paths use the same safety validation. A blocked, skipped, disabled, or ambiguous event before admission does not abort an active fade. A validated/admitted recall—including zero-duration or no-target cases—enters post-recall readiness; its timeout aborts paused fades.

## Timing and Correlation

The recall actor applies these concrete safety windows:

- **25 ms settle delay:** lets current-scene frames stabilize before policy evaluation.
- **2 s arming window:** observations establish the reconnect baseline rather than triggering a fade.
- **500 ms scene-list-edit suppression:** avoids intermediate list-edit state.
- **Configurable same-scene repeat suppression:** 500 ms by default; it suppresses repeated exact observations, not explicit queue entries.
- **5 s queue deadline:** spans the exact post-dispatch observation and Fade readiness.

`SceneObservation.sequence` is connection-local. An explicit queued recall requires an observation with a sequence later than its dispatch sequence and the exact requested index/name. Fade readiness then requires two newer same-generation LV1 pings. Canceled in-flight requests leave bounded five-second late-observation suppression records; overflow uses a five-second fail-closed suppression fallback rather than guessing correlation.

These gates are not retries. The recall actor uses fresh LV1 state where subscriber ordering could otherwise create a stale decision, and exact scene matching remains mandatory before any fader command.
