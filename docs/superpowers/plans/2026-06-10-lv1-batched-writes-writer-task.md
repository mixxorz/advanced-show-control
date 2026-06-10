# LV1 Batched Writes And Writer Task Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Batch all fade-generated LV1 parameter writes and move TCP socket writes out of the `Lv1Actor` select loop.

**Architecture:** Add an LV1-owned write-batch command, route `FadeEngine` tick and zero-duration sends through one fire-and-forget batch per tick, and then introduce a per-connection writer task that owns `OwnedWriteHalf`. The actor remains responsible for encoding and connection decisions, but it only `try_send`s pre-encoded byte buffers to the writer channel.

**Tech Stack:** Rust, Tokio, Tauri core crate, OSC-over-TCP helpers in `src/lv1/tcp.rs`, runtime command bus in `src/runtime/commands.rs`, nextest for Rust tests.

---

## File Structure

- Modify `src/lv1/commands.rs`: add `Lv1WriteParameter`, `Lv1ParameterWrite`, and `Lv1Command::WriteBatch`.
- Modify `src/lv1/handle.rs`: add `write_batch` and tests for no-reply command behavior.
- Modify `src/lv1/tcp.rs`: add `TCP_NODELAY`, batch encoding helpers, and writer task support helpers if they are small and transport-specific.
- Modify `src/lv1/actor.rs`: handle `WriteBatch`, route writes through the writer task, route pongs through the writer task, and treat writer backpressure as disconnect.
- Modify `src/runtime/commands.rs`: add `AppCommandBus::write_batch`.
- Modify `src/fade/actor.rs`: collect zero-duration and tick writes into batches.
- Modify `tests/lv1_actor.rs`: add integration coverage for batch writes and pongs, plus actor-level unit coverage for writer-channel enqueue failure.
- Modify `tests/fade_engine.rs`: add integration coverage that multiple fade targets are emitted through one batch command path while preserving final wire frames.
- Create `docs/tcp-handling.md`: document implemented TCP framing, socket options, read/write ownership, backpressure, ping/pong routing, and disconnect safety.
- Modify `docs/architecture.md`: update LV1 actor ownership text to mention the scoped writer task.

## Task 1: Add TCP_NODELAY And Batch Encoding Types

**Files:**
- Modify: `src/lv1/commands.rs`
- Modify: `src/lv1/tcp.rs`
- Test: `src/lv1/tcp.rs`

- [ ] **Step 1: Add failing tests for batch encoding**

Add these tests inside `#[cfg(test)] mod tests` in `src/lv1/tcp.rs`:

```rust
use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};

#[test]
fn encodes_parameter_write_batch_in_order() {
    let bytes = encode_parameter_write_batch(&[
        Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: Lv1WriteParameter::FaderDb,
            value: -12.5,
        },
        Lv1ParameterWrite {
            group: 2,
            channel: 3,
            parameter: Lv1WriteParameter::Pan,
            value: 15.0,
        },
        Lv1ParameterWrite {
            group: 4,
            channel: 5,
            parameter: Lv1WriteParameter::Balance,
            value: -25.0,
        },
        Lv1ParameterWrite {
            group: 6,
            channel: 7,
            parameter: Lv1WriteParameter::Width,
            value: 0.75,
        },
    ])
    .unwrap();

    let mut decoder = FrameDecoder::default();
    let frames = decoder.push(&bytes).unwrap();
    let messages = frames
        .iter()
        .map(decode_frame_payload)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(messages[0].address, "/Set/Track/Out/Gain");
    assert_eq!(messages[0].args, vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(-12.5)]);
    assert_eq!(messages[1].address, "/Set/Track/Pan");
    assert_eq!(messages[1].args, vec![OscArg::Int(2), OscArg::Int(3), OscArg::Double(15.0)]);
    assert_eq!(messages[2].address, "/Set/Track/Pan/Balance");
    assert_eq!(messages[2].args, vec![OscArg::Int(4), OscArg::Int(5), OscArg::Double(-25.0)]);
    assert_eq!(messages[3].address, "/Set/Track/Pan/Width");
    assert_eq!(messages[3].args, vec![OscArg::Int(6), OscArg::Int(7), OscArg::Double(0.75)]);
}

#[test]
fn empty_parameter_write_batch_encodes_to_empty_buffer() {
    assert_eq!(encode_parameter_write_batch(&[]).unwrap(), Vec::<u8>::new());
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo nextest run -p advanced-show-control lv1::tcp::tests::encodes_parameter_write_batch_in_order lv1::tcp::tests::empty_parameter_write_batch_encodes_to_empty_buffer`

Expected: FAIL because `Lv1ParameterWrite`, `Lv1WriteParameter`, and `encode_parameter_write_batch` do not exist.

- [ ] **Step 3: Add LV1 write types**

Update `src/lv1/commands.rs`:

```rust
use tokio::sync::oneshot;

use super::events::Lv1ActorError;
use super::types::Lv1StateSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lv1WriteParameter {
    FaderDb,
    Pan,
    Balance,
    Width,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lv1ParameterWrite {
    pub group: i32,
    pub channel: i32,
    pub parameter: Lv1WriteParameter,
    pub value: f64,
}

pub enum Lv1Command {
    GetState {
        reply: oneshot::Sender<Lv1StateSnapshot>,
    },
    WriteBatch(Vec<Lv1ParameterWrite>),
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetPan {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetBalance {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetWidth {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetMute {
        group: i32,
        channel: i32,
        muted: bool,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    Flush {
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
}
```

- [ ] **Step 4: Add batch encoding helper and TCP_NODELAY**

Update imports and helpers in `src/lv1/tcp.rs`:

```rust
use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};
use crate::osc::{OscArg, OscError, OscMessage, decode_packet, encode_message};

pub fn encode_parameter_write_batch(
    writes: &[Lv1ParameterWrite],
) -> Result<Vec<u8>, Lv1TcpError> {
    let mut out = Vec::new();
    for write in writes {
        let address = match write.parameter {
            Lv1WriteParameter::FaderDb => "/Set/Track/Out/Gain",
            Lv1WriteParameter::Pan => "/Set/Track/Pan",
            Lv1WriteParameter::Balance => "/Set/Track/Pan/Balance",
            Lv1WriteParameter::Width => "/Set/Track/Pan/Width",
        };
        out.extend_from_slice(&encode_frame(
            address,
            &[
                OscArg::Int(write.group),
                OscArg::Int(write.channel),
                OscArg::Double(write.value),
            ],
        )?);
    }
    Ok(out)
}
```

Update `Lv1TcpClient::connect`:

```rust
pub async fn connect(host: &str, port: u16) -> std::io::Result<Self> {
    let stream = tokio::net::TcpStream::connect((host, port)).await?;
    stream.set_nodelay(true)?;
    let (reader, writer) = stream.into_split();
    Ok(Self {
        reader,
        writer,
        decoder: FrameDecoder::default(),
    })
}
```

- [ ] **Step 5: Update actor matches to ignore batch while disconnected**

In both `drain_commands_for` and the post-connect stale command drain in `src/lv1/actor.rs`, add:

```rust
Some(Lv1Command::WriteBatch(_)) => {}
```

This makes fire-and-forget batches harmless when the actor is not connected.

- [ ] **Step 6: Run tests and verify pass**

Run: `cargo nextest run -p advanced-show-control lv1::tcp::tests::encodes_parameter_write_batch_in_order lv1::tcp::tests::empty_parameter_write_batch_encodes_to_empty_buffer`

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/lv1/commands.rs src/lv1/tcp.rs src/lv1/actor.rs
git commit -m "feat: encode lv1 write batches"
```

## Task 2: Add Batch Command Bus Plumbing

**Files:**
- Modify: `src/lv1/handle.rs`
- Modify: `src/runtime/commands.rs`
- Modify: `src/lv1/actor.rs`
- Test: `src/lv1/handle.rs`
- Test: `src/runtime/commands.rs`

- [ ] **Step 1: Add failing handle test**

Add to `#[cfg(test)] mod tests` in `src/lv1/handle.rs`:

```rust
#[tokio::test]
async fn handle_sends_write_batch_without_reply() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let handle = Lv1ActorHandle::new(tx);
    let writes = vec![crate::lv1::commands::Lv1ParameterWrite {
        group: 0,
        channel: 1,
        parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
        value: -18.0,
    }];

    assert_eq!(handle.write_batch(writes.clone()).await, Ok(()));

    match rx.recv().await {
        Some(Lv1Command::WriteBatch(received)) => assert_eq!(received, writes),
        other => panic!("expected WriteBatch, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo nextest run -p advanced-show-control lv1::handle::tests::handle_sends_write_batch_without_reply`

Expected: FAIL because `write_batch` does not exist.

- [ ] **Step 3: Implement handle method**

Update imports and impl in `src/lv1/handle.rs`:

```rust
use super::commands::{Lv1Command, Lv1ParameterWrite};
```

Add method:

```rust
pub async fn write_batch(&self, writes: Vec<Lv1ParameterWrite>) -> Result<(), Lv1ActorError> {
    if writes.is_empty() {
        return Ok(());
    }
    self.tx
        .send(Lv1Command::WriteBatch(writes))
        .await
        .map_err(|_| Lv1ActorError::CommandChannelClosed)
}
```

- [ ] **Step 4: Add AppCommandBus method**

Update `src/runtime/commands.rs` imports:

```rust
use crate::lv1::commands::Lv1ParameterWrite;
```

Add method before `set_gain`:

```rust
pub async fn write_batch(&self, writes: Vec<Lv1ParameterWrite>) -> Result<(), AppCommandError> {
    if writes.is_empty() {
        return Ok(());
    }
    let lv1 = self.targets.lock().await.lv1.clone();
    let result = match lv1 {
        Some(lv1) => lv1.write_batch(writes).await.map_err(map_lv1_error),
        None => Err(AppCommandError::Lv1Unavailable),
    };
    publish_failure(&self.event_bus, "write_batch", &result);
    result
}
```

- [ ] **Step 5: Add connected actor handling for direct-write phase**

In `src/lv1/actor.rs`, import `encode_parameter_write_batch`:

```rust
use super::tcp::{
    Lv1TcpClient, decode_frame_payload, encode_parameter_write_batch, pong_for_ping,
    read_next_async, send_async,
};
```

Add this command arm in `run_connected` before single-value commands:

```rust
Some(Lv1Command::WriteBatch(writes)) => {
    if writes.is_empty() {
        continue;
    }
    let bytes = match encode_parameter_write_batch(&writes) {
        Ok(bytes) => bytes,
        Err(_) => return DisconnectReason::TcpError,
    };
    use tokio::io::AsyncWriteExt;
    if writer.write_all(&bytes).await.is_err() {
        return DisconnectReason::TcpError;
    }
}
```

- [ ] **Step 6: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control lv1::handle::tests::handle_sends_write_batch_without_reply runtime::commands::tests`

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/lv1/handle.rs src/runtime/commands.rs src/lv1/actor.rs
git commit -m "feat: add lv1 batch command plumbing"
```

## Task 3: Batch FadeEngine Writes

**Files:**
- Modify: `src/fade/actor.rs`
- Test: `tests/fade_engine.rs`

- [ ] **Step 1: Add failing zero-duration multi-target integration test**

Add this test to `tests/fade_engine.rs`:

```rust
#[tokio::test]
async fn zero_duration_fade_sends_all_parameters_in_one_batch() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (messages_tx, messages_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();
        stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
        stream.write_all(&lv1_frame("/Channels", &channels_args())).unwrap();

        let mut buf = [0_u8; 8192];
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut messages = Vec::new();
        while std::time::Instant::now() < deadline && messages.len() < 4 {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address.starts_with("/Set/Track") {
                            messages.push(msg.address);
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
        messages_tx.send(messages).unwrap();
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1, event_bus).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![
                FadeTarget { group: 0, channel: 0, parameter: FadeParameter::FaderDb, target: -12.5 },
                FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Pan, target: 15.0 },
                FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Balance, target: -10.0 },
                FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Width, target: 0.75 },
            ],
            0,
        ))
        .await
        .unwrap();

    let messages = tokio::task::spawn_blocking(move || {
        messages_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap()
    })
    .await
    .unwrap();

    assert_eq!(
        messages,
        vec![
            "/Set/Track/Out/Gain",
            "/Set/Track/Pan",
            "/Set/Track/Pan/Balance",
            "/Set/Track/Pan/Width",
        ]
    );
}
```

- [ ] **Step 2: Run test and verify current behavior**

Run: `cargo nextest run -p advanced-show-control --test fade_engine zero_duration_fade_sends_all_parameters_in_one_batch`

Expected: It may PASS at the wire-frame level even before batching, because TCP can coalesce writes. Treat this as regression coverage, not proof of batching.

- [ ] **Step 3: Replace per-target sends with batches**

Update imports in `src/fade/actor.rs`:

```rust
use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};
```

Add helper functions near `send_target`:

```rust
fn write_parameter_for(parameter: FadeParameter) -> Lv1WriteParameter {
    match parameter {
        FadeParameter::FaderDb => Lv1WriteParameter::FaderDb,
        FadeParameter::Pan => Lv1WriteParameter::Pan,
        FadeParameter::Balance => Lv1WriteParameter::Balance,
        FadeParameter::Width => Lv1WriteParameter::Width,
    }
}

fn parameter_write(
    group: i32,
    channel: i32,
    parameter: FadeParameter,
    value: f64,
) -> Lv1ParameterWrite {
    Lv1ParameterWrite {
        group,
        channel,
        parameter: write_parameter_for(parameter),
        value,
    }
}

async fn send_batch(command_bus: &AppCommandBus, writes: Vec<Lv1ParameterWrite>) {
    let _ = command_bus.write_batch(writes).await;
}
```

Update duration `0` branch:

```rust
let writes = config
    .targets
    .iter()
    .map(|target| parameter_write(target.group, target.channel, target.parameter, target.target))
    .collect();
send_batch(&command_bus, writes).await;
for target in &config.targets {
    state.channels.retain(|ch| ch.key != target.key());
    state.fan_out(FadeEvent::ChannelCompleted {
        group: target.group,
        channel: target.channel,
    });
}
```

Update tick arm to collect writes:

```rust
let mut writes = Vec::new();

for (i, ch) in state.channels.iter_mut().enumerate() {
    if ch.is_done(now) {
        let target_db = ch.exact_final_send();
        writes.push(parameter_write(ch.group, ch.channel, ch.key.parameter, target_db));
        completed_events.push(FadeEvent::ChannelCompleted { group: ch.group, channel: ch.channel });
        done_indices.push(i);
        continue;
    }

    if let Some(new_value) = ch.next_send(now) {
        writes.push(parameter_write(ch.group, ch.channel, ch.key.parameter, new_value));
    }
}

send_batch(&command_bus, writes).await;
```

Remove the old `send_target` helper if it becomes unused.

- [ ] **Step 4: Run fade tests**

Run: `cargo nextest run -p advanced-show-control --test fade_engine`

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/fade/actor.rs tests/fade_engine.rs
git commit -m "feat: batch fade engine writes"
```

## Task 4: Add Writer Task And Route Actor Writes Through It

**Files:**
- Modify: `src/lv1/actor.rs`
- Modify: `src/lv1/tcp.rs`
- Test: `tests/lv1_actor.rs`

- [ ] **Step 1: Add failing ping/pong responsiveness test**

Add this test to `tests/lv1_actor.rs`:

```rust
#[tokio::test]
async fn actor_routes_pong_without_blocking_read_loop() {
    use std::io::Read;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (pong_tx, pong_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();
        stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
        stream.write_all(&make_lv1_frame("/ping", &[OscArg::Int64(42)])).unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address == "/pong" {
                            pong_tx.send(msg.args).unwrap();
                            return;
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);
    wait_for_connected(&mut events).await;

    let args = tokio::task::spawn_blocking(move || {
        pong_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap()
    })
    .await
    .unwrap();

    assert_eq!(args, vec![OscArg::Int64(42)]);
}
```

- [ ] **Step 2: Run test before implementation**

Run: `cargo nextest run -p advanced-show-control --test lv1_actor actor_routes_pong_without_blocking_read_loop`

Expected: PASS before writer task. This is regression coverage for pong routing while refactoring.

- [ ] **Step 3: Add writer task helper**

In `src/lv1/actor.rs`, add constants and helper near `RECONNECT_DELAY`:

```rust
const WRITER_QUEUE_CAPACITY: usize = 64;

async fn writer_task(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    mut rx: mpsc::Receiver<Vec<u8>>,
    error_tx: mpsc::Sender<()>,
) {
    use tokio::io::AsyncWriteExt;

    while let Some(bytes) = rx.recv().await {
        if writer.write_all(&bytes).await.is_err() {
            let _ = error_tx.send(()).await;
            break;
        }
    }
}

fn enqueue_writer_bytes(
    writer_tx: &mpsc::Sender<Vec<u8>>,
    bytes: Vec<u8>,
) -> Result<(), DisconnectReason> {
    if bytes.is_empty() {
        return Ok(());
    }
    writer_tx.try_send(bytes).map_err(|_| DisconnectReason::TcpError)
}
```

- [ ] **Step 4: Add writer backpressure unit test**

Add this unit test inside `#[cfg(test)] mod tests` in `src/lv1/actor.rs`:

```rust
#[test]
fn enqueue_writer_bytes_reports_tcp_error_when_queue_is_full() {
    let (tx, _rx) = mpsc::channel(1);
    tx.try_send(vec![1]).unwrap();

    let result = enqueue_writer_bytes(&tx, vec![2]);

    assert_eq!(result, Err(DisconnectReason::TcpError));
}
```

- [ ] **Step 5: Move write half into writer task in `run_connected`**

Change `Lv1TcpClient` so `run_connected` can take ownership of the write half exactly once. In `src/lv1/tcp.rs`, update the struct field:

```rust
pub(crate) writer: Option<tokio::net::tcp::OwnedWriteHalf>,
```

Update construction:

```rust
writer: Some(writer),
```

Update `register_myfoh` and `send` to use:

```rust
let writer = self.writer.as_mut().ok_or_else(|| {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "LV1 writer is not available",
    )) as Box<dyn std::error::Error + Send + Sync>
})?;
```

Then in `run_connected` use:

```rust
let writer = client
    .writer
    .take()
    .expect("connected LV1 client has a writer before run_connected");
tokio::spawn(writer_task(writer, writer_rx, writer_error_tx));
```

- [ ] **Step 6: Route outbound bytes through writer channel**

Replace pong write:

```rust
let bytes = match super::tcp::encode_frame(addr, &args) {
    Ok(bytes) => bytes,
    Err(_) => return DisconnectReason::TcpError,
};
if enqueue_writer_bytes(&writer_tx, bytes).is_err() {
    return DisconnectReason::TcpError;
}
state.last_ping = Instant::now();
```

Replace `WriteBatch` direct write:

```rust
let bytes = match encode_parameter_write_batch(&writes) {
    Ok(bytes) => bytes,
    Err(_) => return DisconnectReason::TcpError,
};
if enqueue_writer_bytes(&writer_tx, bytes).is_err() {
    return DisconnectReason::TcpError;
}
```

For single-value commands, add a local helper:

```rust
fn encode_set_parameter(address: &str, group: i32, channel: i32, value: f64) -> Result<Vec<u8>, Lv1ActorError> {
    super::tcp::encode_frame(
        address,
        &[OscArg::Int(group), OscArg::Int(channel), OscArg::Double(value)],
    )
    .map_err(|_| Lv1ActorError::CommandSendFailed)
}
```

Use it in `SetGain`, `SetPan`, `SetBalance`, and `SetWidth`: encode, `try_send`, reply `Ok(())` if accepted, reply `CommandSendFailed` and disconnect if not.

For `SetMute`, encode with bool:

```rust
let result = super::tcp::encode_frame(
    "/Set/Track/Out/Mute",
    &[OscArg::Int(group), OscArg::Int(channel), OscArg::Bool(muted)],
)
.map_err(|_| Lv1ActorError::CommandSendFailed)
.and_then(|bytes| {
    enqueue_writer_bytes(&writer_tx, bytes).map_err(|_| Lv1ActorError::CommandSendFailed)
});
```

- [ ] **Step 7: Select on writer error**

Add this branch to the `tokio::select!` in `run_connected`:

```rust
writer_error = writer_error_rx.recv() => {
    if writer_error.is_some() {
        return DisconnectReason::TcpError;
    }
}
```

- [ ] **Step 8: Run LV1 actor tests**

Run: `cargo nextest run -p advanced-show-control --test lv1_actor lv1::actor::tests::enqueue_writer_bytes_reports_tcp_error_when_queue_is_full`

Expected: PASS.

- [ ] **Step 9: Commit**

Run:

```bash
git add src/lv1/actor.rs src/lv1/tcp.rs tests/lv1_actor.rs
git commit -m "feat: move lv1 socket writes to writer task"
```

## Task 5: Add TCP Handling Documentation

**Files:**
- Create: `docs/tcp-handling.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Create TCP handling doc**

Create `docs/tcp-handling.md`:

```markdown
# LV1 TCP Handling

## Overview

The LV1 actor owns one TCP connection to LV1. Incoming LV1 traffic is decoded by the actor read loop. Outbound traffic is encoded by the actor and written by a per-connection writer task.

## Framing

LV1 messages use OSC payloads inside a Waves/MyFOH TCP frame:

- 4-byte big-endian payload length.
- 8-byte LV1 header, currently `00 00 00 02 00 00 00 00` for app-sent frames.
- OSC message payload.

Encoding and decoding live in `src/lv1/tcp.rs`.

## Socket Options

`Lv1TcpClient::connect` enables `TCP_NODELAY` before splitting the stream. Fade ticks are latency-sensitive, and Nagle delays can spread logically related fader updates across time.

## Task Ownership

`Lv1Actor` owns the connected runtime and the read half of the TCP stream. A scoped writer task owns the write half after handshake.

The actor never awaits socket writability in its main connected select loop. It encodes outbound bytes and uses `try_send` into the bounded writer channel.

## Outbound Writes

`FadeEngine` batches all due parameter writes for a tick into one `WriteBatch` command. `Lv1Actor` encodes each write as an OSC frame and appends all frames into one contiguous byte buffer. The writer task sends that buffer with one `write_all`.

Shell/manual commands continue to use acknowledged single-value commands. Their acknowledgement means the live actor accepted the encoded bytes for outbound writing, not that LV1 confirmed the change.

## Ping And Pong

Incoming `/ping` messages are answered with `/pong` through the same writer channel as other outbound bytes. This preserves TCP ordering and keeps ping handling from blocking the actor read loop on a stalled socket.

## Backpressure And Disconnects

The writer channel is bounded. A full writer channel means the socket has not accepted writes for long enough that queued fader values may become stale. The actor treats a full writer channel as a TCP failure.

TCP read errors, writer errors, ping timeouts, and writer-channel backpressure all end the connected actor loop. The outer actor lifecycle clears mirrored LV1 state and publishes `Lv1Event::Disconnected`.

## Fade Safety

`FadeEngine` listens for `Lv1Event::Disconnected`. When disconnected during an active fade, it cancels all active channels, stops ticking, and publishes `FadeAborted`.
```

- [ ] **Step 2: Update architecture doc**

Change the `Lv1Actor` ownership line in `docs/architecture.md` to:

```markdown
- `Lv1Actor` owns the LV1 TCP connection lifecycle and mirrored LV1 state. During a connected session, a scoped writer task owns the TCP write half and reports write failures back to the actor.
```

- [ ] **Step 3: Commit docs**

Run:

```bash
git add docs/tcp-handling.md docs/architecture.md
git commit -m "docs: document lv1 tcp handling"
```

## Task 6: Final Verification

**Files:**
- Verify all modified Rust and docs files.

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 2: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 4: Build**

Run: `cargo build --workspace`

Expected: PASS.

- [ ] **Step 5: Inspect final status and diff**

Run: `git status --short`

Expected: clean working tree or only intentional untracked files outside this plan.

Run: `git log --oneline -10`

Expected: includes the task commits from this plan.
