# Vegas Stress Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `lv1-probe vegas`, a continuous whole-console sine-wave fader stress test that mutes channels before movement and restores gains/mutes on exit.

**Architecture:** Extend the LV1 state actor to track and set mute state. Add a small pure `vegas` module that computes deterministic fader-position-space sine output from `(group, channel, tick)`, converting through the measured fader law. Wire a new `vegas` CLI command in `src/main.rs` that snapshots all channels, mutes them, loops until Ctrl-C, then restores gains and mutes.

**Tech Stack:** Rust, Tokio, Clap, existing OSC-over-TCP LV1 actor, measured fader law in `src/fade/fader_law.rs`.

---

## File Structure

- Modify `src/lv1/state.rs`: add mute state to `ChannelInfo`, parse `/Notify/Track/Out/Mute`, add `SetMute` actor command, add `set_mute` handle method, and send `/Set/Track/Out/Mute`.
- Create `src/vegas.rs`: pure deterministic wave function and constants.
- Modify `src/lib.rs`: export `vegas` module.
- Modify `src/main.rs`: add `vegas` Clap command, parse test, runtime loop, Ctrl-C cleanup.

---

### Task 1: Add Mute State Parsing And Storage

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write failing mute state tests**

Add these tests inside `#[cfg(test)] mod tests` in `src/lv1/state.rs` after `apply_fader_update_ignores_unknown_channel`:

```rust
#[test]
fn channels_default_to_unmuted_when_batch_has_no_mute_field() {
    let args = make_channel_args(&[("Channel 1", 0, 0, -9.1)]);
    let channels = parse_channels_batch(&args).unwrap();
    assert_eq!(channels[0].muted, false);
}

#[test]
fn apply_mute_update_changes_matching_channel() {
    let mut channels = vec![
        ChannelInfo {
            group: 0,
            channel: 0,
            name: "Ch 1".to_string(),
            gain_db: -9.0,
            muted: false,
        },
        ChannelInfo {
            group: 0,
            channel: 1,
            name: "Ch 2".to_string(),
            gain_db: -12.0,
            muted: false,
        },
    ];
    apply_mute_update(&mut channels, 0, 0, true);
    assert!(channels[0].muted);
    assert!(!channels[1].muted);
}

#[test]
fn apply_mute_update_ignores_unknown_channel() {
    let mut channels = vec![ChannelInfo {
        group: 0,
        channel: 0,
        name: "Ch 1".to_string(),
        gain_db: -9.0,
        muted: false,
    }];
    apply_mute_update(&mut channels, 0, 99, true);
    assert!(!channels[0].muted);
}
```

Update existing expected `ChannelInfo` literals in tests to include `muted: false`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test lv1::state::tests::channels_default_to_unmuted_when_batch_has_no_mute_field lv1::state::tests::apply_mute_update_changes_matching_channel lv1::state::tests::apply_mute_update_ignores_unknown_channel`

Expected: FAIL with missing `muted` field and missing `apply_mute_update`.

- [ ] **Step 3: Add minimal mute state storage**

In `src/lv1/state.rs`, change `ChannelInfo` to:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
    pub muted: bool,
}
```

In `parse_channels_batch`, change the push to:

```rust
channels.push(ChannelInfo {
    group,
    channel,
    name,
    gain_db,
    muted: false,
});
```

Add this helper after `apply_fader_update`:

```rust
pub fn apply_mute_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, muted: bool) {
    if let Some(ch) = channels.iter_mut().find(|c| c.group == group && c.channel == channel) {
        ch.muted = muted;
    }
}
```

Update all test `ChannelInfo { ... }` literals in `src/lv1/state.rs` to include `muted: false`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test lv1::state::tests::channels_default_to_unmuted_when_batch_has_no_mute_field lv1::state::tests::apply_mute_update_changes_matching_channel lv1::state::tests::apply_mute_update_ignores_unknown_channel`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: track LV1 mute state"
```

---

### Task 2: Add LV1 Mute Notifications And SetMute Command

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write failing actor mute tests**

Add this event variant test near existing parser/helper tests:

```rust
#[test]
fn osc_bool_values_map_to_mute_state() {
    assert_eq!(osc_arg_to_bool(&OscArg::Bool(true)), Some(true));
    assert_eq!(osc_arg_to_bool(&OscArg::Bool(false)), Some(false));
    assert_eq!(osc_arg_to_bool(&OscArg::Int(1)), Some(true));
    assert_eq!(osc_arg_to_bool(&OscArg::Int(0)), Some(false));
    assert_eq!(osc_arg_to_bool(&OscArg::Int(2)), None);
}
```

Add this async test after `actor_sends_set_gain_while_waiting_for_input`:

```rust
#[tokio::test]
async fn actor_sends_set_mute_while_waiting_for_input() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (address_tx, address_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        use std::io::Read;

        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = crate::lv1::tcp::FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        let _ = address_tx.send(msg.address);
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
    });

    let handle = spawn_actor("127.0.0.1".to_string(), port);
    let mut events = handle.subscribe().await;

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Some(e) = events.recv().await {
            if matches!(e, Lv1Event::Connected) { break; }
        }
    }).await.unwrap();

    handle.set_mute(0, 1, true).await;

    tokio::task::spawn_blocking(move || {
        loop {
            let address = address_rx
                .recv_timeout(std::time::Duration::from_millis(150))
                .expect("SetMute frame was not sent promptly while actor was waiting for input");
            if address == "/Set/Track/Out/Mute" {
                break;
            }
        }
    }).await.unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test lv1::state::tests::osc_bool_values_map_to_mute_state lv1::state::tests::actor_sends_set_mute_while_waiting_for_input`

Expected: FAIL with missing `osc_arg_to_bool` and `set_mute`.

- [ ] **Step 3: Add SetMute command, event, parser, and sender**

In `Lv1Command`, add:

```rust
SetMute {
    group: i32,
    channel: i32,
    muted: bool,
},
```

In `Lv1Event`, add:

```rust
MuteChanged {
    group: i32,
    channel: i32,
    muted: bool,
},
```

In `Lv1ActorHandle`, add:

```rust
/// Send a `/Set/Track/Out/Mute` command to LV1. Fire and forget.
pub async fn set_mute(&self, group: i32, channel: i32, muted: bool) {
    let _ = self.tx.send(Lv1Command::SetMute { group, channel, muted }).await;
}
```

Add this helper near parser helpers:

```rust
pub fn osc_arg_to_bool(arg: &OscArg) -> Option<bool> {
    match arg {
        OscArg::Bool(value) => Some(*value),
        OscArg::Int(0) => Some(false),
        OscArg::Int(1) => Some(true),
        _ => None,
    }
}
```

In every command-draining match that currently handles `SetGain`, add `SetMute` handling. While disconnected or before connection setup completes, drop it the same way `SetGain` is dropped.

In the connected command loop, add:

```rust
Some(Lv1Command::SetMute { group, channel, muted }) => {
    let _ = send_async(
        writer,
        "/Set/Track/Out/Mute",
        &[
            crate::osc::OscArg::Int(group),
            crate::osc::OscArg::Int(channel),
            crate::osc::OscArg::Bool(muted),
        ],
    ).await;
}
```

In `handle_message`, add:

```rust
"/Notify/Track/Out/Mute" => {
    if let (
        Some(crate::osc::OscArg::Int(group)),
        Some(crate::osc::OscArg::Int(channel)),
        Some(mute_arg),
    ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
    {
        if let Some(muted) = osc_arg_to_bool(mute_arg) {
            apply_mute_update(&mut state.channels, *group, *channel, muted);
            state.fan_out(Lv1Event::MuteChanged {
                group: *group,
                channel: *channel,
                muted,
            });
        }
    }
}
```

In `run_monitor`, add a `Lv1Event::MuteChanged` match arm:

```rust
Lv1Event::MuteChanged { group, channel, muted } => {
    println!("[mute] group={group} ch={channel} muted={muted}");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test lv1::state::tests::osc_bool_values_map_to_mute_state lv1::state::tests::actor_sends_set_mute_while_waiting_for_input`

Expected: PASS.

- [ ] **Step 5: Run state tests**

Run: `cargo test lv1::state::tests`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lv1/state.rs src/main.rs
git commit -m "feat: add LV1 mute command"
```

---

### Task 3: Add Pure Vegas Wave Module

**Files:**
- Create: `src/vegas.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing pure wave tests**

Create `src/vegas.rs` with only tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::fader_law::pos_to_db;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-10,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn stable_index_is_based_on_group_and_channel() {
        assert_eq!(stable_index(0, 0), 0);
        assert_eq!(stable_index(0, 7), 7);
        assert_eq!(stable_index(1, 0), 128);
    }

    #[test]
    fn faders_eight_apart_share_phase_at_same_tick() {
        assert_close(fader_position_at(0, 0, 3), fader_position_at(0, 8, 3));
    }

    #[test]
    fn tick_advancement_changes_phase() {
        let first = fader_position_at(0, 0, 0);
        let second = fader_position_at(0, 0, 1);
        assert!((first - second).abs() > 1e-6);
    }

    #[test]
    fn fader_position_stays_in_range() {
        for group in 0..=8 {
            for channel in 0..=128 {
                for tick in 0..=64 {
                    let pos = fader_position_at(group, channel, tick);
                    assert!((0.0..=1.0).contains(&pos), "pos={pos}");
                }
            }
        }
    }

    #[test]
    fn gain_uses_measured_fader_law() {
        let pos = fader_position_at(0, 0, 0);
        assert_close(gain_db_at(0, 0, 0), pos_to_db(pos));
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod vegas;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test vegas::tests`

Expected: FAIL with missing `stable_index`, `fader_position_at`, and `gain_db_at`.

- [ ] **Step 3: Implement pure wave functions**

Replace `src/vegas.rs` with:

```rust
use crate::fade::fader_law::pos_to_db;

pub const WAVE_WIDTH_FADERS: f64 = 8.0;
pub const GROUP_STRIDE: i32 = 128;
pub const PHASE_STEP: f64 = std::f64::consts::TAU / 32.0;

pub fn stable_index(group: i32, channel: i32) -> i32 {
    group * GROUP_STRIDE + channel
}

pub fn fader_position_at(group: i32, channel: i32, tick: u64) -> f64 {
    let index = stable_index(group, channel) as f64;
    let phase = (index / WAVE_WIDTH_FADERS) * std::f64::consts::TAU + tick as f64 * PHASE_STEP;
    ((phase.sin() + 1.0) / 2.0).clamp(0.0, 1.0)
}

pub fn gain_db_at(group: i32, channel: i32, tick: u64) -> f64 {
    pos_to_db(fader_position_at(group, channel, tick))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::fader_law::pos_to_db;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-10,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn stable_index_is_based_on_group_and_channel() {
        assert_eq!(stable_index(0, 0), 0);
        assert_eq!(stable_index(0, 7), 7);
        assert_eq!(stable_index(1, 0), 128);
    }

    #[test]
    fn faders_eight_apart_share_phase_at_same_tick() {
        assert_close(fader_position_at(0, 0, 3), fader_position_at(0, 8, 3));
    }

    #[test]
    fn tick_advancement_changes_phase() {
        let first = fader_position_at(0, 0, 0);
        let second = fader_position_at(0, 0, 1);
        assert!((first - second).abs() > 1e-6);
    }

    #[test]
    fn fader_position_stays_in_range() {
        for group in 0..=8 {
            for channel in 0..=128 {
                for tick in 0..=64 {
                    let pos = fader_position_at(group, channel, tick);
                    assert!((0.0..=1.0).contains(&pos), "pos={pos}");
                }
            }
        }
    }

    #[test]
    fn gain_uses_measured_fader_law() {
        let pos = fader_position_at(0, 0, 0);
        assert_close(gain_db_at(0, 0, 0), pos_to_db(pos));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test vegas::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/vegas.rs src/lib.rs
git commit -m "feat: add vegas wave function"
```

---

### Task 4: Add Vegas CLI Parsing

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing CLI parse test**

Add this test after `parses_fade_test_command`:

```rust
#[test]
fn parses_vegas_command_without_group_option() {
    let cli = Cli::try_parse_from([
        "lv1-probe",
        "vegas",
        "--host", "192.168.1.10",
        "--port", "50001",
        "--timeout-ms", "3000",
    ]).unwrap();

    match cli.command {
        Command::Vegas { host, port, timeout_ms } => {
            assert_eq!(host.as_deref(), Some("192.168.1.10"));
            assert_eq!(port, Some(50001));
            assert_eq!(timeout_ms, 3000);
        }
        other => panic!("expected Vegas, got {other:?}"),
    }

    let err = Cli::try_parse_from([
        "lv1-probe",
        "vegas",
        "--group", "0",
    ]).unwrap_err();
    assert!(err.to_string().contains("unexpected argument '--group'"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test parses_vegas_command_without_group_option`

Expected: FAIL with missing `Vegas` variant.

- [ ] **Step 3: Add Vegas command variant and dispatch stub**

Add to `Command` enum:

```rust
#[command(about = "Run a whole-console LV1 fader sine-wave stress test")]
Vegas {
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long, default_value_t = 6000)]
    timeout_ms: u64,
},
```

Add to the `match cli.command` in `main`:

```rust
Command::Vegas { host, port, timeout_ms } => run_vegas(host, port, timeout_ms).await,
```

Add this temporary stub before `unix_timestamp_secs`:

```rust
async fn run_vegas(
    _host: Option<String>,
    _port: Option<u16>,
    _timeout_ms: u64,
) -> AppResult<()> {
    Err("vegas command is not implemented yet".into())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test parses_vegas_command_without_group_option`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add vegas CLI command"
```

---

### Task 5: Implement Vegas Runtime And Cleanup

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add small helper for waiting for channel snapshot**

Add this helper before `run_vegas`:

```rust
async fn wait_for_channels(
    lv1: &lv1_scene_fade_utility::lv1::state::Lv1ActorHandle,
    timeout_ms: u64,
) -> AppResult<Vec<lv1_scene_fade_utility::lv1::state::ChannelInfo>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let snapshot = lv1.get_state().await;
        if !snapshot.channels.is_empty() {
            return Ok(snapshot.channels);
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for LV1 channel snapshot".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
```

- [ ] **Step 2: Replace the Vegas stub with runtime implementation**

Replace `run_vegas` with:

```rust
async fn run_vegas(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
) -> AppResult<()> {
    use lv1_scene_fade_utility::lv1::state::{ChannelInfo, Lv1Event, spawn_actor};
    use lv1_scene_fade_utility::vegas::gain_db_at;

    const VEGAS_TICK_HZ: u64 = 25;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let lv1 = spawn_actor(host.clone(), port);
    let mut events = lv1.subscribe().await;

    tokio::time::timeout(Duration::from_millis(timeout_ms), async {
        while let Some(e) = events.recv().await {
            if matches!(e, Lv1Event::Connected) {
                println!("[connected] {host}:{port}");
                break;
            }
        }
    }).await.map_err(|_| "timed out waiting for LV1 connection")?;

    let mut original: Vec<ChannelInfo> = wait_for_channels(&lv1, timeout_ms).await?;
    original.sort_by_key(|ch| (ch.group, ch.channel));
    println!("[vegas] captured {} faders", original.len());

    for ch in &original {
        lv1.set_mute(ch.group, ch.channel, true).await;
    }
    println!("[vegas] muted captured faders; press Ctrl-C to stop and restore");

    let mut interval = tokio::time::interval(Duration::from_millis(1000 / VEGAS_TICK_HZ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut tick = 0_u64;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("[vegas] stopping; restoring captured faders");
                break;
            }
            _ = interval.tick() => {
                for ch in &original {
                    lv1.set_gain(ch.group, ch.channel, gain_db_at(ch.group, ch.channel, tick)).await;
                }
                tick = tick.wrapping_add(1);
            }
        }
    }

    for ch in &original {
        lv1.set_gain(ch.group, ch.channel, ch.gain_db).await;
    }
    for ch in &original {
        lv1.set_mute(ch.group, ch.channel, ch.muted).await;
    }

    println!("[vegas] restore commands sent");
    Ok(())
}
```

- [ ] **Step 3: Run formatting**

Run: `cargo fmt`

Expected: no errors.

- [ ] **Step 4: Run focused compile check**

Run: `cargo test parses_vegas_command_without_group_option vegas::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement vegas stress test"
```

---

### Task 6: Full Verification

**Files:**
- No source edits expected.

- [ ] **Step 1: Run all tests**

Run: `cargo test`

Expected: PASS.

- [ ] **Step 2: Run formatter check**

Run: `cargo fmt --check`

Expected: PASS.

- [ ] **Step 3: Confirm CLI help shows Vegas**

Run: `cargo run -- --help`

Expected: output includes `vegas`.

- [ ] **Step 4: Confirm Vegas rejects group option**

Run: `cargo run -- vegas --group 0`

Expected: command fails with `unexpected argument '--group'`.

- [ ] **Step 5: Commit any formatting-only changes if needed**

If `cargo fmt --check` changed nothing, skip this step. If `cargo fmt` made formatting edits during verification, run:

```bash
git add src/lv1/state.rs src/main.rs src/vegas.rs src/lib.rs
git commit -m "style: format vegas stress test"
```

---

## Self-Review Notes

- Spec coverage: CLI, all-fader scope, deterministic pure wave function, fader-law conversion, mute-before-move, Ctrl-C cleanup, no empty loop, and tests are covered by Tasks 1-6.
- Mute support was missing in current code, so Tasks 1-2 add it before the `vegas` command depends on it.
- The pure wave function uses `group * 128 + channel`, an 8-fader wavelength, and `pos_to_db` as required by the approved spec.
