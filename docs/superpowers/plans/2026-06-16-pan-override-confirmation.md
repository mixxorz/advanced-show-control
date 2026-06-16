# Pan Override Confirmation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Require two consecutive out-of-threshold pan reports before pan-family manual override cancellation, while logging each suspect hit at debug level.

**Architecture:** Keep confirmation state on `ActiveTarget` so it is scoped to a single active target key and naturally reset when fade ownership changes. `FadeEngine` will use a mutable pan-target check before cancelling the pan family; balance and width reports remain ignored.

**Tech Stack:** Rust core crate, Tokio actor tests, `tracing::debug!`, `cargo nextest`.

---

## File Structure

- Modify `src/fade/tick.rs`: add `PAN_OVERRIDE_CONFIRMATION_COUNT`, add `override_deviation_count` to `ActiveTarget`, and add a method that records pan suspect hits while preserving existing fader/balance/width override behavior.
- Modify `src/fade/actor.rs`: change `handle_pan_family_pan_report` to call the mutable pan confirmation method and emit the debug diagnostic on every out-of-threshold pan suspect hit.
- Tests stay in the existing `#[cfg(test)]` modules in `src/fade/tick.rs` and `src/fade/actor.rs`; no new test files are needed.
- No docs changes are required beyond the approved spec unless implementation reveals a behavior difference.

## Task 1: Add ActiveTarget Pan Confirmation State

**Files:**
- Modify: `src/fade/tick.rs`
- Test: `src/fade/tick.rs`

- [ ] **Step 1: Write failing tests for pan confirmation state**

Add these tests near the existing override tests in `src/fade/tick.rs`:

```rust
    #[test]
    fn pan_override_requires_two_consecutive_out_of_threshold_reports() {
        let mut target = make_pan_family_target(FadeParameter::Pan);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);

        assert!(target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, PAN_OVERRIDE_CONFIRMATION_COUNT);
    }

    #[test]
    fn pan_override_confirmation_resets_after_in_threshold_report() {
        let mut target = make_pan_family_target(FadeParameter::Pan);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);

        assert!(!target.record_override_report(1.0));
        assert_eq!(target.override_deviation_count, 0);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);
    }

    #[test]
    fn fader_override_remains_immediate() {
        use crate::fade::fader_law::{db_to_pos, pos_to_db};

        let mut target = make_channel(-20.0, -10.0, 4000);
        let expected_pos = db_to_pos(-20.0);
        let over_pos = expected_pos + OVERRIDE_THRESHOLD_POS + 0.001;
        let reported_db = pos_to_db(over_pos);

        assert!(target.record_override_report(reported_db));
        assert_eq!(target.override_deviation_count, 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo nextest run -p advanced-show-control fade::tick::tests::pan_override_requires_two_consecutive_out_of_threshold_reports fade::tick::tests::pan_override_confirmation_resets_after_in_threshold_report fade::tick::tests::fader_override_remains_immediate
```

Expected: FAIL because `record_override_report`, `override_deviation_count`, and `PAN_OVERRIDE_CONFIRMATION_COUNT` do not exist yet.

- [ ] **Step 3: Implement minimal tick state and method**

In `src/fade/tick.rs`, add the constant after `PAN_OVERRIDE_THRESHOLD`:

```rust
pub const PAN_OVERRIDE_CONFIRMATION_COUNT: u8 = 2;
```

Add the field to `ActiveTarget`:

```rust
    pub(crate) override_deviation_count: u8,
```

Initialize it in `ActiveTarget::new`:

```rust
            override_deviation_count: 0,
```

Add this method to `impl ActiveTarget` near `is_override`:

```rust
    /// Records an override report and returns true when override is confirmed.
    pub(crate) fn record_override_report(&mut self, reported_value: f64) -> bool {
        if self.key.parameter != FadeParameter::Pan {
            return self.is_override(reported_value);
        }

        if self.is_override(reported_value) {
            self.override_deviation_count = self.override_deviation_count.saturating_add(1);
            self.override_deviation_count >= PAN_OVERRIDE_CONFIRMATION_COUNT
        } else {
            self.override_deviation_count = 0;
            false
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo nextest run -p advanced-show-control fade::tick::tests::pan_override_requires_two_consecutive_out_of_threshold_reports fade::tick::tests::pan_override_confirmation_resets_after_in_threshold_report fade::tick::tests::fader_override_remains_immediate
```

Expected: PASS for all three tests.

- [ ] **Step 5: Commit Task 1**

Run:

```bash
git status --short
git diff -- src/fade/tick.rs
git add src/fade/tick.rs
git commit -m "feat: track pan override confirmation"
```

Expected: commit includes only `src/fade/tick.rs` changes for this task.

## Task 2: Apply Confirmation In Pan-Family Cancellation

**Files:**
- Modify: `src/fade/actor.rs`
- Test: `src/fade/actor.rs`

- [ ] **Step 1: Update existing actor tests to expect confirmation**

In `src/fade/actor.rs`, update `pan_report_cancels_all_pan_family_targets_for_channel` so it sends two out-of-threshold pan reports before asserting cancellation:

```rust
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );
        assert_eq!(state.channels.len(), 4);

        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );
```

Leave `pan_report_cancels_balance_and_width_when_pan_target_is_missing` and `pan_report_completes_when_no_active_targets_remain` as immediate-cancel tests because they cover the no-active-pan-target path.

- [ ] **Step 2: Add actor tests for one-hit suppression and reset**

Add these tests after `pan_report_completes_when_no_active_targets_remain`:

```rust
    #[tokio::test]
    async fn one_out_of_threshold_pan_report_does_not_cancel_active_pan_family_targets() {
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

        assert_eq!(state.channels.len(), 3);
        assert!(state.channels.iter().any(|ch| ch.key.parameter == FadeParameter::Pan));
        assert!(state.channels.iter().any(|ch| ch.key.parameter == FadeParameter::Balance));
        assert!(state.channels.iter().any(|ch| ch.key.parameter == FadeParameter::Width));

        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade(FadeEvent::ChannelOverride { .. }) => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled { .. }) => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => {
                    panic!("unexpected FadeCompleted event")
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn in_threshold_pan_report_resets_override_confirmation() {
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
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            0.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 3);
        let pan_target = state
            .channels
            .iter()
            .find(|ch| ch.key.parameter == FadeParameter::Pan)
            .expect("pan target should remain active");
        assert_eq!(pan_target.override_deviation_count, 1);

        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade(FadeEvent::ChannelOverride { .. }) => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled { .. }) => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => {
                    panic!("unexpected FadeCompleted event")
                }
                _ => {}
            }
        }
    }
```

- [ ] **Step 3: Run actor tests to verify they fail**

Run:

```bash
cargo nextest run -p advanced-show-control fade::actor::tests::one_out_of_threshold_pan_report_does_not_cancel_active_pan_family_targets fade::actor::tests::in_threshold_pan_report_resets_override_confirmation fade::actor::tests::pan_report_cancels_all_pan_family_targets_for_channel
```

Expected: FAIL because `handle_pan_family_pan_report` still cancels immediately when an active pan target reports out of threshold.

- [ ] **Step 4: Implement mutable confirmation in actor**

In `src/fade/actor.rs`, replace the `pan_override` calculation in `handle_pan_family_pan_report` with this code:

```rust
    let pan_override = if let Some(pan_target) = state.channels.iter_mut().find(|ch| {
        ch.group == group && ch.channel == channel && ch.key.parameter == FadeParameter::Pan
    }) {
        let expected_pan = pan_target.expected_value;
        let is_out_of_threshold = pan_target.is_override(reported_pan);
        let confirmed = pan_target.record_override_report(reported_pan);
        if is_out_of_threshold {
            tracing::debug!(
                event = "pan_override_suspect",
                group,
                channel,
                reported_pan,
                expected_pan,
                threshold = crate::fade::tick::PAN_OVERRIDE_THRESHOLD,
                confirmation_count = pan_target.override_deviation_count,
                required_confirmation_count = crate::fade::tick::PAN_OVERRIDE_CONFIRMATION_COUNT,
                "Pan override suspect: group {}, channel {}, reported {}, expected {}",
                group,
                channel,
                reported_pan,
                expected_pan
            );
        }
        confirmed
    } else {
        state.channels.iter().any(|ch| {
            ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family()
        })
    };
```

Keep the cancellation block after `if !pan_override { return; }` unchanged.

- [ ] **Step 5: Run actor tests to verify they pass**

Run:

```bash
cargo nextest run -p advanced-show-control fade::actor::tests::one_out_of_threshold_pan_report_does_not_cancel_active_pan_family_targets fade::actor::tests::in_threshold_pan_report_resets_override_confirmation fade::actor::tests::pan_report_cancels_all_pan_family_targets_for_channel fade::actor::tests::balance_report_does_not_cancel_pan_family_targets fade::actor::tests::width_report_does_not_cancel_pan_family_targets fade::actor::tests::pan_report_cancels_balance_and_width_when_pan_target_is_missing fade::actor::tests::pan_report_completes_when_no_active_targets_remain
```

Expected: PASS for all listed tests.

- [ ] **Step 6: Commit Task 2**

Run:

```bash
git status --short
git diff -- src/fade/actor.rs
git add src/fade/actor.rs
git commit -m "fix: confirm pan override before cancelling"
```

Expected: commit includes only `src/fade/actor.rs` changes for this task.

## Task 3: Verify Debug Diagnostic Coverage And Full Rust Checks

**Files:**
- Modify: `src/fade/actor.rs` only if the diagnostic fields need adjustment after review
- Test: existing Rust test suite

- [ ] **Step 1: Review diagnostic call against the spec**

Confirm the `tracing::debug!` call in `handle_pan_family_pan_report` includes these fields exactly or equivalently:

```rust
event = "pan_override_suspect",
group,
channel,
reported_pan,
expected_pan,
threshold = crate::fade::tick::PAN_OVERRIDE_THRESHOLD,
confirmation_count = pan_target.override_deviation_count,
required_confirmation_count = crate::fade::tick::PAN_OVERRIDE_CONFIRMATION_COUNT,
```

Expected: each out-of-threshold pan report against an active pan target emits a debug diagnostic, including the second report that confirms cancellation.

- [ ] **Step 2: Run formatting check**

Run:

```bash
cargo fmt --all -- --check
```

Expected: PASS. If it fails, run `cargo fmt --all`, inspect the diff, and commit only formatting changes that belong to this feature.

- [ ] **Step 3: Run targeted fade tests**

Run:

```bash
cargo nextest run -p advanced-show-control fade
```

Expected: PASS.

- [ ] **Step 4: Run workspace clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Run workspace tests**

Run:

```bash
cargo nextest run --workspace
```

Expected: PASS.

- [ ] **Step 6: Commit verification-only adjustments if needed**

If formatting or clippy required changes, run:

```bash
git status --short
git diff
git add src/fade/tick.rs src/fade/actor.rs
git commit -m "chore: polish pan override confirmation"
```

Expected: skip this step if there are no new changes after Task 2.

## Self-Review

- Spec coverage: Task 1 covers per-active-target count, reset behavior, pan-only participation, and unchanged fader behavior. Task 2 covers actor cancellation behavior, no-active-pan fallback, balance/width ignore behavior, and debug suspect diagnostics. Task 3 covers final verification.
- Placeholder scan: No placeholders, deferred work, or unspecified tests remain.
- Type consistency: The plan consistently uses `override_deviation_count`, `record_override_report`, `PAN_OVERRIDE_CONFIRMATION_COUNT`, and existing `FadeParameter` names.
