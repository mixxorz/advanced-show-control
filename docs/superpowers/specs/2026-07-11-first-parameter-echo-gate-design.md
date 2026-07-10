# First Parameter Echo Gate Design

## Context

On real LV1 hardware, scene recall can lag while the fade engine continues sending timed parameter writes. Those writes can run ahead of the console state visible to the operator.

This design addresses GitHub issue #35. Although the original issue describes a fader echo, the gate is parameter-agnostic so gain, pan, balance, and width fades receive the same startup protection.

## Design

The fade engine owns a single startup acknowledgement gate for each timed scene-recall fade. After building fade targets from fresh LV1 state, it uses the normal fade scheduler to select the first non-noop parameter write. It sends that write and records:

- The active runtime generation.
- The parameter identity and channel key.
- The exact value sent, compared using the existing parameter-appropriate tolerance.
- A five-second acknowledgement deadline.

The engine sends no subsequent fade writes while the gate is pending. The gate opens only when the LV1 mirror publishes the corresponding parameter event with the expected generation, parameter identity, channel, and value.

When the echo arrives, the engine rebases the complete fade timeline from the acknowledged state. Time spent waiting does not consume the configured fade duration, and every target in a multi-parameter fade resumes together.

Zero-duration fades retain their existing immediate behavior because they do not have a timed progression to gate.

## Ownership And Data Flow

The gate remains private fade-engine state. Scene recall continues to validate the request and send `FadeCommand::RecallSceneFade` through the existing actor boundary. The LV1 actor continues to update its mirror and publish gain, pan, balance, and width facts through `AppEventBus`; it does not acquire write-correlation state or acknowledgement responsibilities.

The flow is:

1. Scene recall validates fresh LV1 state, exact scene identity, lockout, topology, and targets.
2. The fade actor builds or replaces active targets.
3. The normal scheduler produces the first actual parameter write.
4. The fade actor revalidates the runtime generation and sends that write.
5. The fade actor records the pending gate and pauses progression.
6. A matching same-generation LV1 mirror fact releases the gate.
7. The fade actor rebases all active targets and resumes normal ticks.

This keeps acknowledgement policy with the component that owns fade timing and avoids expanding the LV1 actor into a general write-acknowledgement protocol.

## Safety And Failure Handling

A stale-generation event or an event with the wrong parameter, channel, or value does not release the gate. Matching is evaluated before existing manual-override handling so the expected echo is not mistaken for operator movement. Other divergent live changes continue through the existing manual-override policy.

Abort All, disconnect, unsafe or unavailable LV1 state, generation change, and actor shutdown clear the pending gate and prevent further writes. A valid overlapping fade atomically replaces both the affected active targets and any obsolete pending gate so an old echo cannot release replacement work. Existing scene-recall validation remains before fade replacement, so blocked, skipped, or disabled recalls do not abort a running or waiting fade.

If no matching echo arrives within five seconds, the engine aborts the waiting fade and sends no further parameter writes. It emits one user-facing `WARN` with a stable event field and a complete message explaining that fade startup was aborted because LV1 did not acknowledge the first parameter write. Parameter identity, channel, generation, expected value, and timeout are diagnostic fields. The wait and successful acknowledgement remain `DEBUG` diagnostics to avoid frontend log noise.

Subscriber lag never implies acknowledgement. If the fade actor misses the required fact, it remains gated and fails safely at the deadline.

## Testing

Use actor tests through the fade mailbox, fake LV1 mailbox, `AppEventBus`, and a tracing listener when asserting the timeout warning. Do not inspect or mutate actor internals.

Cover these behaviors:

- The first scheduled parameter write is sent and subsequent writes remain blocked before acknowledgement.
- Matching gain, pan, balance, and width echoes release their respective gates.
- An event with the wrong parameter, channel, value, or runtime generation does not release the gate.
- Waiting time does not consume the configured fade duration.
- Five seconds without a matching echo aborts the fade, prevents later writes, and emits one visible warning.
- Disconnect, generation change, Abort All, overlap, and manual override remain safe while waiting.
- An old echo cannot release a gate installed by a replacement fade.
- Existing same-scene and zero-duration behavior remains unchanged.

Run the targeted fade actor tests during development, then the broader Rust verification required by the repository before merge. Real-hardware verification remains valuable because LV1 scene-recall latency motivated the change.

GitHub issue #38 tracks a reusable tracing assertion helper to reduce repeated setup in logging-sensitive actor tests. That infrastructure is follow-up work and is not required before implementing this safety fix.

## Non-Goals

- Adding a general LV1 write-acknowledgement protocol.
- Waiting for every parameter write to be echoed.
- Adding a user-configurable timeout.
- Changing scene recall validation or scene identity matching.
- Changing manual-override tolerance or policy.
- Changing the frontend projection contract.
