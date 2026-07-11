# Post-Recall Ping Gate Design

## Context

On real LV1 hardware, scene recall can lag while the fade engine continues sending timed parameter writes. Those writes can run ahead of the console state visible to the operator.

This design addresses GitHub issue #35 by treating LV1 keepalive pings as a connection-wide readiness signal. The working hardware assumption is that real LV1 hardware pauses its normal ping cadence while scene recall processing prevents it from accepting timely parameter writes. The available hardware simulation does not reproduce recall lag and therefore cannot confirm that assumption: its approximately 200 ms ping cadence continued normally throughout repeated smoke-test recalls.

The assumption must be verified on real lagging hardware when available. Until then, the gate is a conservative two-ping post-recall delay tied to observed LV1 activity rather than a claim that ping proves scene application completed.

## Design

The fade engine observes one generation-wide readiness barrier. Every validated LV1 scene recall resets the barrier and pauses all active and newly created timed fade targets before further parameter writes. The barrier records:

- The active runtime generation.
- The latest scene recall identity.
- The latest observed ping sequence at the recall boundary.
- A count of post-recall pings, initially zero.
- A five-second readiness deadline measured from the latest validated recall.

The gate opens after two pings from the same runtime generation are observed after the latest recall boundary. A ping that preceded the recall cannot contribute. If another validated scene recall arrives before release, the barrier resets to zero post-recall pings and restarts its deadline so readiness is always measured from the most recent recall.

When the second ping arrives, the engine rebases every paused active target by the time spent waiting. Existing fades continue from their pre-pause progress, and newly created targets begin from their fresh live starting values. Waiting does not consume any target's configured remaining duration.

The barrier applies to timed fade-engine parameter writes caused by scene recall. Zero-duration recalls continue to follow existing scene-recall policy and do not create timed fade targets.

## Ownership And Data Flow

The readiness barrier remains private fade-engine state. Scene recall continues to validate the request and send `FadeCommand::RecallSceneFade` through the existing actor boundary.

The LV1 actor already receives `/ping` and returns `/pong`. It will additionally publish a lightweight generation-tagged ping-received fact through `AppEventBus` after accepting each valid ping. The fact carries a monotonically increasing connection-local sequence or equivalent ordering value. It is an operational fact for runtime consumers, not frontend state, and does not produce a user-facing log for normal ping traffic.

The flow is:

1. Scene recall validates fresh LV1 state, exact scene identity, lockout, topology, and targets.
2. The fade actor applies existing scene-owned overlap rules to build, replace, or finish targets.
3. Before any resulting write, the fade actor records or resets the generation-wide barrier and pauses all active targets.
4. The LV1 actor publishes incoming ping facts while continuing to send the required pong replies.
5. The fade actor counts only same-generation pings later than the latest recall boundary.
6. The second qualifying ping releases the barrier.
7. The fade actor rebases all paused targets and resumes normal ticks.

This keeps readiness policy with the component that owns fade timing. The LV1 actor exposes only the keepalive fact it directly observes; it does not decide whether a fade is safe to resume.

## Overlap And Repeated Recall

Scene-owned parameter overlap remains unchanged beneath the global pause:

- A different scene takes ownership only of overlapping parameter targets. Unrelated targets retain their existing scene ownership.
- Existing and incoming targets all pause because LV1 readiness is connection-wide.
- On release, retained targets continue from their previous progress and incoming targets start from fresh live values.
- Two recalls before release do not create competing gates. The one shared barrier resets from the latest recall, and all current targets wait for two later pings.
- Abort All and disconnect remain global.

This model has no parameter-specific acknowledgement locks, so concurrent scene fades cannot release or block one another through reuse of the same gain, pan, balance, or width key.

## Safety And Failure Handling

A stale-generation ping does not advance or release the gate. Runtime generation remains validated before every later parameter write. Normal pings continue to refresh existing connection liveness behavior independently of the fade barrier.

Abort All, disconnect, unsafe or unavailable LV1 state, generation change, and actor shutdown clear the barrier and prevent further writes. Existing scene-recall validation remains before target ownership or the barrier changes, so blocked, skipped, or disabled recalls do not pause, abort, replace, or finish existing fades.

If two qualifying pings do not arrive within five seconds of the latest validated recall, the engine aborts all paused fades and sends no deferred or subsequent parameter writes. It emits one user-facing `WARN` with a stable event field and a complete message explaining that fades were aborted because LV1 did not resume its keepalive cadence after scene recall. Scene identity, generation, observed ping count, and timeout are diagnostic fields. Barrier start, reset, individual ping progress, and successful release remain `DEBUG` diagnostics to avoid frontend log noise.

Subscriber lag never implies readiness. If the fade actor misses required ping facts, it remains gated and fails safely at the deadline.

## Testing

Use actor tests through the fade mailbox, fake LV1 mailbox, `AppEventBus`, and a tracing listener when asserting the timeout warning. Use LV1 actor tests to prove ping facts are published without changing pong behavior. Do not inspect or mutate actor internals.

Cover these behaviors:

- A validated scene recall pauses active and incoming targets before further writes.
- One post-recall ping does not release the barrier; the second same-generation ping does.
- Pings observed before the recall boundary and stale-generation pings do not count.
- A second validated recall resets progress, requiring two pings after the newer recall.
- Waiting time does not consume existing or incoming fade duration.
- Existing unrelated fades pause and resume without losing ownership or progress.
- Incoming recalls still replace only overlapping parameter targets.
- Five seconds without two qualifying pings aborts all paused fades, prevents later writes, and emits one visible warning.
- Disconnect, generation change, Abort All, and manual override remain safe while waiting.
- Blocked, skipped, or disabled recalls do not alter an existing barrier or active fades.
- The LV1 actor publishes a ping fact and still sends a matching pong for each incoming ping.
- Existing zero-duration recall policy remains unchanged.

Run targeted fade and LV1 actor tests during development, then the broader Rust verification required by the repository before merge. Repeat the timestamped recall-versus-ping capture on real hardware that exhibits recall lag before treating the hardware assumption as confirmed.

GitHub issue #38 tracks a reusable tracing assertion helper to reduce repeated setup in logging-sensitive actor tests. That infrastructure is follow-up work and is not required before implementing this safety fix.

GitHub issue #42 tracks the pre-existing gap between documented same-scene finish behavior and the current implementation. The ping gate preserves current target replacement behavior; adding scene ownership and same-scene exact final writes is outside this change.

## Non-Goals

- Adding parameter-write acknowledgement or echo correlation.
- Treating simulator ping behavior as proof of real-hardware behavior.
- Adding a user-configurable timeout.
- Changing scene recall validation or scene identity matching.
- Implementing scene-owned same-scene finish behavior tracked by #42.
- Changing manual-override tolerance or policy.
- Changing the frontend projection contract.
