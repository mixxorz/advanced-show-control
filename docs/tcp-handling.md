# LV1 TCP Handling

`Lv1Actor` owns one generation's LV1 TCP transport and mirrored state. It owns connection attempts, reconnect delay, read loop, ping timeout, disconnect facts, and the socket writer. The frontend does not participate in transport reconnection; it only requests lifecycle-level connect or disconnect.

## Framing

LV1 OSC messages use TCP frames:

```text
[4-byte big-endian payload length][8-byte LV1 header][OSC payload]
```

App-sent frames use the LV1 header `00 00 00 02 00 00 00 00`. Encoding and decoding are in `src-tauri/src/lv1/tcp.rs`.

`Lv1TcpClient::connect` enables `TCP_NODELAY` before splitting the stream because fader writes are latency-sensitive.

## Write Path

Outbound fade writes flow directly through:

```text
FadeEngine -> Lv1Connection -> LV1 mailbox (WriteBatch) -> bounded writer channel -> writer task -> socket
```

There is no `AppCommandBus` in this path. Fade constructs an `Lv1Connection` from its fixed LV1 handle and generation. The client waits for mailbox capacity, then checks the generation and admits the command under the same guard; no guard is held while waiting. This protects admission, not commands or bytes already accepted. The LV1 actor encodes each `WriteBatch` into one byte buffer and uses `try_send` to enqueue it. The writer task exclusively owns the TCP write half and writes queued buffers with `write_all`.

The read loop routes `/ping` replies through that same writer queue, preserving TCP ordering without blocking reads.

## Backpressure and Flush

The writer queue is bounded. A full or closed queue, writer error, read error, or ping timeout ends the connected loop. The actor clears connection-dependent live state, publishes a generation-tagged `Disconnected` fact with a reason, and retries transport connection unless its command channel has closed. Fade disconnect handling aborts active fades.

`Lv1Command::Flush` normally enqueues a flush marker. Its reply succeeds only after all preceding queued bytes have been written and `flush()` succeeds. Writer failure completes pending flush replies with `CommandSendFailed` before the task exits.

Two disconnected-command drain contexts intentionally differ. During a reconnect delay, `Flush` replies `NotConnected` because no transport can accept it. After TCP connection but before full connected-loop initialization, stale commands are drained with `Flush` replying success because the actor is about to enter connected mode. Other direct writes return `NotConnected`; `WriteBatch` remains fire-and-forget and is dropped while disconnected so a new transport interval never receives stale fader values.
