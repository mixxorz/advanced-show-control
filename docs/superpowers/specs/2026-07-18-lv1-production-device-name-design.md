# LV1 Production Device Name Design

## Goal

Identify production Advanced Show Control connections to LV1 with the user-facing name `Advanced Show Control` instead of the internal `lv1-state-mirror` name.

## Scope

- Define the production registration name next to the LV1 actor connection behavior.
- Pass `Advanced Show Control` to the existing MyFOH registration call.
- Keep the developer `lv1-probe` identity unchanged.
- Add automated coverage of the registration name sent by the production actor.

## Non-Goals

- Do not change MyFOH handshake framing, UUID generation, connection retries, or registration timing.
- Do not rename the crate, application bundle, probe, or protocol APIs.
- Do not change LV1 state handling, fader writes, scene recall, lockout, generation guards, or disconnect behavior.

## Design

`src-tauri/src/lv1/actor.rs` will define a private production device-name constant with the value `Advanced Show Control`. `run_actor` will pass that constant to `Lv1TcpClient::register_myfoh` in place of the current local `lv1-state-mirror` value.

The constant remains private to the production actor. It will not be shared with the protocol layer or developer tooling, because `lv1::tcp` should continue accepting a caller-supplied identity and `lv1-probe` intentionally uses its own name.

No error handling or runtime control flow changes are required. A failed registration continues through the existing reconnect path.

## Testing

Add a Rust actor test using the repository's actor-test style. The test will:

1. Start a local TCP listener that stands in for LV1.
2. Build and spawn the production LV1 actor against that listener.
3. Capture and decode the actor's initial MyFOH registration batch.
4. Assert that the `/device_name` message contains `Advanced Show Control` and a generated UUID.
5. Shut down the actor through its normal mailbox lifetime.

This test exercises the production actor path rather than only testing the generic handshake builder. Existing protocol tests continue proving that caller-supplied names, including `lv1-probe`, are encoded unchanged.

## Verification

Run the targeted LV1 actor tests, followed by Rust formatting and clippy. When an LV1-compatible target is practical, connect the production app and confirm LV1 displays the client as `Advanced Show Control`; hardware verification is supplementary to the deterministic actor test.

## Acceptance Criteria

- A production connection sends `Advanced Show Control` as its `/device_name` value.
- The developer probe retains its existing identity.
- The handshake, UUID, reconnect, and safety behavior remain unchanged.
- Automated coverage verifies the production actor's registration name.

## Related Work

- GitHub issue #39
- `src-tauri/src/lv1/actor.rs`
- `src-tauri/src/lv1/tcp.rs`
- `docs/lv1-osc.md`
