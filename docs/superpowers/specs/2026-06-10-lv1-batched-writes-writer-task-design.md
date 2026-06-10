# LV1 Batched Writes And Writer Task Design

## Purpose

Optimize outbound LV1 parameter writes from `FadeEngine` to `Lv1Actor` without weakening disconnect, manual override, abort, or generation-guard safety behavior.

The current fade tick path sends one acknowledged command per target. Each write can take an app-command lock, send one actor command, consume one actor loop turn, and await a TCP `write_all` inside `Lv1Actor`'s connected select loop. If the socket stalls, that write can block the actor from processing reads, pings, pongs, commands, and disconnect-related progress.

This design makes two changes:

- Batch all fade-generated parameter writes for a tick into one actor command and one contiguous TCP buffer.
- Move actual socket writes into a per-connection writer task so the actor select loop never awaits socket writability.

## Scope

In scope:

- Enable `TCP_NODELAY` on the LV1 TCP stream.
- Add a fire-and-forget batch write command for all fade-controlled parameters: fader gain, pan, balance, and width.
- Keep existing single-value acknowledged commands for non-engine callers.
- Batch duration `0` scene writes and normal timed tick writes.
- Encode a batch into one contiguous byte buffer before enqueueing it to the writer.
- Add a bounded per-connection writer channel and treat full writer queues as a degraded TCP connection.
- Preserve existing disconnect fan-out so active fades abort through the current `Lv1Event::Disconnected` path.

Out of scope:

- Changing fade timing, curves, thresholds, or manual override semantics.
- Changing shell/manual command acknowledgement behavior.
- Adding retry, stale-write coalescing, or priority queues.
- Changing scene recall validation or generation-guard policy.

## Architecture

### Batch Command

Add a small LV1 write value type, for example:

```rust
pub struct Lv1ParameterWrite {
    pub group: i32,
    pub channel: i32,
    pub parameter: Lv1WriteParameter,
    pub value: f64,
}
```

`Lv1WriteParameter` should be owned by the LV1 command boundary, with variants for fader gain, pan, balance, and width. `FadeEngine` maps from `FadeParameter` before sending the batch. This keeps `lv1` independent from the `fade` module while still letting the batch command cover every fade-controlled parameter.

Add `Lv1Command::WriteBatch(Vec<Lv1ParameterWrite>)` with no reply channel. Empty batches should be ignored before or inside the actor.

Add:

- `Lv1ActorHandle::write_batch(Vec<Lv1ParameterWrite>) -> Result<(), Lv1ActorError>` for enqueueing the command to the actor.
- `AppCommandBus::write_batch(Vec<Lv1ParameterWrite>) -> Result<(), AppCommandError>` for the fade engine.

The result only reflects whether the command could be queued to the actor. It does not acknowledge socket write success. Socket failures continue to be reported by the actor disconnect path.

Existing single-value commands remain unchanged:

- `SetGain`
- `SetPan`
- `SetBalance`
- `SetWidth`
- `SetMute`

These keep their reply channels for shell/manual callers that benefit from acknowledged command semantics.

### FadeEngine Data Flow

For duration `0` recalls:

1. Build one batch from all targets in the recall config.
2. Send the batch once through `AppCommandBus`.
3. Apply the same in-memory completion and event behavior as today for each target.

For normal ticks:

1. Iterate active channels and compute due sends as today.
2. Push each due parameter write into a `Vec<Lv1ParameterWrite>`.
3. Preserve completion bookkeeping and events as today.
4. After iterating all channels, send one batch if the vector is not empty.

This changes outbound command granularity only. Fade ownership, overlap handling, exact final sends, and manual override handling stay in `FadeEngine`.

### TCP Encoding

Add a helper that appends one encoded frame per parameter write into a single `Vec<u8>`. Each write maps to the existing OSC paths:

- `FaderDb` -> `/Set/Track/Out/Gain`
- `Pan` -> `/Set/Track/Pan`
- `Balance` -> `/Set/Track/Pan/Balance`
- `Width` -> `/Set/Track/Pan/Width`

The actor handles a batch by encoding all frames into one contiguous buffer and enqueueing that buffer to the writer task. A single `write_all` in the writer task then sends the whole tick's writes together.

### TCP_NODELAY

`Lv1TcpClient::connect` calls `stream.set_nodelay(true)?` immediately after `TcpStream::connect` succeeds and before `into_split()`.

This benefits both the existing command path and the new batch path.

## Writer Task

After the MyFOH handshake succeeds, `run_connected` creates a per-connection writer task that owns the `OwnedWriteHalf`.

The actor keeps an `mpsc::Sender<Vec<u8>>` or a small `WriterMessage` wrapper. The minimal version can use `Vec<u8>` and rely on sender drop for shutdown.

The writer task loop:

1. Receives encoded byte buffers from the bounded channel.
2. Calls `write_all` on the owned write half.
3. Reports the first write error back to `run_connected` through a small error channel.
4. Exits when all senders are dropped or a write fails.

`run_connected` selects on:

- inbound reads,
- actor commands,
- writer error notification.

If the writer reports an error, `run_connected` returns `DisconnectReason::TcpError`.

### Actor Command Handling

For `WriteBatch` and pong replies, the actor encodes bytes and calls `try_send` on the writer channel. It must not await socket writability.

If `try_send` fails because the channel is full, the actor treats the connection as degraded and returns `DisconnectReason::TcpError`.

If `try_send` fails because the writer task has gone away, the actor also returns `DisconnectReason::TcpError`.

For existing acknowledged single-value commands, the actor can either:

- encode and `try_send`, then reply `Ok(())` when the buffer is accepted by the writer channel, or
- reply with `CommandSendFailed` if enqueueing fails.

The acknowledgement means "accepted by the live actor for outbound writing," not "LV1 confirmed application," which matches the existing practical semantics.

### Ping/Pong Handling

Pongs use the same writer channel as all other outbound bytes. This preserves TCP ordering and prevents the read arm from awaiting socket writes.

The writer channel should have enough capacity that pongs are not dropped during normal fade activity. A queue around `64` buffers is sufficient for the current design because the fade engine sends at most one buffer per tick, while ticks run at 25 Hz. A full queue means the writer has not kept up for multiple ticks, so disconnecting is safer than queueing stale fader values.

No priority channel is added in this design. If future hardware testing shows false ping timeouts under healthy links, add a focused priority or reserved-headroom mechanism then.

## Lifecycle And Safety

The writer task is scoped to one successful connection. It is created after handshake and dropped when `run_connected` exits.

Disconnect causes remain visible through the existing actor lifecycle:

- read error,
- ping timeout,
- writer task error,
- writer queue full,
- command channel closed.

For TCP-related failures, the outer actor loop clears mirrored state and fans out `Lv1Event::Disconnected`. `FadeEngine` already listens for that event, cancels active channels, stops the tick interval, and emits `FadeAborted`.

The writer task does not hold app state, command targets, or generation information. Existing runtime generation guards remain responsible for stale higher-level runtime handles during disconnect/reconnect.

## Testing

Add or update tests for:

- `Lv1TcpClient::connect` sets `TCP_NODELAY` where practical, or isolate this through a helper if direct socket option testing is awkward.
- Batch encoding maps each parameter to the expected OSC address and arguments.
- `Lv1ActorHandle::write_batch` sends a no-reply command.
- `AppCommandBus::write_batch` locks command targets once and publishes failure only when enqueueing fails.
- `FadeEngine` duration `0` recall sends one batch for multiple targets.
- `FadeEngine` tick sends one batch for multiple due parameter writes.
- Existing single-value command tests still pass.
- Actor treats writer queue full as a TCP disconnect.
- Actor routes ping pongs through the writer channel without awaiting socket writes.

Use targeted Rust checks during implementation, then run the relevant workspace verification before claiming completion.

## Rollout Sequence

1. Add `TCP_NODELAY`.
2. Add batch command plumbing and fade-engine batching while the actor still writes directly.
3. Add the writer task and route all outbound actor writes through it.
4. Update architecture docs if command-bus semantics or actor lifecycle documentation changes.

This sequence keeps each behavioral change small and independently testable.
