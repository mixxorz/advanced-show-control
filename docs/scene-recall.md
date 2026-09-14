# Scene Recall Coordination

LV1 owns scene recall. Advanced Show Control (ASC) validates and sends explicit LV1 recall commands, observes recalls performed directly in LV1, and applies configured fades after the recalled scene has been identified exactly. This document describes how scene recall, Fade readiness, overlapping fades, and queued ASC recalls fit together.

Scene identity and scene-list reconciliation are documented separately in [Scene Tracking](scene-tracking.md). Runtime ownership and generation wiring are documented in [Backend Architecture](architecture.md).

## Recall Paths

ASC supports two recall paths.

### ASC-originated recall

A Recall command or a resolved cue-list GO enters the Scenes actor's eight-request FIFO. Before admission and again before dispatch, Scenes obtains fresh LV1 state and validates the current connection generation, lockout, scene-library availability, and the requested config's exact LV1 index and name.

The caller's command reply remains pending while the request waits for cue resolution or in the recall FIFO. A successful reply means the LV1 recall command was dispatched. It does not mean Fade readiness or the configured recall interval has completed. Cue-list GO advances the cue only after that successful dispatch, so additional GO commands resolve in order against each newly advanced cue.

GO accepts additional distinct presses while earlier GO commands are unsettled, up to eight unsettled commands. The pointer handler ignores platform-reported follow-up clicks in a multi-click sequence, and the keyboard handler ignores held-key repeats. These presentation guards prevent accidental or unbounded submission; backend recall validation and cancellation remain authoritative.

### LV1-originated recall

A scene recalled directly in LV1 produces a `SceneChanged` observation. ASC does not send another LV1 recall command. After the observation settles, Scenes validates generation, connection, lockout, exact index/name, linked config, live topology, enabled scope, and required stored targets before admitting any Fade work.

A blocked, skipped, disabled, stale, or ambiguous direct LV1 observation does not abort an active fade. It has no queue-readiness owner because it did not originate from an ASC request. By contrast, an exact observation caused by an ASC-originated recall must still complete readiness when ordinary Fade policy is skipped, blocked by fade configuration or topology, or has no targets; otherwise ASC could dispatch the next queued LV1 recall while the console is still processing the first one. Lockout, disconnect, generation change, and other runtime-safety failures cancel the queued request instead of creating readiness.

## LV1 Observation Acceptance

LV1-originated observations pass additional gates before normal Fade policy:

- **2-second arming window:** after connection or scene-library recovery, observations update the current-scene baseline instead of triggering a fade. The last observation seen during this window becomes the baseline.
- **500 ms scene-list-edit suppression:** a changed scene list temporarily suppresses observations so an intermediate edit state cannot trigger a fade. Resending an identical list does not reopen the window.
- **Configurable same-scene repeat suppression:** an exact index/name observation repeated below the configured threshold is suppressed. The default is 500 ms, and an observation exactly at the boundary is eligible. This suppresses observations, not admission of distinct explicit FIFO requests.

An exact observation correlated to an ASC-originated request still progresses that request through readiness when ordinary Fade policy is suppressed, skipped, blocked by fade configuration or topology, or has no targets. This preserves queue safety without treating the observation as an independent Fade admission.

## ASC Recall State Machine

An ASC-originated request moves through these phases:

```text
admission validation
    -> FIFO wait
    -> fresh dispatch validation
    -> LV1 recall dispatch
    -> exact newer scene observation
    -> 25 ms observation settle
    -> current Settings and fresh exact LV1 snapshot
    -> Fade admission or readiness-only handoff
    -> two-ping Fade readiness
    -> configured ASC recall interval
    -> next FIFO dispatch
```

The Scenes actor owns these phases through one synchronous `RecallCoordinator`. External Settings, LV1, and Fade waits are actor-owned pending operations. The actor continues processing runtime facts, LV1 facts, lockout changes, recall deadlines, and safety commands while those operations are pending.

## Dispatch Correlation

The LV1 actor captures the current connection-local `SceneObservation.sequence` before it queues recall bytes, then returns that value with the successful dispatch result. The in-flight ASC request accepts only a later observation with both the expected scene index and expected scene name.

This prevents an observation that occurred before dispatch from satisfying the request. Sequence values are scoped to one connection generation and reset after reconnect; they are not durable scene identifiers.

After a matching observation, Scenes waits 25 ms so scene name and index frames can settle. It then refreshes Settings and requests fresh LV1 state. Fresh-state validation requires a connected snapshot whose current scene still matches the expected index and name. Returned mismatched or disconnected snapshots are retried for up to two seconds, bounded by the existing recall safety deadline; an LV1 state-request error returns immediately.

## What Fade Readiness Means

A matching scene observation means that LV1 reported the recalled scene. It does not prove that LV1 has finished processing the recall or resumed normal command handling.

Fade readiness means all of the following occurred for the current connection generation:

1. Scenes accepted an exact recalled index and name. For an ASC-originated recall, the observation must also be later than that request's dispatch boundary.
2. Fresh LV1 state confirmed that exact current scene and a connected transport.
3. Fade captured the snapshot's current keepalive ping sequence.
4. Fade observed two strictly newer keepalive ping sequences before the recall safety deadline.
5. Neither Fade nor Scenes lost required facts through event-bus lag.

The two pings are a liveness signal that LV1's control loop resumed its normal keepalive cadence after scene recall. They do not prove that every plugin has loaded, every audio process has settled, or every LV1 parameter has reached a final value.

Pings received before the fresh snapshot baseline do not count. Pings received while the snapshot request is pending are retained in a bounded buffer and replayed only when their sequence is newer than the snapshot baseline. A diagnostic `observed_ping_count` of zero means no qualifying post-baseline ping reached the readiness barrier before failure.

## Fade Behavior During Readiness

Fade owns one connection-wide readiness barrier. Installing or replacing that barrier pauses every active target at the same pause boundary. When a recall installs new nonzero-duration targets, they immediately participate in that pause. Same-scene finishing instead marks existing scene-owned targets for completion and installs no incoming targets. Fade interpolation excludes the paused duration.

The barrier is connection-wide because an LV1 scene recall can overwrite controls owned by any active ASC fade, not only controls associated with the newly recalled scene. Once two qualifying pings release the barrier, all remaining targets resume from their paused progress.

A readiness-only handoff installs no new targets or writes. It can still pause unrelated active targets while ASC waits for LV1 to recover from a queued scene recall. An admitted zero-duration recall first sends its exact targets in one checked write and then participates in readiness when the ASC queue owns a completion.

## Overlapping Scene Fades

Recall serialization does not serialize fade completion. Fade can own active targets from several scene recalls at the same time because every target retains the exact scene identity that created it.

When a different scene is admitted:

- incoming targets replace active targets with the same group, channel, and parameter;
- non-overlapping targets from earlier scenes continue;
- all targets share the connection-wide readiness pause while the new LV1 recall settles.

For a repeated recall of the exact same scene that still owns active targets, the configured same-scene behavior either marks all of those targets for exact completion after readiness, without installing the incoming targets, or restarts matching targets over the full configured duration. If that scene owns no active targets, the incoming targets are installed normally. Neither behavior cancels unrelated targets.

## ASC Recall Interval

The ASC recall interval is an optional minimum wait between successful readiness for one ASC-originated recall and dispatch of the next queued ASC recall.

```text
LV1 recall A -> readiness A succeeds -> configured interval -> LV1 recall B
```

The interval defaults to 0 ms and is normalized to the range 0 through 10,000 ms. At zero, the next FIFO request becomes dispatchable immediately after readiness. A nonzero interval remains active even when the FIFO is empty, so a request admitted during the interval waits for the existing boundary.

The interval provides operator-selected pacing. It can give LV1 additional time between scene loads and lets active fades advance before another scene replaces overlapping targets. It is not required for Fade correctness, does not wait for a fade to finish, and does not apply to a scene recalled directly in LV1.

## Deadlines and Failure Behavior

One absolute five-second safety deadline starts when an ASC recall is dispatched. It spans the exact newer observation, observation processing, Fade admission, and two-ping readiness. The configured ASC recall interval begins only after readiness succeeds; it does not increase the five seconds available for observation or readiness.

Failure is conservative:

- A safety timeout cancels remaining queued recall intent.
- An installed Fade readiness timeout removes all paused targets rather than sending delayed writes into uncertain LV1 state.
- Lockout, disconnect, generation change, session replacement, event-bus lag, and actor shutdown cancel affected runtime recall intent.
- Abort All cancels coordinated recall intent before requesting Fade cancellation.
- A blocked or skipped direct LV1 observation sends no Fade command and does not abort an existing fade. The same policy outcome correlated to an ASC request does not abort immediately, but its required readiness-only handoff can still time out and remove paused targets.
- When a recall is canceled while awaiting its exact observation, a matching late observation is suppressed for a bounded period rather than being treated as an independent recall.

A readiness timeout therefore means ASC could not prove that the expected scene was followed by a healthy post-recall keepalive cadence within five seconds. It does not identify which LV1 subsystem delayed that cadence.

## Timing Summary

| Timing rule | Meaning |
| --- | --- |
| 2 s arming window | Establishes the latest observed scene as the baseline after recall tracking resets. |
| 500 ms scene-list-edit suppression | Blocks observations after an actual scene-list change. |
| Configurable same-scene threshold | Suppresses repeated exact observations below the boundary; defaults to 500 ms. |
| 25 ms observation settle | Allows scene index/name reporting to stabilize before policy evaluation. |
| Up to 2 s fresh-scene retry | Retries returned mismatched or disconnected snapshots, clamped to the safety deadline; request errors return immediately. |
| 5 s recall safety deadline | Bounds exact observation, Fade admission, and two-ping readiness. |
| Two newer keepalive pings | Confirms post-snapshot LV1 control-loop liveness without event loss. |
| 0–10 s ASC recall interval | Optional pacing after readiness and before the next ASC-originated dispatch. |
| 5 s late-observation suppression | Prevents a canceled request's delayed exact observation from triggering independent policy. |
