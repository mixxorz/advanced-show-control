# Scene List Edit Recall Suppression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent LV1 scene-list management actions from starting app-managed scene recall fades.

**Architecture:** Keep `Lv1Actor` factual and unchanged. Add a small scene-list edit gate to `SceneRecallState`, and have `SceneRecallFader` feed `SceneListChanged` facts into that state before evaluating `SceneChanged` facts.

**Tech Stack:** Rust, Tokio paused-time tests, existing `AppEventBus`, `AppCommandBus`, `SceneRecallFader`, and `SceneRecallState`.

---

## File Structure

- Modify `src/scene_recall/state.rs`: add scene-list baseline tracking and a 500 ms scene-list edit suppression window.
- Modify `src/scene_recall/actor.rs`: handle `Lv1Event::SceneListChanged` in the recall actor and add log-faithful actor tests.
- No changes to `src/lv1/state.rs`: LV1 facts must remain visible to UI and shell state.
- Reference spec: `docs/superpowers/specs/2026-06-10-scene-list-edit-recall-suppression-design.md`.

Use this timeout wrapper for timing-sensitive test commands on macOS:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall -- --nocapture
```

---

### Task 1: Add State-Level Scene-List Edit Gate

**Files:**
- Modify: `src/scene_recall/state.rs`

- [ ] **Step 1: Write failing state tests**

Add `SceneListEntry` to the imports and add tests inside `#[cfg(test)] mod tests`:

```rust
use crate::lv1::types::SceneListEntry;

fn scene_entry(index: i32, name: &str) -> SceneListEntry {
    SceneListEntry {
        index,
        name: name.to_string(),
    }
}

fn initial_scene_list() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 2 -- Changed"),
        scene_entry(4, "Song 3"),
        scene_entry(5, "Test"),
    ]
}

fn moved_current_scene_list() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 3"),
        scene_entry(4, "Song 2 -- Changed"),
        scene_entry(5, "Test"),
    ]
}

#[test]
fn first_scene_list_establishes_baseline_without_suppression() {
    let mut state = SceneRecallState::default();
    let now = Instant::now();

    state.observe_scene_list(initial_scene_list(), now);

    assert!(!state.is_scene_list_edit_suppressed(now));
}

#[test]
fn identical_scene_list_does_not_open_suppression_window() {
    let mut state = SceneRecallState::default();
    let now = Instant::now();

    state.observe_scene_list(initial_scene_list(), now);
    state.observe_scene_list(initial_scene_list(), now + Duration::from_millis(10));

    assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
}

#[test]
fn changed_scene_list_suppresses_until_window_expires() {
    let mut state = SceneRecallState::default();
    let now = Instant::now();

    state.observe_scene_list(initial_scene_list(), now);
    state.observe_scene_list(moved_current_scene_list(), now + Duration::from_millis(10));

    assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
    assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(509)));
    assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(510)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall::state -- --nocapture
```

Expected: FAIL because `observe_scene_list` and `is_scene_list_edit_suppressed` do not exist.

- [ ] **Step 3: Implement minimal state gate**

Update `src/scene_recall/state.rs`:

```rust
use crate::lv1::types::{SceneListEntry, SceneState};

const RECALL_ARMING_DELAY: Duration = Duration::from_millis(2_000);
const SAME_SCENE_REPEAT_DELAY: Duration = Duration::from_millis(500);
const SCENE_LIST_EDIT_SUPPRESSION_WINDOW: Duration = Duration::from_millis(500);
```

Add fields to `SceneRecallState`:

```rust
last_scene_list: Option<Vec<SceneListEntry>>,
scene_list_edit_suppressed_until: Option<Instant>,
```

Add methods to `impl SceneRecallState`:

```rust
pub fn observe_scene_list(&mut self, scene_list: Vec<SceneListEntry>, now: Instant) {
    match self.last_scene_list.as_ref() {
        None => {
            self.last_scene_list = Some(scene_list);
        }
        Some(previous) if previous == &scene_list => {
            self.last_scene_list = Some(scene_list);
        }
        Some(_) => {
            self.last_scene_list = Some(scene_list);
            self.scene_list_edit_suppressed_until = Some(now + SCENE_LIST_EDIT_SUPPRESSION_WINDOW);
        }
    }
}

pub fn is_scene_list_edit_suppressed(&self, now: Instant) -> bool {
    self.scene_list_edit_suppressed_until
        .map(|deadline| now < deadline)
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall::state -- --nocapture
```

Expected: PASS.

---

### Task 2: Wire Scene-List Gate Into SceneRecallFader

**Files:**
- Modify: `src/scene_recall/actor.rs`

- [ ] **Step 1: Write failing actor tests for log-faithful scenarios**

Update the test import:

```rust
use crate::lv1::types::{Lv1StateSnapshot, SceneListEntry, SceneState};
```

Add helper functions near `intro_scene()`:

```rust
fn song_3_at(index: i32) -> SceneState {
    SceneState {
        index,
        name: "Song 3".to_string(),
    }
}

fn scene_entry(index: i32, name: &str) -> SceneListEntry {
    SceneListEntry {
        index,
        name: name.to_string(),
    }
}

fn scene_list_before_current_move() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 2 -- Changed"),
        scene_entry(4, "Song 3"),
        scene_entry(5, "Test"),
    ]
}

fn scene_list_after_current_move() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 3"),
        scene_entry(4, "Song 2 -- Changed"),
        scene_entry(5, "Test"),
    ]
}

fn scene_list_before_non_current_rename() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 2"),
        scene_entry(4, "Song 3"),
        scene_entry(5, "Test"),
    ]
}

fn scene_list_after_non_current_rename() -> Vec<SceneListEntry> {
    vec![
        scene_entry(0, "My first scene"),
        scene_entry(1, "Song 1"),
        scene_entry(2, "My second scene"),
        scene_entry(3, "Song 2 -- Changed"),
        scene_entry(4, "Song 3"),
        scene_entry(5, "Test"),
    ]
}

async fn assert_no_scene_recall_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    tokio::task::yield_now().await;
    loop {
        match events.try_recv() {
            Ok(AppEvent::SceneRecall(event)) => panic!("unexpected scene recall event: {event:?}"),
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
                panic!("test subscriber lagged by {count} events")
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                panic!("event bus closed")
            }
        }
    }
}
```

Add tests:

```rust
#[tokio::test(start_paused = true)]
async fn current_scene_move_sequence_does_not_start_fade() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_generation(1).await;
    let show = show_handle();
    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
    command_bus.set_lv1(Some(lv1)).await;
    command_bus.set_fade(Some(fade)).await;
    command_bus.set_show(Some(show.clone())).await;
    seed_show(&show).await;

    let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
    release_lv1.send(()).unwrap();
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(song_3_at(4))));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_before_current_move())));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(2_050)).await;
    tokio::task::yield_now().await;

    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_after_current_move())));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(song_3_at(3))));
    tokio::task::yield_now().await;

    assert!(matches!(fade_rx.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)));
    assert_eq!(fade_starts.load(Ordering::SeqCst), 0);
    assert_no_scene_recall_event(&mut events).await;

    handle.abort();
    command_bus.set_lv1(None).await;
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn identical_scene_list_resend_does_not_block_real_recall() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_generation(1).await;
    let show = show_handle();
    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
    command_bus.set_lv1(Some(lv1)).await;
    command_bus.set_fade(Some(fade)).await;
    command_bus.set_show(Some(show.clone())).await;
    seed_show(&show).await;

    let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
    release_lv1.send(()).unwrap();
    arm_recall_state(&event_bus).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_before_current_move())));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_before_current_move())));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

    let fade_command = tokio::time::timeout(Duration::from_secs(1), fade_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fade_command.scene.index, 1);
    assert_eq!(fade_command.scene.name, "Intro");
    assert_eq!(fade_starts.load(Ordering::SeqCst), 1);

    handle.abort();
    command_bus.set_lv1(None).await;
    server.await.unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall::actor -- --nocapture
```

Expected: FAIL because `SceneRecallFader` ignores `SceneListChanged`; the current-scene move test starts or logs automation.

- [ ] **Step 3: Wire the gate into actor event handling**

Modify the `match events.recv().await` loop in `spawn_scene_recall_fader`:

```rust
Ok(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list))) => {
    recall_state.observe_scene_list(scene_list, tokio::time::Instant::now());
}
Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
    if recall_state.is_scene_list_edit_suppressed(tokio::time::Instant::now()) {
        continue;
    }
    if !recall_state.accepts(&scene) {
        continue;
    }
    // Leave the rest of the existing SceneChanged handling unchanged.
}
```

- [ ] **Step 4: Run actor tests to verify they pass**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall::actor -- --nocapture
```

Expected: PASS.

---

### Task 3: Add Remaining Sequence Coverage

**Files:**
- Modify: `src/scene_recall/actor.rs`

- [ ] **Step 1: Add failing tests for non-current rename and post-window recovery**

Add tests:

```rust
#[tokio::test(start_paused = true)]
async fn non_current_rename_delayed_pair_does_not_start_fade() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_generation(1).await;
    let show = show_handle();
    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
    command_bus.set_lv1(Some(lv1)).await;
    command_bus.set_fade(Some(fade)).await;
    command_bus.set_show(Some(show.clone())).await;
    seed_show(&show).await;

    let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
    release_lv1.send(()).unwrap();
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(song_3_at(4))));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_before_non_current_rename())));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(2_050)).await;
    tokio::task::yield_now().await;

    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_after_non_current_rename())));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(song_3_at(4))));
    tokio::task::yield_now().await;

    assert!(matches!(fade_rx.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)));
    assert_eq!(fade_starts.load(Ordering::SeqCst), 0);
    assert_no_scene_recall_event(&mut events).await;

    handle.abort();
    command_bus.set_lv1(None).await;
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn valid_recall_after_scene_list_edit_window_starts_fade() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_generation(1).await;
    let show = show_handle();
    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
    command_bus.set_lv1(Some(lv1)).await;
    command_bus.set_fade(Some(fade)).await;
    command_bus.set_show(Some(show.clone())).await;
    seed_show(&show).await;

    let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
    release_lv1.send(()).unwrap();
    arm_recall_state(&event_bus).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_before_non_current_rename())));
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list_after_non_current_rename())));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(500)).await;
    tokio::task::yield_now().await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

    let fade_command = tokio::time::timeout(Duration::from_secs(1), fade_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fade_command.scene.index, 1);
    assert_eq!(fade_command.scene.name, "Intro");
    assert_eq!(fade_starts.load(Ordering::SeqCst), 1);

    handle.abort();
    command_bus.set_lv1(None).await;
    server.await.unwrap();
}
```

- [ ] **Step 2: Run tests to verify they pass or expose missing behavior**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall::actor -- --nocapture
```

Expected: PASS if Task 2 implementation is complete. If a test fails, fix only the gate timing/order issue it exposes.

- [ ] **Step 3: Run broader scene recall tests with timeout**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall -- --nocapture
```

Expected: PASS, no hangs.

---

### Task 4: Verification And Cleanup

**Files:**
- Verify: `src/scene_recall/state.rs`
- Verify: `src/scene_recall/actor.rs`
- Verify: `docs/superpowers/specs/2026-06-10-scene-list-edit-recall-suppression-design.md`

- [ ] **Step 1: Format check**

Run:

```bash
cargo fmt --all -- --check
```

Expected: PASS. If it fails, run `cargo fmt --all`, then repeat the check.

- [ ] **Step 2: Targeted timed tests**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall -- --nocapture
```

Expected: PASS within 20 seconds.

- [ ] **Step 3: Relevant package tests**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control-tauri scene_recall -- --nocapture
```

Expected: PASS within 20 seconds, or report that no tests matched if this package has no matching scene recall tests.

- [ ] **Step 4: Inspect diff**

Run:

```bash
git diff -- src/scene_recall/state.rs src/scene_recall/actor.rs docs/superpowers/specs/2026-06-10-scene-list-edit-recall-suppression-design.md docs/superpowers/plans/2026-06-10-scene-list-edit-recall-suppression.md
```

Expected: Diff only includes the scene-list edit suppression change, tests, and docs.

---

## Self-Review Notes

- Spec coverage: plan covers state-level baseline/identical/changed list behavior, actor-level log-faithful current-scene move, non-current rename, identical resend, real recall, and post-window recovery.
- Placeholder scan: no deferred implementation placeholders are intentionally left.
- Type consistency: plan uses existing `SceneRecallState`, `Lv1Event::SceneListChanged`, `SceneListEntry`, `SceneState`, `AppEventBus`, `AppCommandBus`, and existing test helpers in `src/scene_recall/actor.rs`.
