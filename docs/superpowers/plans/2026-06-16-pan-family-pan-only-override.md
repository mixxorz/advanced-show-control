# Pan-Only Pan Family Override Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change pan-family manual override behavior so only LV1 pan movement cancels active pan, balance, and width fades.

**Architecture:** Keep pan, balance, and width as independent fade targets for interpolation and writes. Remove balance/width from manual override detection entirely; `Lv1Event::PanChanged` remains the only pan-family override input and cancels all active pan-family targets for the same channel.

**Tech Stack:** Rust, Tokio, existing `FadeEngine` actor tests and live `pan-family-smoke-test` CLI.

---

### Task 1: Add Failing Tests For New Override Policy

**Files:**
- Modify: `src/fade/actor.rs`

- [ ] **Step 1: Add an active target helper in the fade actor test module**

Add this helper inside `#[cfg(test)] mod tests` in `src/fade/actor.rs`:

```rust
fn active_pan_family_target(parameter: FadeParameter) -> ActiveTarget {
    let target = FadeTarget {
        group: 0,
        channel: 0,
        parameter,
        target: 45.0,
    };

    ActiveTarget::new(ActiveTargetInit {
        key: target.key(),
        group: target.group,
        channel: target.channel,
        start_value: 0.0,
        target_value: target.target,
        curve: FadeCurve::Linear,
        duration: std::time::Duration::from_millis(1000),
        started_at: Instant::now(),
        expected_generation: None,
    })
}
```

- [ ] **Step 2: Add tests proving balance and width events do not cancel through the real engine path**

Add async tests near the other fade actor tests. These tests should spawn the fade engine, start a long pan-family fade with pan, balance, and width targets, publish a `Lv1Event::BalanceChanged` or `Lv1Event::WidthChanged` with a value far away from the expected value, and assert no `FadeEvent::ChannelOverride` is emitted within a short timeout. Use the existing `AppEventBus`, `AppCommandBus`, and fake LV1 command channel patterns already present in `src/fade/actor.rs` tests.

```rust
#[tokio::test]
async fn balance_report_does_not_cancel_pan_family_targets() {
    assert_pan_family_aux_event_does_not_override(FadeParameter::Balance).await;
}

#[tokio::test]
async fn width_report_does_not_cancel_pan_family_targets() {
    assert_pan_family_aux_event_does_not_override(FadeParameter::Width).await;
}
```

Add this helper for those two tests:

```rust
async fn assert_pan_family_aux_event_does_not_override(parameter: FadeParameter) {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let bus = AppCommandBus::new(event_bus.clone());
    let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
    bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx))).await;
    let engine = spawn_engine(bus.clone(), event_bus.clone());

    tokio::spawn(async move {
        while let Some(command) = lv1_rx.recv().await {
            match command {
                Lv1Command::GetState { reply } => {
                    let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                        connection: crate::lv1::types::ConnectionStatus::Connected,
                        scene: None,
                        scene_list: vec![],
                        channels: vec![crate::lv1::types::ChannelInfo {
                            name: "Channel 1".to_string(),
                            group: 0,
                            channel: 0,
                            gain_db: -10.0,
                            muted: false,
                            pan: Some(0.0),
                            balance: Some(0.0),
                            width: Some(0.0),
                            pan_mode: None,
                        }],
                    });
                }
                Lv1Command::WriteBatch(_) => {}
                _ => {}
            }
        }
    });

    engine.start_fade(fade_config(
        scene(1, "Pan Family"),
        vec![
            FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Pan, target: 45.0 },
            FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Balance, target: 45.0 },
            FadeTarget { group: 0, channel: 0, parameter: FadeParameter::Width, target: 1.4 },
        ],
        10_000,
    )).await.unwrap();

    match parameter {
        FadeParameter::Balance => event_bus.publish(AppEvent::Lv1(crate::lv1::events::Lv1Event::BalanceChanged { group: 0, channel: 0, balance: 45.0 })),
        FadeParameter::Width => event_bus.publish(AppEvent::Lv1(crate::lv1::events::Lv1Event::WidthChanged { group: 0, channel: 0, width: 1.4 })),
        other => panic!("unexpected auxiliary parameter: {other:?}"),
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(20), events.recv()).await {
            Ok(Ok(AppEvent::Fade(FadeEvent::ChannelOverride { .. }))) => panic!("unexpected override"),
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => {}
        }
    }
}
```

- [ ] **Step 3: Add a test proving pan cancels all pan-family targets**

Add this test near the tests from Step 2:

```rust
#[tokio::test]
async fn pan_report_cancels_all_pan_family_targets_for_channel() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let mut state = EngineState::new(event_bus);
    let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));
    state.channels.push(active_pan_family_target(FadeParameter::Pan));
    state.channels.push(active_pan_family_target(FadeParameter::Balance));
    state.channels.push(active_pan_family_target(FadeParameter::Width));

    let mut fade_completed_emitted = false;
    handle_pan_family_pan_report(
        &mut state,
        0,
        0,
        45.0,
        &mut tick_interval,
        &mut fade_completed_emitted,
    );

    assert!(state.channels.is_empty());
    match events.try_recv().unwrap() {
        AppEvent::Fade(FadeEvent::ChannelOverride { group, channel, parameter }) => {
            assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 4: Run tests to verify RED**

Run: `cargo nextest run -p advanced-show-control fade::actor::tests::`

Expected: FAIL because balance and width events still route through pan-family override detection in the current implementation.

### Task 2: Implement Pan-Only Override Detection And Remove Dead Override Code

**Files:**
- Modify: `src/fade/actor.rs`
- Modify: `src/fade/tick.rs`

- [ ] **Step 1: Remove balance and width from the override event path**

In `src/fade/actor.rs`, stop matching `BalanceChanged` and `WidthChanged` in the fade-engine override path. Keep only the pan event:

```rust
Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::PanChanged { group, channel, pan })) => {
    handle_pan_family_pan_report(
        &mut state,
        group,
        channel,
        pan,
        &mut tick_interval,
        &mut fade_completed_emitted,
    );
}
```

- [ ] **Step 2: Replace `cancel_pan_family_overrides` with a pan-specific handler**

Replace the old helper with this implementation:

```rust
fn handle_pan_family_pan_report(
    state: &mut EngineState,
    group: i32,
    channel: i32,
    reported_pan: f64,
    tick_interval: &mut Option<tokio::time::Interval>,
    fade_completed_emitted: &mut bool,
) {
    let cancel = state.channels.iter().any(|ch| {
        ch.group == group
            && ch.channel == channel
            && ch.key.parameter == FadeParameter::Pan
            && ch.is_override(reported_pan)
    });

    if !cancel {
        return;
    }

    state.channels.retain(|ch| {
        !(ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family())
    });
    state.fan_out(FadeEvent::ChannelOverride {
        group,
        channel,
        parameter: FadeParameter::Pan,
    });
    for parameter in [FadeParameter::Pan, FadeParameter::Balance, FadeParameter::Width] {
        state.fan_out(FadeEvent::ChannelCancelled {
            group,
            channel,
            parameter,
        });
    }

    if !state.is_active() {
        complete_fade(tick_interval, state, fade_completed_emitted);
    }
}
```

- [ ] **Step 3: Remove balance/width override-only thresholds**

In `src/fade/tick.rs`, delete these constants:

```rust
pub const BALANCE_OVERRIDE_THRESHOLD: f64 = 1.8;
pub const WIDTH_OVERRIDE_THRESHOLD: f64 = 0.056;
```

Update `ActiveTarget::override_threshold` so balance and width are not accepted override parameters. This keeps the invariant explicit even though the actor no longer calls override detection for balance or width:

```rust
fn override_threshold(&self) -> Option<f64> {
    match self.key.parameter {
        FadeParameter::FaderDb => Some(OVERRIDE_THRESHOLD_POS),
        FadeParameter::Pan => Some(PAN_OVERRIDE_THRESHOLD),
        FadeParameter::Balance | FadeParameter::Width => None,
    }
}
```

Update `ActiveTarget::is_override` accordingly:

```rust
pub(crate) fn is_override(&self, reported_value: f64) -> bool {
    if self.is_fader() {
        let reported_pos = db_to_pos(reported_value);
        let expected_pos = db_to_pos(self.expected_value);
        (reported_pos - expected_pos).abs() >= OVERRIDE_THRESHOLD_POS
    } else if let Some(threshold) = self.override_threshold() {
        (reported_value - self.expected_value).abs() >= threshold
    } else {
        false
    }
}
```

Then run `grep` for `BALANCE_OVERRIDE_THRESHOLD` and `WIDTH_OVERRIDE_THRESHOLD` and remove any remaining references.

- [ ] **Step 4: Run targeted tests to verify GREEN**

Run: `cargo nextest run -p advanced-show-control fade::actor`

Expected: PASS.

### Task 3: Clean Up Tests And Documentation For Removed Behavior

**Files:**
- Modify: `src/fade/actor.rs`
- Modify: `src/fade/tick.rs`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Remove or update tests that assert balance/width override behavior, if present**

Search for old assumptions:

Run: `rg "BalanceChanged|WidthChanged|BALANCE_OVERRIDE_THRESHOLD|WIDTH_OVERRIDE_THRESHOLD|cancel_pan_family_overrides|manual override" src/fade docs/architecture.md`

If any tests directly prove balance or width can override, remove or update them. If there are no such tests, leave existing tests alone and rely on the new pan-only tests from Task 1.

- [ ] **Step 2: Update architecture wording**

In `docs/architecture.md`, replace this sentence:

```markdown
Pan-family manual override cancels pan, balance, and width for that channel together, but it does not cancel that channel's fader fade.
```

With:

```markdown
Pan-family manual override is driven only by pan movement. A pan override cancels pan, balance, and width for that channel together, but balance and width reports do not trigger override cancellation.
```

- [ ] **Step 3: Run formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS, or run `cargo fmt --all` and re-check if formatting is needed.

### Task 4: Verify Live Smoke Behavior

**Files:**
- Modify: none unless verification exposes an issue.

- [ ] **Step 1: Run code quality checks**

Run: `cargo nextest run -p advanced-show-control fade::actor && cargo clippy -p advanced-show-control --bin advanced-show-control -- -D warnings`

Expected: PASS.

- [ ] **Step 2: Run live smoke test**

Run: `cargo run -p advanced-show-control --bin advanced-show-control -- pan-family-smoke-test`

Expected: Min and max fades complete without balance/width override.

- [ ] **Step 3: Inspect smoke JSONL**

Read the emitted `logs/pan-family-smoke-test/lv1-pan-family-smoke-<timestamp>.jsonl` path and confirm it contains `smoke_complete` and no `manual_override_detected` event.
