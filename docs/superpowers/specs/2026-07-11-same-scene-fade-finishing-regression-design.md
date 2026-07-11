# Same-Scene Fade Finishing Regression Design

## Context

GitHub issue #42 tracks a regression in scene-owned fade behavior. Commit `376642f` introduced the rule that recalling the same validated scene while it owned active fade targets finished only those targets at their exact stored values. Commit `7ec29ec` later removed active-target scene ownership and the scene-scoped finish path while adding pan-family fade support.

The current engine retains `FadeConfig::scene`, but `ActiveTarget` does not retain that identity. A repeated scene recall therefore replaces matching parameter targets with another timed fade instead of finishing all active targets owned by that exact scene.

The post-recall ping readiness barrier from GitHub issue #35 now pauses all fade writes after a validated timed recall. Restored same-scene finishing must use that barrier rather than reintroducing the old direct-write helper.

## Goal

Restore exact-scene ownership to active fade targets. After full recall validation, recalling an exact scene identity that owns active targets finishes only that scene's targets at their stored final values through the existing readiness-gated scheduler and completion path.

## Chosen Approach

Represent a same-scene finish by rewriting the matching active timelines for instant completion.

Each `ActiveTarget` will retain its owning `FadeSceneIdentity`. When a validated recall arrives for a scene that owns one or more active targets, the fade engine will rewrite only those targets so that the normal tick scheduler considers them complete. Their stored `target_value` remains unchanged.

The recall will reset the connection-wide post-recall readiness barrier through the same path as other validated timed recalls. No final parameter write occurs until the barrier releases. Once released, the normal scheduler sends each rewritten target's exact stored final value, publishes its existing completion facts, and removes it.

This avoids a second write path and keeps generation checks, readiness gating, parameter dispatch, completion events, and target removal centralized.

## Active Target Ownership

Add `scene: FadeSceneIdentity` to `ActiveTarget` and `ActiveTargetInit`. Every timed target created from `FadeConfig` receives `config.scene.clone()` as its owner.

Ownership is the existing exact identity pair:

- LV1 scene index.
- LV1 scene name.

Both fields must match. This change does not weaken or replace scene validation.

Different-scene recalls preserve current parameter-key overlap behavior:

- Incoming targets replace existing targets with the same `FadeTargetKey`.
- The replacement target receives the incoming scene identity.
- Existing targets with other keys continue with their original scene ownership.

## Same-Scene Recall Flow

The scenes actor retains ownership of recall policy and validation. Blocked, skipped, disabled, stale, unsafe, or otherwise invalid recalls must not send a fade command and therefore cannot alter target ownership or readiness state.

For a fade command that reaches the fade actor:

1. Recheck the expected runtime generation.
2. Reject an empty target set as the existing no-op.
3. Obtain fresh LV1 state through the LV1 actor.
4. Recheck the expected runtime generation.
5. Determine whether any active target is owned by `config.scene`.
6. If the exact scene owns active targets, rewrite only those targets for instant completion.
7. Reset post-recall readiness using the fresh snapshot's ping boundary.
8. Return without creating replacement timelines from the incoming target list.
9. After readiness releases, let the normal tick path send exact final values and remove completed targets.

The active targets' stored final values are authoritative for same-scene finishing. The incoming configuration is used to identify the validated scene, not to replace targets or broaden the finish scope.

If the scene owns no active target, the existing timed fade creation and parameter-key replacement flow remains unchanged.

## Instant Timeline Rewrite

Add a narrow `ActiveTarget` operation that makes an active target complete on the next eligible scheduler tick. It must:

- Preserve `scene`, `key`, channel coordinates, `target_value`, and expected generation.
- Set timing so `is_done` is true when the scheduler resumes.
- Preserve compatibility with a target that is already paused by an earlier readiness barrier.
- Avoid sending a parameter write itself.

Rewriting happens before the latest recall resets readiness. The readiness reset must retain the original pause boundary for targets already paused, matching the existing repeated-recall behavior. Waiting time must not create an intermediate write or consume unrelated targets' timelines.

The normal completion branch remains responsible for calling the exact-final-send operation, building the parameter-specific write, validating generation, publishing completion facts, and removing the target.

## Safety Behavior

- Full recall validation remains before any same-scene rewrite.
- Exact scene identity remains index plus name.
- The latest validated recall resets the post-recall ping barrier.
- Rewritten targets send nothing before two qualifying same-generation pings release readiness.
- A readiness timeout aborts rewritten and unrelated paused targets through the existing global safe-abort behavior.
- A generation change, disconnect, Abort All, unavailable LV1 state, or actor shutdown prevents later final writes.
- Manual override remains target-scoped. If an override removes a rewritten target while readiness is closed, that target cannot send its final value later.
- Targets owned by other scenes are paused by the connection-wide readiness barrier but are not finished, replaced, or removed by the same-scene decision.
- A different scene continues to take ownership only of overlapping parameter keys.
- Zero-duration recall behavior remains unchanged and does not become the mechanism for same-scene timed finishing.

## Events And Logging

Use the existing per-target completion and global fade-completion behavior. A same-scene finish emits global completion only when no active targets remain after the rewritten targets complete.

Add one `INFO` event at the fade layer when matching timelines are rewritten. It must use `tracing`, include the stable event field `fade_same_scene_finishing`, and state that the repeated scene recall is finishing that scene's active fade targets. Include scene identity and target count as diagnostic fields. Do not duplicate the same outcome in the scenes actor.

Normal ping progress and successful readiness release remain `DEBUG` diagnostics. This change must not add ping noise to the frontend log.

## Testing

Use test-driven development and the repository's approved Rust test styles.

### Pure Unit Tests

Test `ActiveTarget` timeline behavior directly without side effects:

- Rewriting a running target makes it complete on the next eligible tick.
- Rewriting preserves the exact stored target value, scene identity, parameter key, and generation.
- Rewriting a paused target remains compatible with readiness resume.

### Actor Tests

Use the fade mailbox, fake LV1 mailbox, `AppEventBus`, and observable LV1 write batches. Do not mutate or inspect private actor state.

Cover:

- A repeated exact-scene recall waits for the post-recall ping barrier and then sends exact final values for all targets owned by that scene.
- Targets owned by another scene receive no final write and continue after readiness releases.
- Same-scene finishing works across fader, pan, balance, and width targets through the normal parameter dispatch path.
- A different-scene recall replaces only overlapping parameter keys and assigns the replacement to the new owner.
- Manual override during readiness removes the affected rewritten target and prevents its later final write.
- Abort All, disconnect, generation change, and readiness timeout prevent deferred final writes.
- Global fade completion occurs only when the full active target set becomes empty.

Use scenes actor tests for the validation boundary:

- Blocked, skipped, disabled, stale, and unsafe recalls do not send a fade command and do not disturb an existing fade.
- Exact identity validation still requires both scene index and scene name.

Strengthen or replace the misleading existing same-scene test so it asserts exact writes, scene-scoped completion, and unrelated-scene continuation rather than only the absence of an early global completion event.

## Documentation

Update `docs/architecture.md` to state that active targets retain exact scene ownership and that repeated validated scene recalls rewrite only that scene's targets for readiness-gated exact completion.

Historical design documents remain unchanged.

## Non-Goals

- Changing scene identity matching.
- Changing recall validation or lockout policy.
- Changing the two-ping readiness threshold or timeout.
- Adding a separate finish command or direct final-write path.
- Changing parameter interpolation, manual-override thresholds, or overlap keys.
- Adding retries or parameter-echo acknowledgement for final writes.
- Redesigning frontend fade status.

## Exit Criteria

- Every timed active target retains its exact owning scene identity.
- Recalling an exact validated scene that owns active targets finishes only those targets.
- Exact final values are sent through the normal scheduler after post-recall readiness releases.
- Other scenes' targets remain owned and continue after the connection-wide pause.
- Manual override and lifecycle cancellation prevent stale deferred writes.
- Different-scene overlap behavior remains parameter-key scoped.
- The regression is covered by observable actor tests and focused pure timeline tests.
