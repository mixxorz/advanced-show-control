# Backend Event And Command Bus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a backend event bus and acknowledged command bus so LV1, fade, ShellState, and future automation modules communicate through shared runtime abstractions instead of concrete cross-module handles.

**Architecture:** Add a `runtime` module to the core crate containing `AppEventBus`, `AppCommandBus`, and a runtime dispatcher. `Lv1Actor` and `FadeEngine` remain local Tokio actors; events are broadcast with `tokio::sync::broadcast`, while commands route through an `mpsc` dispatcher and return `oneshot` acknowledgements. Tauri owns `ShellState` and runtime handles, and uses an event-bus projector task to update UI state.

**Tech Stack:** Rust 2024, Tokio `mpsc`/`broadcast`/`oneshot`, Tauri commands, existing LV1 and fade modules, Cargo workspace tests.

**Implementation Note:** During execution, the command routing design was simplified by user request. The queued `RuntimeDispatcher` described in early tasks was removed and replaced with direct `AppCommandBus` target routing. `AppCommandBus` now owns shared optional LV1/fade targets, clones handles under a short lock, drops the lock before awaits, and publishes command failures through `AppEventBus`.

---

## File Structure

- Create `src/runtime/mod.rs`: exports runtime bus modules.
- Create `src/runtime/events.rs`: defines `AppEvent`, `AutomationEvent`, `AppEventBus`, lag helper, and unit tests.
- Create `src/runtime/commands.rs`: defines `AppCommand`, `AppCommandBus`, `AppCommandError`, command helper methods, and unit tests for unavailable handlers.
- Create `src/runtime/dispatcher.rs`: owns runtime actor handles and routes commands to LV1/fade actors.
- Modify `src/lib.rs`: exports `runtime`.
- Modify `src/lv1/state.rs`: replace local subscriber list with `AppEventBus` publishing while preserving actor command behavior.
- Modify `src/fade/engine.rs`: consume LV1 events from `AppEventBus`, publish fade events to `AppEventBus`, and send LV1 commands through `AppCommandBus`.
- Modify `src/fade/types.rs`: make fade command handling acknowledged with reply channels.
- Modify `src-tauri/src/app_state/events.rs`: add fade-event application methods.
- Modify `src-tauri/src/app_state/mod.rs`: expose event projection helpers as needed.
- Modify `src-tauri/src/commands.rs`: route UI commands through `AppCommandBus`, spawn actors with bus dependencies, and move ShellState projection to event-bus subscriber task.
- Modify `src-tauri/src/main.rs`: manage runtime buses/dispatcher state in Tauri.
- Modify `tests/fade_engine.rs`: update integration tests to use the buses.
- Create `tests/runtime_bus.rs`: integration tests for bus publishing and command routing.
- Create `docs/architecture.md`: backend architecture doc covering actor ownership, event flow, command flow, ShellState projection, and future automation boundaries.

## Task 1: Add Event Bus

**Files:**
- Create: `src/runtime/mod.rs`
- Create: `src/runtime/events.rs`
- Modify: `src/lib.rs`
- Test: `src/runtime/events.rs`

- [ ] **Step 1: Write failing event bus tests**

Create `src/runtime/mod.rs`:

```rust
pub mod events;
```

Create `src/runtime/events.rs` with the tests first:

```rust
use tokio::sync::broadcast;

use crate::fade::types::FadeEvent;
use crate::lv1::events::Lv1Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationEvent {
    RuleTriggered { rule_id: String },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    Automation(AutomationEvent),
    CommandFailed { command: String, message: String },
}

#[derive(Clone)]
pub struct AppEventBus {
    tx: broadcast::Sender<AppEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::SceneState;

    #[tokio::test]
    async fn publish_succeeds_without_subscribers() {
        let bus = AppEventBus::new(16);

        let sent = bus.publish(AppEvent::CommandFailed {
            command: "test".to_string(),
            message: "no subscriber".to_string(),
        });

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn subscriber_receives_published_event() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 7,
            name: "Chorus".to_string(),
        })));

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
                assert_eq!(scene.index, 7);
                assert_eq!(scene.name, "Chorus");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn lagged_subscriber_reports_missed_events() {
        let bus = AppEventBus::new(1);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::CommandFailed {
            command: "first".to_string(),
            message: "one".to_string(),
        });
        bus.publish(AppEvent::CommandFailed {
            command: "second".to_string(),
            message: "two".to_string(),
        });

        let err = rx.recv().await.unwrap_err();
        assert!(matches!(err, broadcast::error::RecvError::Lagged(1)));
    }
}
```

Modify `src/lib.rs`:

```rust
pub mod fade;
pub mod lv1;
pub mod osc;
pub mod runtime;
pub mod vegas;
```

- [ ] **Step 2: Run event bus tests and verify failure**

Run: `cargo test runtime::events --lib`

Expected: FAIL with errors that `AppEventBus::new`, `publish`, and `subscribe` are not defined.

- [ ] **Step 3: Implement event bus methods**

Update `src/runtime/events.rs` to add the implementation above the test module:

```rust
impl AppEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for AppEventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

pub fn log_lagged_subscriber(name: &str, count: u64) {
    eprintln!("{name} event subscriber lagged and missed {count} events");
}
```

- [ ] **Step 4: Run event bus tests and verify pass**

Run: `cargo test runtime::events --lib`

Expected: PASS, all `runtime::events` tests pass.

- [ ] **Step 5: Commit event bus**

```bash
git add src/lib.rs src/runtime/mod.rs src/runtime/events.rs
git commit -m "feat: add app event bus"
```

## Task 2: Add Command Bus And Dispatcher Skeleton

**Files:**
- Create: `src/runtime/commands.rs`
- Create: `src/runtime/dispatcher.rs`
- Modify: `src/runtime/mod.rs`
- Test: `src/runtime/commands.rs`

- [ ] **Step 1: Write failing command bus tests**

Update `src/runtime/mod.rs`:

```rust
pub mod commands;
pub mod dispatcher;
pub mod events;
```

Create `src/runtime/commands.rs`:

```rust
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::fade::types::FadeConfig;
use crate::lv1::types::Lv1StateSnapshot;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AppCommandError {
    #[error("app command dispatcher is closed")]
    DispatcherClosed,
    #[error("app command reply channel is closed")]
    ReplyChannelClosed,
    #[error("LV1 actor is unavailable")]
    Lv1Unavailable,
    #[error("fade engine is unavailable")]
    FadeUnavailable,
    #[error("command failed: {0}")]
    CommandFailed(String),
}

pub enum AppCommand {
    GetLv1State {
        reply: oneshot::Sender<Result<Lv1StateSnapshot, AppCommandError>>,
    },
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    StartFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAllFades {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishFadeNow {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}

#[derive(Clone)]
pub struct AppCommandBus {
    tx: mpsc::Sender<AppCommand>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeTarget, FadeConfig};

    #[tokio::test]
    async fn closed_dispatcher_returns_error() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let bus = AppCommandBus::new(tx);

        let err = bus.abort_all_fades().await.unwrap_err();

        assert_eq!(err, AppCommandError::DispatcherClosed);
    }

    #[tokio::test]
    async fn start_fade_sends_acknowledged_command() {
        let (tx, mut rx) = mpsc::channel(1);
        let bus = AppCommandBus::new(tx);
        let config = FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                target_db: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        let task = tokio::spawn(async move { bus.start_fade(config).await });

        match rx.recv().await.unwrap() {
            AppCommand::StartFade { config, reply } => {
                assert_eq!(config.targets[0].channel, 1);
                reply.send(Ok(())).unwrap();
            }
            _ => panic!("unexpected command"),
        }

        assert_eq!(task.await.unwrap(), Ok(()));
    }
}
```

- [ ] **Step 2: Run command bus tests and verify failure**

Run: `cargo test runtime::commands --lib`

Expected: FAIL with errors that `AppCommandBus::new`, `abort_all_fades`, and `start_fade` are not defined.

- [ ] **Step 3: Implement command bus methods**

Add this implementation to `src/runtime/commands.rs` above the test module:

```rust
impl AppCommandBus {
    pub fn new(tx: mpsc::Sender<AppCommand>) -> Self {
        Self { tx }
    }

    pub async fn get_lv1_state(&self) -> Result<Lv1StateSnapshot, AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::GetLv1State { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn set_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::SetGain {
                group,
                channel,
                gain_db,
                reply,
            })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::StartFade { config, reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn abort_all_fades(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::AbortAllFades { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn finish_fade_now(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::FinishFadeNow { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }
}
```

Create `src/runtime/dispatcher.rs`:

```rust
use tokio::sync::mpsc;

use crate::fade::engine::FadeEngineHandle;
use crate::lv1::messages::Lv1ActorError;
use crate::lv1::handle::Lv1ActorHandle;
use crate::runtime::commands::{AppCommand, AppCommandError};
use crate::runtime::events::{AppEvent, AppEventBus};

pub struct RuntimeDispatcher {
    rx: mpsc::Receiver<AppCommand>,
    event_bus: AppEventBus,
    lv1: Option<Lv1ActorHandle>,
    fade: Option<FadeEngineHandle>,
}

impl RuntimeDispatcher {
    pub fn new(rx: mpsc::Receiver<AppCommand>, event_bus: AppEventBus) -> Self {
        Self {
            rx,
            event_bus,
            lv1: None,
            fade: None,
        }
    }

    pub fn set_lv1(&mut self, lv1: Option<Lv1ActorHandle>) {
        self.lv1 = lv1;
    }

    pub fn set_fade(&mut self, fade: Option<FadeEngineHandle>) {
        self.fade = fade;
    }

    pub async fn run(mut self) {
        while let Some(command) = self.rx.recv().await {
            self.handle(command).await;
        }
    }

    async fn handle(&mut self, command: AppCommand) {
        match command {
            AppCommand::GetLv1State { reply } => {
                let result = match &self.lv1 {
                    Some(lv1) => Ok(lv1.get_state().await),
                    None => Err(AppCommandError::Lv1Unavailable),
                };
                let _ = reply.send(result);
            }
            AppCommand::SetGain { group, channel, gain_db, reply } => {
                let result = match &self.lv1 {
                    Some(lv1) => lv1.set_gain(group, channel, gain_db).await.map_err(map_lv1_error),
                    None => Err(AppCommandError::Lv1Unavailable),
                };
                publish_failure(&self.event_bus, "set_gain", &result);
                let _ = reply.send(result);
            }
            AppCommand::StartFade { config, reply } => {
                let result = match &self.fade {
                    Some(fade) => fade.start_fade(config).await,
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "start_fade", &result);
                let _ = reply.send(result);
            }
            AppCommand::AbortAllFades { reply } => {
                let result = match &self.fade {
                    Some(fade) => fade.abort_all().await,
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "abort_all_fades", &result);
                let _ = reply.send(result);
            }
            AppCommand::FinishFadeNow { reply } => {
                let result = match &self.fade {
                    Some(fade) => fade.finish_now().await,
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "finish_fade_now", &result);
                let _ = reply.send(result);
            }
        }
    }
}

fn map_lv1_error(error: Lv1ActorError) -> AppCommandError {
    AppCommandError::CommandFailed(error.to_string())
}

fn publish_failure(event_bus: &AppEventBus, command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        event_bus.publish(AppEvent::CommandFailed {
            command: command.to_string(),
            message: error.to_string(),
        });
    }
}
```

- [ ] **Step 4: Run command bus tests and verify pass**

Run: `cargo test runtime::commands --lib`

Expected: PASS, all command bus unit tests pass.

- [ ] **Step 5: Commit command bus skeleton**

```bash
git add src/runtime/mod.rs src/runtime/commands.rs src/runtime/dispatcher.rs
git commit -m "feat: add app command bus"
```

## Task 3: Publish LV1 Actor Events Through Event Bus

**Files:**
- Modify: `src/lv1/state.rs`
- Test: `src/lv1/state.rs`

- [ ] **Step 1: Write failing LV1 event bus test**

Add this test to `src/lv1/state.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
async fn actor_publishes_scene_changes_to_event_bus() {
    use crate::lv1::events::Lv1Event;
    use crate::runtime::events::{AppEvent, AppEventBus};

    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();
    let mut state = ActorState::new(bus.clone());

    handle_message(
        &mut state,
        &crate::osc::OscMessage {
            address: "/Notify/CurSceneIndex".to_string(),
            args: vec![crate::osc::OscArg::Int(3)],
        },
    );
    handle_message(
        &mut state,
        &crate::osc::OscMessage {
            address: "/Notify/Scene/Name".to_string(),
            args: vec![crate::osc::OscArg::String("Bridge".to_string())],
        },
    );

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
            assert_eq!(scene.index, 3);
            assert_eq!(scene.name, "Bridge");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 2: Run LV1 state test and verify failure**

Run: `cargo test actor_publishes_scene_changes_to_event_bus --lib`

Expected: FAIL because `ActorState::new` and `spawn_actor` do not accept `AppEventBus` yet.

- [ ] **Step 3: Replace LV1 subscriber list with event bus publishing**

In `src/lv1/state.rs`, add imports:

```rust
use crate::runtime::events::{AppEvent, AppEventBus};
```

Change `Lv1ActorHandle::subscribe` to remove it entirely after downstream callers are migrated in later tasks. For this task, leave it temporarily if needed by existing tests, but stop using it from new runtime code.

Change `ActorState`:

```rust
struct ActorState {
    connection: ConnectionStatus,
    scene: Option<SceneState>,
    scene_list: Vec<SceneListEntry>,
    channels: Vec<ChannelInfo>,
    scene_buf: SceneBuffer,
    last_ping: Instant,
    event_bus: AppEventBus,
}
```

Change constructor and fanout:

```rust
impl ActorState {
    fn new(event_bus: AppEventBus) -> Self {
        Self {
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            scene_buf: SceneBuffer::default(),
            last_ping: Instant::now(),
            event_bus,
        }
    }

    fn snapshot(&self) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: self.connection.clone(),
            scene: self.scene.clone(),
            scene_list: self.scene_list.clone(),
            channels: self.channels.clone(),
        }
    }

    fn fan_out(&self, event: Lv1Event) {
        self.event_bus.publish(AppEvent::Lv1(event));
    }
}
```

Change `spawn_actor`:

```rust
pub fn spawn_actor(host: String, port: u16, event_bus: AppEventBus) -> Lv1ActorHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_actor(host, port, event_bus, cmd_rx));
    Lv1ActorHandle { tx: cmd_tx }
}
```

Change `run_actor` signature and constructor:

```rust
async fn run_actor(host: String, port: u16, event_bus: AppEventBus, mut cmd_rx: mpsc::Receiver<Lv1Command>) {
    let mut state = ActorState::new(event_bus);
    // existing loop remains
}
```

In `drain_commands_for` and command-drain sections, remove `Subscribe` handling only after deleting the `Subscribe` variant in a later cleanup step. Until then, answer `Subscribe` by doing nothing and letting the receiver close.

- [ ] **Step 4: Update tests and call sites for new spawn_actor signature**

For each `spawn_actor(host, port)` call in tests and `src/main.rs`, pass an event bus:

```rust
let event_bus = lv1_scene_fade_utility::runtime::events::AppEventBus::default();
let handle = spawn_actor(host.clone(), port, event_bus);
```

For tests already inside the core crate, use:

```rust
let event_bus = crate::runtime::events::AppEventBus::default();
let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);
```

- [ ] **Step 5: Run LV1 tests and verify pass**

Run: `cargo test lv1::state --lib`

Expected: PASS, all LV1 state tests pass.

- [ ] **Step 6: Commit LV1 event publishing**

```bash
git add src/lv1/state.rs src/main.rs tests/fade_engine.rs
git commit -m "refactor: publish lv1 events through app bus"
```

## Task 4: Acknowledge Fade Commands And Publish Fade Events

**Files:**
- Modify: `src/fade/types.rs`
- Modify: `src/fade/engine.rs`
- Test: `tests/fade_engine.rs`

- [ ] **Step 1: Write failing fade command acknowledgement test**

Modify `tests/fade_engine.rs` imports:

```rust
use lv1_scene_fade_utility::runtime::commands::{AppCommandBus, AppCommandError};
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};
use tokio::sync::mpsc;
```

Add this test near the top after helpers:

```rust
#[tokio::test]
async fn engine_acknowledges_abort_all() {
    let event_bus = AppEventBus::default();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    let command_bus = AppCommandBus::new(cmd_tx);
    let engine = spawn_engine(command_bus, event_bus);

    let result = engine.abort_all().await;

    assert_eq!(result, Ok(()));
    assert!(cmd_rx.try_recv().is_err());
}
```

This test will need adjustment after `spawn_engine` changes; it intentionally drives the new API.

- [ ] **Step 2: Run fade test and verify failure**

Run: `cargo test --test fade_engine engine_acknowledges_abort_all`

Expected: FAIL because `spawn_engine` still expects `Lv1ActorHandle`, and fade handle methods return `()`.

- [ ] **Step 3: Make fade commands acknowledged**

Update `src/fade/types.rs`:

```rust
use tokio::sync::{mpsc, oneshot};

use crate::fade::curve::FadeCurve;
use crate::runtime::commands::AppCommandError;

#[derive(Debug, Clone)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAll {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishNow {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    Subscribe {
        tx: mpsc::Sender<FadeEvent>,
    },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}
```

- [ ] **Step 4: Update fade engine handle and constructor**

In `src/fade/engine.rs`, update imports:

```rust
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::runtime::commands::{AppCommandBus, AppCommandError};
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
```

Change handle methods:

```rust
impl FadeEngineHandle {
    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::StartFade { config, reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn abort_all(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::AbortAll { reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn finish_now(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::FinishNow { reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn subscribe(&self) -> mpsc::Receiver<FadeEvent> {
        let (tx, rx) = mpsc::channel(64);
        let _ = self.tx.send(FadeCommand::Subscribe { tx }).await;
        rx
    }
}
```

Change constructor:

```rust
pub fn spawn_engine(command_bus: AppCommandBus, event_bus: AppEventBus) -> FadeEngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_engine(command_bus, event_bus, cmd_rx));
    FadeEngineHandle { tx: cmd_tx }
}
```

Change `EngineState::fan_out` to publish both legacy local subscribers and global bus events by passing bus where called:

```rust
fn fan_out(&mut self, event_bus: &AppEventBus, event: FadeEvent) {
    event_bus.publish(AppEvent::Fade(event.clone()));
    self.subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());
}
```

- [ ] **Step 5: Update run_engine to use command bus and event bus**

Change signature:

```rust
async fn run_engine(command_bus: AppCommandBus, event_bus: AppEventBus, mut cmd_rx: mpsc::Receiver<FadeCommand>) {
    let mut app_events = event_bus.subscribe();
    let mut state = EngineState::new();
    let mut tick_interval: Option<tokio::time::Interval> = None;
    // existing loop
}
```

Replace `lv1.get_state().await` with:

```rust
let snapshot = match command_bus.get_lv1_state().await {
    Ok(snapshot) => snapshot,
    Err(error) => {
        let _ = reply.send(Err(error));
        continue;
    }
};
```

Replace `lv1.set_gain(...).await` with:

```rust
let _ = command_bus.set_gain(ch.group, ch.channel, new_db).await;
```

For `FinishNow`, send final gains through `command_bus.set_gain` and reply `Ok(())` after the loop completes.

Replace LV1 event branch with AppEvent handling:

```rust
app_event = app_events.recv() => {
    match app_event {
        Ok(AppEvent::Lv1(Lv1Event::FaderChanged { group, channel, gain_db })) => {
            if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel) {
                if state.channels[pos].is_override(gain_db) {
                    state.fan_out(&event_bus, FadeEvent::ChannelOverride { group, channel });
                    state.channels.remove(pos);
                    state.fan_out(&event_bus, FadeEvent::ChannelCancelled { group, channel });

                    if !state.is_active() {
                        tick_interval = None;
                    }
                }
            }
        }
        Ok(AppEvent::Lv1(Lv1Event::Disconnected)) => {
            if state.is_active() {
                state.cancel_all_in_place();
                tick_interval = None;
                state.fan_out(&event_bus, FadeEvent::FadeAborted);
            }
        }
        Ok(_) => {}
        Err(broadcast::error::RecvError::Lagged(count)) => {
            log_lagged_subscriber("fade-engine", count);
        }
        Err(broadcast::error::RecvError::Closed) => break,
    }
}
```

- [ ] **Step 6: Run fade tests and verify pass**

Run: `cargo test --test fade_engine`

Expected: PASS after updating all `spawn_engine` call sites to use `AppCommandBus` and `AppEventBus` and after adding command-dispatch test scaffolding where needed.

- [ ] **Step 7: Commit fade bus integration**

```bash
git add src/fade/types.rs src/fade/engine.rs tests/fade_engine.rs
git commit -m "refactor: route fade engine through app buses"
```

## Task 5: Wire Runtime Buses Into Tauri Shell

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/events.rs`
- Test: `src-tauri/src/app_state/events_tests.rs`

- [ ] **Step 1: Add failing fade event projection test**

Add to `src-tauri/src/app_state/events_tests.rs`:

```rust
#[tokio::test]
async fn fade_events_update_fade_state() {
    use lv1_scene_fade_utility::fade::types::FadeEvent;
    use super::view::AppFadeState;

    let state = ShellState::default();

    let started = state.apply_fade_event(&FadeEvent::FadeStarted).await;
    assert_eq!(started.fade_state, AppFadeState::Running);

    let completed = state.apply_fade_event(&FadeEvent::FadeCompleted).await;
    assert_eq!(completed.fade_state, AppFadeState::Idle);

    let aborted = state.apply_fade_event(&FadeEvent::FadeAborted).await;
    assert_eq!(aborted.fade_state, AppFadeState::Idle);
}
```

- [ ] **Step 2: Run Tauri app_state test and verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri fade_events_update_fade_state`

Expected: FAIL because `apply_fade_event` does not exist.

- [ ] **Step 3: Implement fade event projection**

In `src-tauri/src/app_state/events.rs`, add:

```rust
use lv1_scene_fade_utility::fade::types::FadeEvent;
```

Add to `impl ShellState`:

```rust
pub async fn apply_fade_event(&self, event: &FadeEvent) -> AppViewState {
    let mut inner = self.inner.lock().await;
    match event {
        FadeEvent::FadeStarted => {
            inner.fade_state = super::view::AppFadeState::Running;
            inner.push_log(LogSource::Fade, LogSeverity::Info, "Fade started".to_string());
        }
        FadeEvent::FadeCompleted => {
            inner.fade_state = super::view::AppFadeState::Idle;
            inner.push_log(LogSource::Fade, LogSeverity::Info, "Fade completed".to_string());
        }
        FadeEvent::FadeAborted => {
            inner.fade_state = super::view::AppFadeState::Idle;
            inner.push_log(LogSource::Fade, LogSeverity::Warning, "Fade aborted".to_string());
        }
        FadeEvent::ChannelOverride { group, channel } => {
            inner.fade_state = super::view::AppFadeState::Blocked;
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Warning,
                format!("Fade channel override detected: group={group} channel={channel}"),
            );
        }
        FadeEvent::ChannelCancelled { group, channel } => {
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Warning,
                format!("Fade channel cancelled: group={group} channel={channel}"),
            );
        }
    }
    snapshot_from_inner(&inner)
}
```

- [ ] **Step 4: Add Tauri-managed runtime state**

In `src-tauri/src/main.rs`, manage event bus and command bus sender alongside `ShellState`:

```rust
use lv1_scene_fade_utility::runtime::events::AppEventBus;
```

In `main()` before `invoke_handler`:

```rust
.manage(AppEventBus::default())
```

In `src-tauri/src/commands.rs`, add imports:

```rust
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use tokio::sync::broadcast;
```

For the first implementation, keep `ShellState.handles` as the runtime handle store and create the command dispatcher in `connect_lv1`; follow-up cleanup can move it fully out of `ShellState`.

- [ ] **Step 5: Route abort/finish commands through command bus**

Change `abort_all_fades` and `finish_fade_now` to accept `State<'_, AppCommandBus>` after `AppCommandBus` is managed. Implement:

```rust
#[tauri::command]
pub async fn abort_all_fades(command_bus: State<'_, AppCommandBus>) -> Result<(), String> {
    command_bus.abort_all_fades().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn finish_fade_now(command_bus: State<'_, AppCommandBus>) -> Result<(), String> {
    command_bus.finish_fade_now().await.map_err(|err| err.to_string())
}
```

If `AppCommandBus` cannot be managed before dispatcher setup, manage an `Arc<Mutex<Option<AppCommandBus>>>` wrapper in this task and return `"Command bus is unavailable"` when missing. Keep the API acknowledged either way.

- [ ] **Step 6: Add ShellState projector task**

In `src-tauri/src/commands.rs`, add helper:

```rust
fn spawn_shell_state_projector(app: AppHandle, generation: u64, event_bus: AppEventBus) {
    let mut events = event_bus.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(event)) => {
                    let state = app.state::<ShellState>();
                    if let Some(snapshot) = state.apply_lv1_event_for_generation(generation, &event).await {
                        let _ = app.emit("app-status-changed", &snapshot);
                    }
                }
                Ok(AppEvent::Fade(event)) => {
                    let state = app.state::<ShellState>();
                    let snapshot = state.apply_fade_event(&event).await;
                    let _ = app.emit("app-status-changed", &snapshot);
                }
                Ok(AppEvent::CommandFailed { command, message }) => {
                    eprintln!("command failed: {command}: {message}");
                }
                Ok(AppEvent::Automation(_)) => {}
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("shell-state-projector", count);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
```

Remove the direct `lv1.subscribe().await` bridge once LV1 actor publishing is active.

- [ ] **Step 7: Run Tauri tests and verify pass**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS, all Tauri package tests pass.

- [ ] **Step 8: Commit Tauri runtime wiring**

```bash
git add src-tauri/src/main.rs src-tauri/src/commands.rs src-tauri/src/app_state/events.rs src-tauri/src/app_state/events_tests.rs
git commit -m "refactor: project shell state from app events"
```

## Task 6: Remove Legacy Actor Subscriber APIs

**Files:**
- Modify: `src/lv1/messages.rs`
- Modify: `src/lv1/state.rs`
- Modify: `src/fade/types.rs`
- Modify: `src/fade/engine.rs`
- Test: `tests/runtime_bus.rs`

- [ ] **Step 1: Write integration test for no-subscriber event flow**

Create `tests/runtime_bus.rs`:

```rust
use lv1_scene_fade_utility::lv1::events::Lv1Event;
use lv1_scene_fade_utility::lv1::types::SceneState;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};

#[tokio::test]
async fn app_event_bus_carries_lv1_events_without_actor_subscriber_api() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
        index: 4,
        name: "Outro".to_string(),
    })));

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
            assert_eq!(scene.index, 4);
            assert_eq!(scene.name, "Outro");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 2: Remove `Subscribe` from LV1 commands**

In `src/lv1/messages.rs`, remove:

```rust
Subscribe {
    tx: mpsc::Sender<Lv1Event>,
},
```

Remove the now-unused `mpsc` import from `src/lv1/messages.rs` if only `oneshot` remains.

In `src/lv1/state.rs`, remove `Lv1ActorHandle::subscribe` and all `Lv1Command::Subscribe` match arms.

- [ ] **Step 3: Remove `Subscribe` from fade commands if tests no longer need local fade receivers**

If all fade event assertions have been moved to `AppEventBus`, remove from `src/fade/types.rs`:

```rust
Subscribe { tx: mpsc::Sender<FadeEvent> },
```

Then remove `FadeEngineHandle::subscribe` and local `subscribers` from `EngineState`. If any existing integration test still needs local fade events, keep this API temporarily and document it as test/backward-compatible local observation only.

- [ ] **Step 4: Run full Rust test suite and verify pass**

Run: `cargo test --workspace`

Expected: PASS, all workspace tests pass.

- [ ] **Step 5: Commit legacy subscriber cleanup**

```bash
git add src/lv1/messages.rs src/lv1/state.rs src/fade/types.rs src/fade/engine.rs tests/runtime_bus.rs
git commit -m "refactor: remove legacy actor subscriber routing"
```

## Task 7: Add Backend Architecture Document

**Files:**
- Create: `docs/architecture.md`
- Test: documentation review by inspection

- [ ] **Step 1: Write architecture document**

Create `docs/architecture.md`:

```markdown
# Backend Architecture

## Overview

The backend is a local actor-based runtime. Long-running modules communicate through two shared abstractions: `AppEventBus` for facts that already happened and `AppCommandBus` for acknowledged requests.

## Core Actors

`Lv1Actor` owns the TCP connection to LV1 and the latest LV1 state mirror. It receives routed commands such as `GetLv1State`, `SetGain`, and `SetMute`, then publishes LV1 events such as connection changes, scene changes, fader changes, mute changes, and channel topology changes.

`FadeEngine` owns active fade timing. It receives routed fade commands, listens to LV1 events from `AppEventBus` for manual overrides and disconnects, sends LV1 gain commands through `AppCommandBus`, and publishes fade lifecycle events.

`ShellState` owns the UI projection. It is updated by a projector task that consumes `AppEventBus` events and emits `AppViewState` snapshots to the Tauri frontend.

## Events Versus Commands

Events are facts and are broadcast to any interested consumer. Publishers do not know or wait for subscribers.

Commands are requests and are routed to exactly one owning handler through the runtime dispatcher. Commands return acknowledgements so callers can report errors and avoid racing UI refreshes against unprocessed work.

## Event Flow

LV1 TCP messages enter `Lv1Actor`. The actor updates its local LV1 mirror and publishes `AppEvent::Lv1`. `ShellStateProjector`, `FadeEngine`, and future automation modules subscribe independently. If a subscriber lags, it logs the missed event count and continues. Recovery and replay are intentionally deferred.

## Command Flow

Tauri commands, the fade engine, and future automation modules send requests through `AppCommandBus`. The runtime dispatcher owns concrete actor handles and routes each command to the correct actor. If the target actor is unavailable, the command returns an `AppCommandError`.

## Automation Boundary

Automation modules should depend only on `AppEventBus` and `AppCommandBus`. They should observe events, evaluate rules, send commands, and publish automation events. They should not store concrete LV1 or fade actor handles.

## Non-Goals

The runtime is in-process only. There is no durable event store, replay system, distributed message bus, or plugin runtime in the initial architecture.
```

- [ ] **Step 2: Review architecture doc against implemented files**

Check that the doc mentions the exact implemented modules: `runtime::events`, `runtime::commands`, `runtime::dispatcher`, `lv1::state`, `fade::engine`, and Tauri `ShellState` projection.

Expected: the doc accurately describes implemented ownership boundaries and does not mention unimplemented recovery/replay as current behavior.

- [ ] **Step 3: Commit architecture doc**

```bash
git add docs/architecture.md
git commit -m "docs: add backend architecture overview"
```

## Task 8: Final Verification

**Files:**
- Verify: whole workspace

- [ ] **Step 1: Format Rust code**

Run: `cargo fmt --all`

Expected: command exits 0 and formats all Rust workspace files.

- [ ] **Step 2: Run workspace tests**

Run: `cargo test --workspace`

Expected: PASS, all tests pass.

- [ ] **Step 3: Run frontend build if package scripts are available**

Run: `npm --prefix ui run build`

Expected: PASS, frontend build completes. If dependencies are missing, install only if that is standard for the repo; otherwise record the exact failure.

- [ ] **Step 4: Inspect git status**

Run: `git status --short`

Expected: clean working tree after all task commits, or only intentional uncommitted artifacts explicitly listed.

- [ ] **Step 5: Commit final formatting fixes if needed**

If `cargo fmt --all` changed files after the previous commits:

```bash
git add <formatted-files>
git commit -m "style: format backend bus changes"
```

If no files changed, skip this step.

## Self-Review Notes

- Spec coverage: event bus, command bus, runtime dispatcher, ShellState projection, lag logging, architecture doc, command/event split, and non-goals are each covered by tasks.
- Scope: automatic scene recall trigger is intentionally not implemented in this plan; it should be a follow-up feature on top of the bus foundation.
- Placeholders: no `TBD` or unspecified implementation tasks remain.
- Type consistency: `AppEventBus`, `AppCommandBus`, `AppCommand`, `AppCommandError`, and `RuntimeDispatcher` names are consistent across tasks.
