# LV1 TCP Handling

`Lv1Actor` owns the LV1 connection lifecycle and mirrored state. During a connected session, the actor owns the read half and a scoped writer task owns the write half.

## Framing

LV1 OSC messages are carried in TCP frames with this shape:

```text
[4-byte big-endian payload length][8-byte LV1 header][OSC payload]
```

The LV1 header for app-sent frames is `00 00 00 02 00 00 00 00`.

Encoding and decoding live in `src/lv1/tcp.rs`.

## Socket Options

`Lv1TcpClient::connect` enables `TCP_NODELAY` before the stream is split. Fader fades are latency-sensitive, and Nagle delay would hold back small writes that should reach LV1 immediately.

## Write Path

Outbound fader writes flow through:

`FadeEngine -> AppCommandBus -> Lv1Actor -> writer channel -> writer task -> socket`

`FadeEngine` batches due writes into `WriteBatch` commands. `Lv1Actor` encodes those writes into one byte buffer and sends the buffer into the bounded writer channel with `try_send`. The writer task owns the socket write half and writes each queued buffer with `write_all`.

## Ping and Pong

The actor read loop routes incoming `/ping` messages to `/pong` and sends the reply through the same writer channel as all other outbound bytes. That keeps TCP ordering intact and keeps ping handling off the actor's read loop.

## Backpressure

The writer channel is bounded. If the channel is full or closed, the actor treats that as a TCP failure. At that point, queued fader values are considered stale and are not allowed to pile up indefinitely.

## Flush Semantics

`Lv1Command::Flush` queues a flush message for the writer task. The writer task completes the flush reply only after all prior bytes in the writer task have been written and `flush()` has returned. If the writer task encounters an error, it completes any pending flush replies with `Err(Lv1ActorError::CommandSendFailed)` before exiting.

## Failure Handling

TCP read errors, writer errors, ping timeouts, and writer-channel backpressure all end the connected actor loop. The outer actor lifecycle then publishes `Lv1Event::Disconnected`, clears mirrored LV1 state, and the existing `FadeEngine` disconnect path aborts any active fade.
