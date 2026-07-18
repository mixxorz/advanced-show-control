# LV1 Production Device Name Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the production LV1 actor register as `Advanced Show Control` while preserving the separate developer probe identity and all existing connection behavior.

**Architecture:** Keep the registration identity private to the production LV1 actor and continue passing caller-selected names through the generic TCP protocol API. Prove the production wiring with an actor test that observes the real registration batch through a local TCP listener.

**Tech Stack:** Rust 2024, Tokio TCP and async I/O, existing OSC frame decoder, `cargo nextest`

## Global Constraints

- The production `/device_name` value is exactly `Advanced Show Control`.
- The developer `lv1-probe` identity remains unchanged.
- Do not change MyFOH handshake framing, UUID generation, connection retries, registration timing, or protocol APIs.
- Do not change LV1 state handling, fader writes, scene recall, lockout, generation guards, or disconnect behavior.
- Use the approved Rust actor-test category: interact through the actor's normal mailbox lifetime and observe its TCP output rather than mutating actor internals.
- Follow TDD: observe the new actor test fail against `lv1-state-mirror` before changing production code.

---

### Task 1: Register The Production Actor With The Application Name

**Files:**
- Modify: `src-tauri/src/lv1/actor.rs:22-24,251-253,566-697`
- Test: `src-tauri/src/lv1/actor.rs:566-697`

**Interfaces:**
- Consumes: `build_actor(host: String, port: u16, event_bus: AppEventBus, generation: u64) -> (Lv1ActorHandle, Lv1ActorTask)`, `FrameDecoder::push(&mut self, bytes: &[u8])`, and `decode_frame_payload(&Lv1Frame)`.
- Produces: private `const PRODUCTION_DEVICE_NAME: &str = "Advanced Show Control"` used only by the production actor registration path.

- [ ] **Step 1: Write the failing actor test**

In `src-tauri/src/lv1/actor.rs`, import Tokio's async read extension in the existing test module and add this test:

```rust
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn production_actor_registers_as_advanced_show_control() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let (handle, task) = build_actor(
        "127.0.0.1".to_string(),
        port,
        AppEventBus::default(),
        0,
    );
    task.spawn();

    let (mut stream, _) = listener.accept().await.unwrap();
    let mut decoder = FrameDecoder::default();
    let mut messages = Vec::new();
    let mut buffer = [0_u8; 1024];

    while messages.len() < 2 {
        let size = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buffer))
            .await
            .expect("production actor did not send its registration batch")
            .unwrap();
        assert!(size > 0, "production actor closed before registration");
        messages.extend(
            decoder
                .push(&buffer[..size])
                .unwrap()
                .into_iter()
                .map(|frame| decode_frame_payload(&frame).unwrap()),
        );
    }

    let device_name = messages
        .iter()
        .find(|message| message.address == "/device_name")
        .expect("registration batch did not include /device_name");
    assert_eq!(
        device_name.args.first(),
        Some(&OscArg::String("Advanced Show Control".to_string()))
    );
    match device_name.args.get(1) {
        Some(OscArg::String(value)) => {
            uuid::Uuid::parse_str(value).expect("registration UUID should be valid");
        }
        value => panic!("registration UUID should be a string, got {value:?}"),
    }

    drop(handle);
}
```

Keep `use tokio::io::AsyncReadExt;` with the existing test imports. This test starts the real actor, observes the protocol boundary, validates both registration arguments, and closes the command mailbox by dropping the handle.

- [ ] **Step 2: Run the new test and verify the intended failure**

Run:

```bash
cargo nextest run -p advanced-show-control production_actor_registers_as_advanced_show_control
```

Expected: FAIL at the first `/device_name` argument comparison because the actor sends `lv1-state-mirror` instead of `Advanced Show Control`. If the test times out or fails before that assertion, fix only the test harness and rerun until it demonstrates the identity mismatch.

- [ ] **Step 3: Add the private production identity constant**

In `src-tauri/src/lv1/actor.rs`, add the constant beside the existing actor constants:

```rust
const PING_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const WRITER_QUEUE_CAPACITY: usize = 64;
const PRODUCTION_DEVICE_NAME: &str = "Advanced Show Control";
```

Replace the current local device-name assignment and registration call:

```rust
let uuid = uuid::Uuid::new_v4().to_string();
if client
    .register_myfoh(PRODUCTION_DEVICE_NAME, &uuid)
    .await
    .is_err()
{
```

Leave the existing registration failure body, reconnect delay, and surrounding loop unchanged. Do not modify `src-tauri/src/lv1/tcp.rs` or any developer probe files.

- [ ] **Step 4: Run the focused actor test**

Run:

```bash
cargo nextest run -p advanced-show-control production_actor_registers_as_advanced_show_control
```

Expected: PASS.

- [ ] **Step 5: Run all LV1 tests**

Run:

```bash
cargo nextest run -p advanced-show-control lv1
```

Expected: all selected LV1 tests pass, including existing generic protocol coverage that uses the `lv1-probe` caller name.

- [ ] **Step 6: Run standard Rust verification**

Run each command and inspect its output:

```bash
make rust-fmt
make rust-lint
make rust-test
make rust-build
```

Expected: the formatting check passes, clippy reports no warnings, all workspace Rust tests pass, and the Rust workspace builds successfully.

- [ ] **Step 7: Review and commit the implementation**

Review only the intended source changes:

```bash
git status --short
git diff -- src-tauri/src/lv1/actor.rs
```

Confirm that the diff contains one private constant, one registration call-site update, and the actor test. Then commit:

```bash
git add src-tauri/src/lv1/actor.rs
git commit -m "chore: identify production LV1 connection"
```

Expected: the commit succeeds without bypassing hooks and does not include unrelated files.

- [ ] **Step 8: Record optional hardware confirmation**

When an LV1-compatible target is available, launch the production app, connect to LV1, and confirm LV1 identifies the client as `Advanced Show Control`. This supplementary check does not replace the deterministic actor test and does not block completion when hardware is unavailable.
