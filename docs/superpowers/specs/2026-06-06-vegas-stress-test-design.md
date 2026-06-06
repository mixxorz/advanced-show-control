# Vegas Stress Test Design

## Purpose

Add `lv1-probe vegas`, a whole-console LV1 fader stress test. The command continuously drives every known LV1 fader through a deterministic sine-wave pattern for hardware/protocol stress testing.

This command is intentionally not a show workflow feature. It is a diagnostic/stress command, so it should prioritize predictable behavior, repeatability, and cleanup safety.

## CLI

```text
lv1-probe vegas [--host <host>] [--port <port>] [--timeout-ms <ms>]
```

There is no `--group`, range, or count option. `vegas` operates on every fader present in the LV1 channel snapshot.

## Runtime Flow

1. Resolve and connect to LV1 using the same discovery path as `monitor` and `fade-test`.
2. Spawn the existing LV1 state actor.
3. Wait for connection and the initial channel list.
4. Snapshot all known channels, including each channel's group, channel, gain, and mute state.
5. Mute every captured channel before moving any faders.
6. Continuously send gain updates at a fixed tick rate until Ctrl-C.
7. On shutdown, restore each captured channel's original gain and mute state before exiting.

If the snapshot contains no channels, the command should fail with a clear message instead of running an empty loop.

## Deterministic Wave Function

The fader position for a given tick is calculated by a pure function from only:

- group
- channel
- tick index
- animation constants

The function should not depend on snapshot ordering, wall-clock timing jitter, random values, or mutable animation state other than the tick counter. This makes the pattern repeatable and easy to unit test.

The wave is 8 faders wide. For each channel, derive a stable fader index directly from `(group, channel)` and calculate:

```text
stable_index = group * 128 + channel
phase = (stable_index / 8.0) * TAU + tick * phase_step
normalized = (sin(phase) + 1.0) / 2.0
gain_db = -40.0 + normalized * 40.0
```

The normalized value maps to `-40.0..0.0 dB`. This is wide enough to visibly stress the faders while avoiding LV1's deepest and loudest extremes.

## Channel Ordering And Indexing

The animation must be deterministic across runs for the same LV1 topology. Use the direct `group * 128 + channel` index, not the order channels happen to arrive in the `/Channels` message.

The `128` group stride keeps all known LV1 channel numbers from overlapping between groups and is divisible by 8, so each group starts on the same sine-wave phase.

## Mute Handling

`vegas` must record and restore original mute states. If the existing LV1 state model does not expose mute state yet, implementation must first add mute tracking for the relevant LV1 notify/set messages before adding the command.

Shutdown restore order:

1. Restore original fader gains.
2. Restore original mute states.

This keeps channels muted while faders return to their pre-test positions.

## Error Handling

- If LV1 discovery or connection fails, return the existing connection error.
- If initial channel state does not arrive before timeout, fail clearly.
- If Ctrl-C is received, always attempt cleanup.
- If cleanup partially fails, report the failure and continue attempting to restore remaining channels.

## Testing

Unit tests should cover the pure wave function:

- Same inputs return the same gain.
- Tick advancement changes phase.
- Faders 8 positions apart share the same phase at the same tick.
- Output remains inside the configured dB range.

CLI parsing tests should cover `vegas` with host, port, and timeout options, and confirm there is no group option.

If mute state support is added, state parsing tests should cover mute notifications and snapshot storage.
