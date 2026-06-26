# Explicit LV1 Latency Probe Design

## Purpose

Connection-modal latency should describe an explicit measurement, not the time until the next multicast discovery advertisement arrives. Discovery state should identify LV1 systems and their availability only. A one-shot probe command should measure TCP connect time on demand and return that result to the frontend.

## Scope

This change removes `latencyMs` / `latency_ms` from discovered LV1 system state and adds a frontend-callable Tauri command for a single TCP connect measurement.

In scope:

- Remove latency from `DiscoveredLv1System` in Rust and TypeScript.
- Stop displaying discovery-derived latency in `ConnectionModal`.
- Add a Tauri command, `probe_lv1_tcp_connect_latency`, that accepts an `Lv1SystemIdentity` and optional timeout.
- Return a small result object with `tcpConnectMs` for successful probes.
- Surface the probe result as local modal UI state, not projected backend state.

Out of scope:

- LV1 protocol request/response latency.
- Fader echo latency.
- Persistent diagnostics history.
- Automatic continuous latency polling.

## Backend Design

`DiscoveredLv1System` will contain only `identity` and `status`. Discovery parsing remains responsible for `/zDNS` identity data and does not record elapsed advertisement delay.

The new Tauri command belongs at the UI command adapter layer because it is an acknowledged, stateless diagnostic request. It does not require `Lv1Actor`, `ShowState`, or `AppEventBus` ownership because it does not mutate app state and does not send LV1 protocol data.

Command shape:

```text
probe_lv1_tcp_connect_latency(identity, timeoutMs?) -> { tcpConnectMs }
```

The command will:

- Resolve the target from `identity.address` and `identity.port`.
- Use a bounded timeout, defaulting to 500 ms and clamped to 100-2000 ms.
- Measure from immediately before `TcpStream::connect` begins until it succeeds.
- Immediately drop the stream after connect success.
- Return a frontend-safe error string on timeout or connection failure.

This probe must not send any LV1 protocol messages and must not reuse or interfere with the persistent `Lv1Actor` connection.

## Frontend Design

The connection modal will no longer render latency from `appState.discoveredLv1Systems`.

Each discovered system row will expose a small `Test` button. When clicked, the modal calls `probeLv1TcpConnectLatency(identity)` through the existing app command/service pattern and stores the result locally by system key.

Possible row states:

- Not measured: `Test`
- In flight: `Testing...`
- Success: `TCP <n> ms`
- Failure: `Unavailable` or the concise command error

The row click behavior for selecting/connecting a system must remain unchanged. The probe action should stop event propagation so testing does not connect to the system.

## Error Handling

Probe failures are diagnostic results, not app-wide connection failures. They should not change `AppViewState.connection`, `connectedLv1Identity`, or discovered system status.

The frontend should display probe failure beside the row that was tested. It should not replace the global command error unless the command fails before a row-specific result can be recorded.

Timeouts should produce the message `TCP probe timed out`.

## Testing

Rust tests:

- Pure unit tests for discovery/system mapping without latency.
- Pure unit tests for timeout clamping or probe target validation if implemented as helper functions.
- A focused async test for a successful TCP connect probe can use a local `TcpListener` if it does not inspect private actor state.

Frontend tests:

- `ConnectionModal` no longer expects discovery latency text.
- The modal calls the probe command when the row probe action is clicked.
- A successful probe result appears on the correct row.
- The probe action does not select/connect the system.

Verification:

- Targeted Rust tests for connection/discovery/probe code.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
- Targeted frontend tests plus `npm --prefix ui run typecheck`.

## Safety Notes

The TCP probe creates a short-lived connection to the LV1 TCP port. It must be explicit, bounded, and user-triggered. It should not run continuously while the modal is open. If LV1 or another control surface is sensitive to additional client connections, this limits exposure to a single connect/drop diagnostic action.
