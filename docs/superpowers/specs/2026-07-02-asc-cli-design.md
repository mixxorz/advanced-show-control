# ASC CLI Design

## Purpose

Add an Advanced Show Control command-line interface that can control Waves eMotion LV1 directly as a power tool. The CLI must avoid per-command discovery and TCP handshake latency by using a local long-running daemon that owns LV1 discovery, the selected target, the active LV1 connection, reconnect behavior, and an authoritative state mirror.

The CLI intentionally does not enforce the Tauri app's show-file, lockout, scene identity, or fade safety policy. Operators who use it are expected to understand that it sends raw LV1 control commands.

## Binaries

The Rust crate will add two binaries:

- `asc`: the user-facing CLI client.
- `ascd`: the headless local daemon.

`asc` parses command-line arguments, sends one request to `ascd`, prints the response, and exits. `ascd` owns the persistent LV1 runtime.

## Daemon Lifecycle

`asc` uses Docker-like daemon behavior:

- A command first tries to connect to the per-user daemon socket.
- If the socket is unavailable, `asc` starts `ascd`, waits for readiness, then retries the request.
- `asc daemon status` reports daemon process and LV1 connection state.
- `asc daemon stop` asks the daemon to shut down cleanly.
- Stale sockets are detected and replaced during startup.

The first implementation is macOS-first. The design should keep IPC isolated behind a small local transport boundary so Windows named pipes can be added later without changing command routing.

## Local IPC

`asc` and `ascd` communicate over a per-user Unix domain socket with JSON request/response messages.

Each request includes:

- Request ID.
- Command name or enum variant.
- Structured arguments.
- Output preference, including human-readable or JSON response mode.

Each response includes:

- Request ID.
- Success or error status.
- Structured result payload when available.
- Machine-readable error code and human-readable message on failure.

The wire protocol is not a public stability promise in the first version, but it should be explicit and test-covered so scripts using `--json` receive predictable output.

## Persistent Files

The daemon stores CLI-specific state outside the Tauri app show/session state. The state directory should live under the platform app config location for the CLI, for example a `com.advancedshowcontrol.cli` namespace on macOS.

Persisted state includes:

- Selected LV1 target host and port.
- Optional discovered LV1 identity metadata when available.
- Daemon preferences needed for reconnect behavior.

Daemon diagnostics are written to CLI-specific logs, separate from app UI logs.

## LV1 Runtime Ownership

`ascd` owns:

- LV1 discovery cache.
- Selected LV1 target.
- Active TCP connection and MyFOH registration.
- Keepalive responses.
- Reconnect loop.
- Authoritative LV1 state mirror.
- Request routing from CLI commands to LV1 OSC writes or state reads.

The daemon should reuse existing LV1 framing, OSC encoding/decoding, discovery, and actor/state code where practical. Shared protocol code should remain in library modules, not duplicated in the binaries.

## Safety Boundary

The ASC CLI is a raw LV1 power tool.

Mutating CLI commands intentionally bypass:

- App lockout.
- Show-file state.
- App-managed scene fade policy.
- Exact scene identity validation.
- Fade generation guards.

This boundary must be documented in command help and user-facing docs so the CLI is not confused with safe app-managed automation.

## Command Surface

The first version includes named commands for every documented client-to-LV1 OSC command in `docs/lv1-osc.md`, plus a raw OSC escape hatch.

Representative commands:

```bash
asc daemon status
asc daemon stop

asc discover
asc connect --host 192.168.1.10 --port 12345
asc connect <discovered-id>
asc status

asc scene recall --index 4
asc track gain --group 0 --channel 1 --db -12
asc track mute --group 0 --channel 1 --on
asc track solo --group 0 --channel 1 --on
asc track pan --group 0 --channel 1 --degrees 10
asc track balance --group 0 --channel 1 --degrees -5
asc track width --group 0 --channel 1 --value 1.0
asc track name --group 0 --channel 1 --name "Lead Vox"

asc aux tracks
asc aux send-on --source-group 0 --source-channel 1 --aux 2 --on
asc aux send-gain --source-group 0 --source-channel 1 --aux 2 --db -6
asc aux send-pan --source-group 0 --source-channel 1 --aux 2 --pan 0

asc user-key set --index 3 --on
asc spill set --bank 0 --slot 2 --state 1
asc mute-group set --index 1 --on
asc tempo tap

asc osc send /Set/Track/Out/Gain i:0 i:1 d:-12
```

The raw `osc send` command accepts OSC arguments using the existing documentation notation: `i:`, `h:`, `f:`, `d:`, `s:`, `T`, and `F`.

## Read Commands

Read commands are first-class and return authoritative daemon-mirrored LV1 state only. The daemon must not answer reads from a last-written-value cache.

Representative reads:

```bash
asc get scenes
asc get current-scene
asc get channels
asc get track --group 0 --channel 1
asc get track gain --group 0 --channel 1
asc get track mute --group 0 --channel 1
asc get track solo --group 0 --channel 1
asc get track pan --group 0 --channel 1
asc get track balance --group 0 --channel 1
asc get track width --group 0 --channel 1
asc get aux tracks
asc get aux send-on --source-group 0 --source-channel 1 --aux 2
asc get aux send-gain --source-group 0 --source-channel 1 --aux 2
asc get user-keys
asc get spill --bank 0 --slot 2
asc get mute-groups
asc get tempo
asc state --json
asc watch
asc watch --json
```

If LV1 has not published a value, or the project has not confirmed a notification mapping, the read command returns an explicit unsupported or unavailable error. It must not infer state from previous CLI writes.

Aux send pan is writeable through the documented set command, but read support remains unavailable until authoritative LV1 notification behavior is confirmed.

## State Mirror Additions

The existing LV1 mirror already tracks channels, scene list, current scene, output fader gain, output mute, pan, balance, and width. The CLI work extends the mirror for documented authoritative LV1 notifications that are currently not fully modeled.

Add mirror support for:

- Aux tracks from `/Aux/Tracks`.
- Aux send on/off from `/Notify/Aux/Send/On`.
- Aux send gain from `/Notify/Aux/Send/Gain`.
- Solo from `/Notify/Solo` if not already queryable.
- User key info from `/Notify/UserKeyInfo`.
- Spill button state from `/Notify/SpillButton`.
- Mute group state from `/Notify/MuteGroup`.
- Internal assign state from `/Notify/InternalAssign`.
- Tempo from `/Notify/Tempo`.
- Layers and current layer from `/Notify/Layers` and `/Notify/CurrentLayer`.
- Diagnostic metadata from documented authoritative notifications, including service version, current session ID, track color, and number of channels.

State updates should remain owned by LV1 state-handling modules. The CLI daemon reads snapshots or subscribes to facts; it should not maintain a separate competing state model.

## Error Handling

Errors returned to `asc` should be concise for humans and structured for scripts.

Examples:

- `daemon_unavailable`: daemon could not be started or contacted.
- `lv1_not_connected`: daemon is running but LV1 is disconnected.
- `target_not_selected`: no LV1 target has been selected and the command requires one.
- `unsupported_read`: the requested value has no confirmed authoritative LV1 notification mapping.
- `state_unavailable`: the mapping exists, but LV1 has not published the value in the current connection.
- `osc_encode_failed`: raw OSC arguments could not be encoded.
- `command_failed`: the daemon could not send the command to LV1.

Human-readable output should be stable enough for operators. Scripts should use `--json`.

## Testing Strategy

Rust tests should use the repository's allowed categories:

- Pure unit tests for CLI parsing, OSC argument parsing, request and response serialization, command-to-OSC mapping, response formatting, state query behavior, and snapshot serialization.
- Actor-style tests for daemon request routing, daemon state queries, unavailable authoritative reads, and interactions through fake or in-memory command sinks.
- Smoke tests through a real hardware CLI smoke path.

Do not test side-effecting actor behavior by mutating private actor internals directly.

## Required Hardware Verification

Completion requires real-hardware verification against the currently available disposable LV1 target. This is not optional.

Add a CLI hardware smoke target, such as `make cli-smoke` or `make smoke-cli`, that validates the built `asc` and `ascd` binaries end to end.

The hardware smoke must verify:

- `asc discover` finds the LV1 target.
- `asc connect ...` persists the selected target.
- A later command reuses the running daemon connection without rediscovery.
- `asc status` reports connected state.
- Read commands return authoritative mirrored state.
- A named write command changes LV1 state.
- The matching read command confirms the changed authoritative state after LV1 publishes it.
- `asc osc send ...` works for at least one simple set command.
- `asc daemon stop` stops the daemon.
- A later command auto-starts the daemon and reconnects from persisted state.
- A daemon restart reuses the persisted target.

The disposable target may be mutated during verification. Where practical, the smoke should use a known channel and restore simple values after assertions, but the target does not need production-safe preservation.

## Acceptance Criteria

The ASC CLI feature is complete when:

- `asc` and `ascd` binaries exist in the Rust crate.
- `asc` auto-starts `ascd` on demand.
- The daemon persists and reuses the selected LV1 target.
- Commands avoid rediscovery and TCP registration latency when the daemon is already connected.
- Named write commands cover documented client-to-LV1 OSC commands.
- Raw `asc osc send` supports arbitrary OSC messages using documented argument notation.
- Read commands expose every documented authoritative LV1 state value implemented in the mirror.
- Unsupported or unavailable reads fail explicitly rather than returning inferred state.
- The daemon and CLI have targeted unit and actor-style tests.
- Real-hardware CLI smoke verification passes against the disposable LV1 target.
- Documentation clearly states that the CLI is a raw LV1 power tool and does not enforce app safety policy.
