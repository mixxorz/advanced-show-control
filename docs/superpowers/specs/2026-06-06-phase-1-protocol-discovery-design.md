# Phase 1 Protocol Discovery Design

## Purpose

Phase 1 builds a Rust protocol discovery prototype for Waves eMotion LV1. Its job is to verify that this app can reliably discover LV1, connect to it, complete the MyFOH-style OSC-over-TCP registration flow, keep the connection alive, observe scene and fader messages, and send a controlled single fader gain command.

The prototype is not the full fade app. It is a protocol probe whose output will decide the safe shape of later phases.

## Scope

Included:

- Custom Waves `/zDNS` network discovery.
- Minimal OSC 1.0 encode/decode support for LV1 messages.
- LV1 OSC-over-TCP framing and parsing.
- MyFOH-style handshake and `/device_name` registration.
- `/ping` to `/pong` keepalive handling.
- Raw and decoded protocol logging.
- Scene and fader message highlighting.
- One-shot `/Set/Track/Out/Gain` test command.
- Automated tests for OSC, discovery parsing, and TCP framing.

Excluded:

- Fade scheduling.
- Capture/listen mode for saved fade targets.
- Project files or persistent fade configs.
- Desktop UI.
- Stream Deck or Companion API.
- Automatic scene-triggered fades.
- Bulk, repeated, or animated fader sends.

## Architecture

The crate will contain reusable protocol modules plus a CLI probe front-end. The protocol modules are the primary artifact; the CLI exists to exercise them against LV1.

### `osc`

`osc` owns pure OSC packet encoding and decoding. It has no LV1 networking knowledge.

Supported argument types for Phase 1:

- `i`: int32.
- `f`: float32.
- `h`: int64.
- `d`: float64.
- `s`: string.
- `b`: blob.
- `T`: true.
- `F`: false.
- `N`: nil.
- `I`: impulse.

The decoder should be best-effort but defensive. Unknown tags and malformed input should return errors or partial decode results where practical without crashing the process.

### `lv1::discovery`

`lv1::discovery` implements the custom Waves discovery behavior observed in the Companion module.

It listens for OSC `/zDNS` packets on UDP multicast `225.1.1.1:13337`, joins the multicast group on available non-internal IPv4 interfaces, parses LV1 service announcements, and returns discovered targets.

Each discovery result includes:

- Service name, such as `_waveslv113._tcp`.
- LV1 UUID when present.
- Hostname when present.
- TCP port.
- Advertised IPv4 addresses, ranked best-first.
- Advertised IPv6 addresses.
- Source IP that sent the multicast packet.

Address ranking should prefer likely studio LAN addresses over loopback, APIPA, virtual adapter, Docker, WSL, and host-only adapter addresses.

### `lv1::tcp`

`lv1::tcp` owns the LV1 TCP protocol details:

- TCP connection lifecycle.
- Frame encoding and decoding.
- MyFOH registration.
- Keepalive responses.
- Send API for OSC messages.
- Receive loop that emits raw frame metadata and decoded OSC messages.

Frame format:

```text
[4-byte big-endian OSC payload length][8-byte LV1 header][OSC payload]
```

The length field describes only the OSC payload size. The default outgoing LV1 header is:

```text
00 00 00 02 00 00 00 00
```

The MyFOH registration flow sends `/handshake` and `/device_name` in one TCP write, then waits for `/handshake i:1` before considering the session registered.

### `lv1::probe`

`lv1::probe` coordinates Phase 1 behavior. It classifies messages, writes logs, and exposes the events needed to answer the protocol discovery questions in `PHASES.md`.

It should classify at least:

- Scene-related messages.
- Fader gain notifications, especially `/Notify/Track/Out/Gain`.
- Handshake messages.
- Keepalive messages.
- Decode errors.
- Connection events.
- Sent commands.

This module is not the fade engine and should not own fade state.

## CLI Commands

The CLI binary is a thin front-end over the protocol modules.

### `discover`

Runs zDNS discovery for a bounded timeout and prints discovered LV1 instances.

Useful options:

- `--timeout-ms <ms>`.
- `--filter-host <ip>`.
- `--json` for machine-readable output.

### `listen`

Discovers or connects to an explicit target, completes registration, responds to keepalive pings, logs all traffic, and highlights scene/fader messages.

Useful options:

- `--host <ip>`.
- `--port <port>`.
- `--timeout-ms <ms>` for discovery.
- `--log-dir <path>`.
- `--json` for structured console output.
- `--raw` to include raw frame bytes in console output in addition to log files.

If no host or port is provided, `listen` uses discovery. If host is provided without a port, discovery may be filtered to that host to find the current LV1 port.

### `set-gain`

Connects to LV1, completes registration, sends exactly one `/Set/Track/Out/Gain` command, logs the send, and exits after a short wait for any echo or related notification.

Required options:

- `--group <number>`.
- `--channel <number>`.
- `--gain-db <number>`.

Target selection options:

- `--host <ip>`.
- `--port <port>`.

If target options are missing, discovery is used the same way as `listen`.

## Data Flow

### Discovery Flow

The CLI opens a UDP socket on `225.1.1.1:13337`, joins multicast membership on usable IPv4 interfaces, receives `/zDNS` OSC packets, decodes service metadata, ranks advertised IPv4 addresses, and returns candidate LV1 targets.

### Connection Flow

The CLI selects a discovered target or uses an explicit host/port. `lv1::tcp` opens TCP, sends the MyFOH batched `/handshake` and `/device_name` messages, waits for `/handshake i:1`, then marks the session registered. Incoming `/ping h:<clock> i:<seq>` messages are answered with `/pong` using the same arguments.

### Receive And Logging Flow

Every received TCP frame is split into frame metadata, the 8-byte LV1 header, raw OSC bytes, and a best-effort decoded OSC message. The probe writes structured logs with timestamps, direction, address, args, frame size, header bytes, and decode errors. Scene and fader messages are additionally highlighted in console output.

### Send Flow

Passive `listen` never sends gain changes. A gain send happens only through `set-gain` with explicit group, channel, and gain dB arguments. Sent frames are logged the same way as received frames.

## Safety And Failure Handling

The default CLI mode is observe-only. Discovery and `listen` do not send gain changes. The only automatic sends are keepalive `/pong` responses and registration messages required to receive useful state.

`set-gain` sends exactly one `/Set/Track/Out/Gain` command. It does not require a separate confirmation prompt or flag. It does not support bulk sends, fades, repeated sends, or automatic retries in Phase 1.

If handshake fails, the tool remains disconnected or unregistered and does not allow gain sends. If TCP disconnects, the receive loop stops and logs the disconnect. Automatic reconnect is optional in Phase 1, but automatic re-sending of gain commands is not allowed.

Frame parsing is defensive. Invalid lengths, malformed OSC packets, unknown OSC tags, and decode errors are logged without panicking where possible. Raw bytes are preserved so protocol questions can still be answered when decode support is incomplete.

## Logging

Logs are a Phase 1 deliverable. A run should produce files that can be attached to protocol notes and reviewed after hardware tests.

Each log entry should include:

- Timestamp.
- Direction: received, sent, event, or error.
- Connection target when known.
- Frame size when applicable.
- LV1 header bytes when applicable.
- OSC address when decoded.
- OSC arguments when decoded.
- Raw OSC bytes or a raw byte reference.
- Decode or protocol error details when applicable.

The log format should be line-oriented JSON so long captures can be searched and processed later.

## Testing

### Unit Tests

`osc` should have encode/decode round-trip tests for integers, floats, doubles, int64 values, strings, blobs, booleans, nil, impulse, alignment padding, and malformed packet handling.

### Framing Tests

`lv1::tcp` should have tests for building frames, splitting partial TCP reads into complete frames, rejecting impossible lengths, and preserving the 8-byte LV1 header.

### Discovery Tests

`lv1::discovery` should have parser and IP ranking tests using synthetic `/zDNS` packets based on the Companion module behavior.

### Hardware Validation

With eMotion LV1 running, use the CLI to capture logs proving:

- Discovery finds the LV1 target.
- Handshake succeeds.
- Ping/pong keeps the session alive.
- Scene-related messages are visible during scene recall.
- Fader-related messages are visible during available fader movement paths.
- A safe test `/Set/Track/Out/Gain` command works on a chosen non-critical channel.
- Any echo or notification generated by app-sent gain changes is captured.

## Exit Criteria

Phase 1 is complete when:

- The repo contains reusable Rust OSC and LV1 protocol modules.
- The CLI supports `discover`, `listen`, and `set-gain`.
- Automated tests for OSC, discovery parsing, and TCP framing pass.
- eMotion LV1 logs are captured for discovery, handshake, scene messages, fader messages, keepalive, and one safe gain send.
- The captured logs and notes are sufficient to answer the Phase 1 questions from `PHASES.md`.

## Reference Implementation Notes

The Rust implementation should use `../companion-module-waves-lv1` as the reference for observed protocol behavior, especially:

- `src/osc.ts` for minimal OSC encoding and decoding.
- `src/osc-tcp.ts` for LV1 frame format, MyFOH handshake, and ping/pong behavior.
- `src/zdns-discover.ts` for custom Waves zDNS discovery.

The Rust code should not blindly copy TypeScript structure. It should preserve protocol behavior while using Rust types, errors, and tests that make the modules safe to reuse in later phases.
