# Projector Cache And Log Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement approved architecture phases 12-13: build the new projector cache and route UI logs into the projector input path.

**Architecture:** Create a focused `projector/` module that owns projection cache state, applies LV1/fade events, treats show events as stale-show notifications, owns UI log cache, and emits `app-status-changed` at the existing 10 Hz cadence. Logging keeps using `tracing`, but its UI sink sends `UiLogEvent` into the projector instead of appending logs and emitting snapshots itself. Direct command-return snapshots and command direct emits remain transitional and are explicitly deferred to phase 14/15.

**Tech Stack:** Rust, Tauri, Tokio, `tokio::sync::broadcast` for `AppEventBus`, `tokio::sync::mpsc` for UI log input, existing `AppViewState`, `ShellState`, `ShowStateHandle`, and `tracing`.

## Global Constraints

- Use the approved spec: `docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md`.
- Implement only phases 12 and 13 from the approved migration list.
- Do not remove direct command emits or command-returned `AppViewState`; that is phase 14/15.
- Do not eliminate `ShellState`; that is phase 16.
- Do not remove `ActiveCommandBus`; that is phase 17.
- Projector inputs are exactly `AppEventBus` and the logging channel.
- Do not route logs through `AppEventBus`.
- Logging must no longer emit `app-status-changed` directly after this plan.
- Preserve the existing 10 Hz projection cadence.
- Preserve generation guards and stale-runtime cleanup behavior.
- Preserve LV1, fade, scene recall, manual override, abort, overlap, disconnect, lockout, exact scene identity, and generation safety behavior.
- Preserve frontend `AppViewState` schema.
- Preserve Tauri command names and existing React command behavior.
- Preserve `lv1-probe` buildability.

---

## File Structure

- Create `src-tauri/src/projector/cache.rs`: projector-owned cache state and pure-ish cache operations.
- Create `src-tauri/src/projector/runtime.rs`: async projector task, 10 Hz coalescing loop, event/log input handling, and Tauri `app-status-changed` emission.
- Modify `src-tauri/src/projector/mod.rs`: public projector API exports.
- Modify `src-tauri/src/app_state/shell.rs`: expose only the transitional state accessors the projector needs; keep existing snapshot methods for command compatibility.
- Modify `src-tauri/src/app_state/events.rs`: move or share projection event application helpers so the projector cache can apply LV1/fade events without depending on the old shell-state projector loop.
- Modify `src-tauri/src/commands.rs`: replace `spawn_shell_state_projector` with `projector::spawn_projector`; keep initial direct emit and command direct emits for phase 14.
- Modify `src-tauri/src/logging.rs`: keep tracing setup and UI sink, remove `ui_log_projector`, and return/send the UI log receiver for projector ownership.
- Modify `src-tauri/src/ui/mod.rs`: wire logging UI receiver into app/projector lifecycle setup as needed.
- Modify `docs/architecture.md`: document that the projector cache and logging input path are now in place while projector-only command emission remains pending.

---

### Task 1: Add Projector Cache Type And Unit Tests

**Files:**
- Create: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/projector/mod.rs`
- Test: `src-tauri/src/projector/cache.rs`

**Interfaces:**
- Consumes: `crate::app_state::{AppConnectionState, AppViewState, LogSeverity}`, `crate::app_state::view::{AppFadeState, AppLogEntry, ChannelSummary, SceneSummary}`, `crate::lv1::events::Lv1Event`, `crate::fade::events::FadeEvent`, `crate::show::types::ShowSnapshot`, `crate::logging::UiLogEvent`.
- Produces: `ProjectionCache`, `ProjectionCache::new()`, `ProjectionCache::apply_lv1_event(&mut self, &Lv1Event)`, `ProjectionCache::apply_fade_event(&mut self, &FadeEvent)`, `ProjectionCache::append_log(&mut self, UiLogEvent)`, `ProjectionCache::mark_show_stale(&mut self)`, `ProjectionCache::take_show_stale(&mut self) -> bool`, `ProjectionCache::build_snapshot(&mut self, ShowSnapshot) -> AppViewState`.

- [ ] **Step 1: Export view model types needed by the cache**

In `src-tauri/src/app_state/mod.rs`, change the public exports to include the existing view types the projector cache needs:

```rust
pub use view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    SceneSummary,
};
```

- [ ] **Step 2: Write failing projector cache tests**

Create `src-tauri/src/projector/cache.rs` with the tests first. Include the production type skeleton only if the compiler needs names to resolve.

```rust
use std::collections::VecDeque;
use std::path::PathBuf;

use crate::app_state::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    SceneSummary,
};
use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::fade::events::FadeEvent;
use crate::logging::UiLogEvent;
use crate::lv1::events::Lv1Event;
use crate::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot};
use crate::show::types::ShowSnapshot;

pub const MAX_PROJECTOR_LOGS: usize = 200;

#[derive(Debug)]
pub struct ProjectionCache {
    generation: u64,
    lv1_snapshot: Option<Lv1StateSnapshot>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect_state: ReconnectState,
    fade_state: AppFadeState,
    selected_scene_id: Option<String>,
    show_file_path: Option<PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
    last_event_at: Option<String>,
    next_state_version: u64,
    show_stale: bool,
}

impl Default for ProjectionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionCache {
    pub fn new() -> Self {
        Self {
            generation: 0,
            lv1_snapshot: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            pending_lv1_identity: None,
            reconnect_state: ReconnectState::default(),
            fade_state: AppFadeState::Idle,
            selected_scene_id: None,
            show_file_path: None,
            show_file_dirty: false,
            show_file_last_saved_at: None,
            logs: VecDeque::new(),
            next_log_id: 1,
            last_event_at: None,
            next_state_version: 1,
            show_stale: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::SceneState;
    use crate::show::types::SceneConfig;

    fn empty_show() -> ShowSnapshot {
        ShowSnapshot {
            lockout: false,
            scene_configs: Vec::<SceneConfig>::new(),
            cued_scene_id: None,
        }
    }

    #[test]
    fn cache_builds_initial_disconnected_snapshot_with_incrementing_versions() {
        let mut cache = ProjectionCache::new();

        let first = cache.build_snapshot(empty_show());
        let second = cache.build_snapshot(empty_show());

        assert_eq!(first.connection, AppConnectionState::Disconnected);
        assert_eq!(first.show_file_name, "Untitled Show");
        assert_eq!(first.state_version, 1);
        assert_eq!(second.state_version, 2);
    }

    #[test]
    fn cache_applies_lv1_scene_and_topology_events() {
        let mut cache = ProjectionCache::new();

        cache.apply_lv1_event(&Lv1Event::Connected);
        cache.apply_lv1_event(&Lv1Event::SceneChanged(SceneState {
            index: 3,
            name: "Bridge".to_string(),
        }));
        cache.apply_lv1_event(&Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
            group: 1,
            channel: 2,
            name: "Vox".to_string(),
            gain_db: -5.0,
            muted: false,
            pan: Some(0.0),
            balance: None,
            width: None,
        }]));

        let snapshot = cache.build_snapshot(empty_show());

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Bridge");
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels[0].name, "Vox");
    }

    #[test]
    fn cache_applies_fade_state_events() {
        let mut cache = ProjectionCache::new();

        cache.apply_fade_event(&FadeEvent::FadeStarted);
        assert_eq!(cache.build_snapshot(empty_show()).fade_state, AppFadeState::Running);

        cache.apply_fade_event(&FadeEvent::ChannelOverride { group: 1, channel: 1 });
        assert_eq!(cache.build_snapshot(empty_show()).fade_state, AppFadeState::Blocked);

        cache.apply_fade_event(&FadeEvent::FadeCompleted);
        assert_eq!(cache.build_snapshot(empty_show()).fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_owns_bounded_log_entries() {
        let mut cache = ProjectionCache::new();

        for index in 0..(MAX_PROJECTOR_LOGS + 2) {
            cache.append_log(UiLogEvent {
                severity: LogSeverity::Info,
                message: format!("log {index}"),
            });
        }

        let snapshot = cache.build_snapshot(empty_show());

        assert_eq!(snapshot.logs.len(), MAX_PROJECTOR_LOGS);
        assert_eq!(snapshot.logs[0].id, 3);
        assert_eq!(snapshot.logs.last().unwrap().message, "log 201");
    }

    #[test]
    fn show_stale_flag_is_explicitly_consumed() {
        let mut cache = ProjectionCache::new();

        assert!(cache.take_show_stale());
        assert!(!cache.take_show_stale());
        cache.mark_show_stale();
        assert!(cache.take_show_stale());
    }
}
```

- [ ] **Step 3: Run the failing test**

Run: `cargo nextest run -p advanced-show-control projector::cache`

Expected: FAIL because `ProjectionCache` methods are missing.

- [ ] **Step 4: Implement the minimal cache behavior**

Add these methods below `ProjectionCache::new()` in `src-tauri/src/projector/cache.rs`:

```rust
    pub fn apply_lv1_event(&mut self, event: &Lv1Event) {
        match event {
            Lv1Event::Connected => {
                self.ensure_lv1_snapshot().connection = ConnectionStatus::Connected;
                self.reconnect_state.active = false;
            }
            Lv1Event::Disconnected { .. } => {
                let had_connected_identity = self.connected_lv1_identity.is_some();
                self.lv1_snapshot = None;
                self.pending_lv1_identity = None;
                self.reconnect_state.active = had_connected_identity;
                if had_connected_identity {
                    self.reconnect_state.attempt = self.reconnect_state.attempt.saturating_add(1);
                }
            }
            Lv1Event::SceneChanged(scene) => {
                self.ensure_lv1_snapshot().scene = Some(scene.clone());
            }
            Lv1Event::SceneListChanged(scenes) => {
                self.ensure_lv1_snapshot().scene_list = scenes.clone();
                self.mark_show_stale();
            }
            Lv1Event::FaderChanged { group, channel, gain_db } => {
                self.update_channel(*group, *channel, |existing| existing.gain_db = *gain_db);
            }
            Lv1Event::MuteChanged { group, channel, muted } => {
                self.update_channel(*group, *channel, |existing| existing.muted = *muted);
            }
            Lv1Event::PanChanged { group, channel, pan } => {
                self.update_channel(*group, *channel, |existing| existing.pan = Some(*pan));
            }
            Lv1Event::BalanceChanged { group, channel, balance } => {
                self.update_channel(*group, *channel, |existing| existing.balance = Some(*balance));
            }
            Lv1Event::WidthChanged { group, channel, width } => {
                self.update_channel(*group, *channel, |existing| existing.width = Some(*width));
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                self.ensure_lv1_snapshot().channels = channels.clone();
            }
        }
    }

    pub fn apply_fade_event(&mut self, event: &FadeEvent) {
        match event {
            FadeEvent::FadeStarted => self.fade_state = AppFadeState::Running,
            FadeEvent::FadeCompleted | FadeEvent::FadeAborted => self.fade_state = AppFadeState::Idle,
            FadeEvent::ChannelCompleted { .. } => {}
            FadeEvent::ChannelOverride { .. } => self.fade_state = AppFadeState::Blocked,
        }
    }

    pub fn append_log(&mut self, event: UiLogEvent) {
        let entry = AppLogEntry {
            id: self.next_log_id,
            timestamp: crate::time::current_timestamp_millis(),
            severity: event.severity,
            message: event.message,
        };
        self.next_log_id = self.next_log_id.saturating_add(1);
        self.logs.push_back(entry);
        while self.logs.len() > MAX_PROJECTOR_LOGS {
            self.logs.pop_front();
        }
    }

    pub fn mark_show_stale(&mut self) {
        self.show_stale = true;
    }

    pub fn take_show_stale(&mut self) -> bool {
        let stale = self.show_stale;
        self.show_stale = false;
        stale
    }

    pub fn build_snapshot(&mut self, show: ShowSnapshot) -> AppViewState {
        let state_version = self.next_state_version;
        self.next_state_version = self.next_state_version.saturating_add(1);

        let connection = self
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| match snapshot.connection {
                ConnectionStatus::Connecting => AppConnectionState::Connecting,
                ConnectionStatus::Connected => AppConnectionState::Connected,
                ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
            })
            .unwrap_or(AppConnectionState::Disconnected);

        let current_scene = self.lv1_snapshot.as_ref().and_then(|snapshot| {
            snapshot.scene.as_ref().map(|scene| SceneSummary {
                index: scene.index,
                name: scene.name.clone(),
            })
        });

        let scenes = self
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .scene_list
                    .iter()
                    .map(|scene| SceneSummary {
                        index: scene.index,
                        name: scene.name.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let channels = self
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .channels
                    .iter()
                    .map(|channel| ChannelSummary {
                        group: channel.group,
                        channel: channel.channel,
                        name: channel.name.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        AppViewState {
            connection,
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect_state.clone(),
            current_scene,
            scene_count: scenes.len(),
            scenes,
            channel_count: channels.len(),
            channels,
            fade_state: self.fade_state.clone(),
            lockout: show.lockout,
            scene_configs: show.scene_configs,
            cued_scene_id: show.cued_scene_id,
            selected_scene_id: self.selected_scene_id.clone(),
            show_file_name: self
                .show_file_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Untitled Show".to_string()),
            show_file_path: self
                .show_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            logs: self.logs.iter().cloned().collect(),
            last_event_at: self.last_event_at.clone(),
            state_version,
        }
    }

    fn ensure_lv1_snapshot(&mut self) -> &mut Lv1StateSnapshot {
        self.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
    }

    fn update_channel(
        &mut self,
        group: i32,
        channel: i32,
        apply: impl FnOnce(&mut ChannelInfo),
    ) {
        if let Some(existing) = self
            .ensure_lv1_snapshot()
            .channels
            .iter_mut()
            .find(|existing| existing.group == group && existing.channel == channel)
        {
            apply(existing);
        }
    }
```

- [ ] **Step 5: Export the cache module**

Replace `src-tauri/src/projector/mod.rs` with:

```rust
//! AppViewState projection and `app-status-changed` emission.
//!
//! The projector owns coalesced `AppViewState` projection from runtime facts
//! and UI log input. Direct command emits remain transitional until the
//! projector-only phase removes them.

mod cache;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};
```

- [ ] **Step 6: Run tests for the cache**

Run: `cargo nextest run -p advanced-show-control projector::cache`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/app_state/mod.rs src-tauri/src/projector/mod.rs src-tauri/src/projector/cache.rs
git commit -m "refactor: add projector projection cache"
```

---

### Task 2: Add Projector Runtime With Event And Log Inputs

**Files:**
- Create: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/projector/mod.rs`
- Test: `src-tauri/src/projector/runtime.rs`

**Interfaces:**
- Consumes: `ProjectionCache`, `AppEvent`, `AppEventBus` receivers, `UiLogEvent`, `ShowStateHandle`, `ActiveCommandBus`, `ShellState` transitional cleanup hooks.
- Produces: `spawn_projector<R: Runtime>(ProjectorInputs<R>) -> tokio::task::JoinHandle<()>`, `ProjectorInputs<R>`.

- [ ] **Step 1: Write failing runtime tests**

Create `src-tauri/src/projector/runtime.rs` with tests for log coalescing, show-event stale pulls, and unchanged 10 Hz event coalescing. Use the existing Tauri `mock_app` event listener pattern from `src-tauri/src/commands.rs` tests.

```rust
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::app_state::{AppViewState, LogSeverity, ShellState};
use crate::lifecycle::ActiveCommandBus;
use crate::logging::UiLogEvent;
use crate::runtime::events::{log_lagged_subscriber, AppEvent};
use crate::show::events::ShowEvent;

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub shell_state: ShellState,
    pub active_command_bus: ActiveCommandBus,
    pub generation: u64,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: mpsc::Receiver<UiLogEvent>,
    pub start_rx: oneshot::Receiver<()>,
}

pub fn spawn_projector<R: Runtime>(inputs: ProjectorInputs<R>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = inputs;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEventBus;
    use crate::show::events::ShowSnapshotChange;
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, test::mock_app};

    fn spawn_started_projector(
        handle: AppHandle<tauri::Wry>,
        state: ShellState,
        active_command_bus: ActiveCommandBus,
        generation: u64,
        events: broadcast::Receiver<AppEvent>,
        logs: mpsc::Receiver<UiLogEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let (start_tx, start_rx) = oneshot::channel();
        let projector = spawn_projector(ProjectorInputs {
            app: handle,
            shell_state: state,
            active_command_bus,
            generation,
            events,
            logs,
            start_rx,
        });
        let _ = start_tx.send(());
        projector
    }

    #[tokio::test]
    async fn projector_emits_ui_log_entries_from_log_input() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let state = ShellState::new(event_bus.clone());
        let active_command_bus = ActiveCommandBus::default();
        let (log_tx, log_rx) = mpsc::channel(8);
        let received = Arc::new(Mutex::new(Vec::<AppViewState>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: AppViewState = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(
            handle,
            state,
            active_command_bus,
            0,
            event_bus.subscribe(),
            log_rx,
        );

        log_tx
            .send(UiLogEvent {
                severity: LogSeverity::Warning,
                message: "projected log".to_string(),
            })
            .await
            .unwrap();
        tokio::time::sleep(PROJECTOR_INTERVAL + std::time::Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| {
            snapshot
                .logs
                .iter()
                .any(|entry| entry.severity == LogSeverity::Warning && entry.message == "projected log")
        }));
    }

    #[tokio::test]
    async fn show_event_marks_cache_dirty_and_pulls_show_snapshot() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let state = ShellState::new(event_bus.clone());
        state.show.set_lockout(true).await;
        let active_command_bus = ActiveCommandBus::default();
        let (_log_tx, log_rx) = mpsc::channel(8);
        let received = Arc::new(Mutex::new(Vec::<AppViewState>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: AppViewState = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(
            handle,
            state,
            active_command_bus,
            0,
            event_bus.subscribe(),
            log_rx,
        );

        event_bus.publish(AppEvent::Show(ShowEvent::SnapshotChanged {
            reason: ShowSnapshotChange::Lockout,
        }));
        tokio::time::sleep(PROJECTOR_INTERVAL + std::time::Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| snapshot.lockout));
    }
}
```

- [ ] **Step 2: Run failing runtime tests**

Run: `cargo nextest run -p advanced-show-control projector::runtime`

Expected: FAIL because `spawn_projector` does not process inputs or emit snapshots.

- [ ] **Step 3: Implement the projector runtime loop**

Replace the placeholder `spawn_projector` body with:

```rust
pub fn spawn_projector<R: Runtime>(inputs: ProjectorInputs<R>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            app,
            shell_state,
            active_command_bus,
            generation,
            mut events,
            mut logs,
            start_rx,
        } = inputs;

        if start_rx.await.is_err() {
            return;
        }

        tracing::debug!(
            event = "projector_started",
            generation = generation,
            "projector started"
        );

        let mut cache = ProjectionCache::new();
        let mut interval = tokio::time::interval(PROJECTOR_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        let mut dirty = false;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if dirty {
                        let show = shell_state.show.get_snapshot().await;
                        let snapshot = cache.build_snapshot(show);
                        if let Err(err) = app.emit("app-status-changed", &snapshot) {
                            tracing::debug!(
                                event = "app_status_emit_failed",
                                error = %err,
                                "failed to emit app-status-changed from projector"
                            );
                        }
                        dirty = false;
                    }
                }
                received = events.recv() => {
                    match received {
                        Ok(app_event) => {
                            if super::apply_projector_event(
                                &mut cache,
                                &shell_state,
                                generation,
                                &active_command_bus,
                                &app_event,
                            ).await {
                                dirty = true;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(count)) => {
                            dirty = true;
                            log_lagged_subscriber("projector", count);
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                received = logs.recv() => {
                    match received {
                        Some(ui_log) => {
                            cache.append_log(ui_log);
                            dirty = true;
                        }
                        None => break,
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 4: Add the event application function in `projector/mod.rs`**

Extend `src-tauri/src/projector/mod.rs`:

```rust
mod cache;
mod runtime;

use crate::app_state::ShellState;
use crate::lifecycle::ActiveCommandBus;
use crate::lv1::events::Lv1Event;
use crate::runtime::events::AppEvent;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};
pub use runtime::{PROJECTOR_INTERVAL, ProjectorInputs, spawn_projector};

async fn apply_projector_event(
    cache: &mut ProjectionCache,
    state: &ShellState,
    generation: u64,
    active_command_bus: &ActiveCommandBus,
    event: &AppEvent,
) -> bool {
    match event {
        AppEvent::Lv1(event) => {
            if let Lv1Event::SceneListChanged(scenes) = event {
                let _ = state.show.scene_reconciliation_diagnostic(scenes.clone()).await;
            }

            cache.apply_lv1_event(event);

            if matches!(event, Lv1Event::Disconnected { .. }) {
                state.clear_runtime_handles(generation, active_command_bus).await;
            }
            true
        }
        AppEvent::Fade(event) => {
            cache.apply_fade_event(event);
            true
        }
        AppEvent::SceneRecall(_) => false,
        AppEvent::Show(_) => {
            cache.mark_show_stale();
            true
        }
    }
}
```

- [ ] **Step 5: Run runtime tests**

Run: `cargo nextest run -p advanced-show-control projector::runtime`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/projector/mod.rs src-tauri/src/projector/runtime.rs
git commit -m "refactor: add projector runtime input loop"
```

---

### Task 3: Route Logging UI Sink Into Projector Input

**Files:**
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Test: `src-tauri/src/logging.rs`

**Interfaces:**
- Consumes: existing `UiLogLayer` and `UiLogEvent`.
- Produces: `logging::init_logging() -> Result<(WorkerGuard, mpsc::Receiver<UiLogEvent>), Box<dyn Error>>` or equivalent `LoggingRuntime` struct containing `guard` and `ui_logs`.

- [ ] **Step 1: Write failing logging boundary test**

Add this test to `src-tauri/src/logging.rs` tests to lock the phase-13 invariant:

```rust
#[test]
fn logging_module_no_longer_contains_direct_app_status_emit_projector() {
    let source = std::fs::read_to_string("src-tauri/src/logging.rs").unwrap();

    assert!(!source.contains("ui_log_projector"));
    assert!(!source.contains("app.emit(\"app-status-changed\""));
}
```

- [ ] **Step 2: Run failing logging test**

Run: `cargo nextest run -p advanced-show-control logging::tests::logging_module_no_longer_contains_direct_app_status_emit_projector`

Expected: FAIL because `logging.rs` still contains `ui_log_projector` and direct emit code.

- [ ] **Step 3: Change logging initialization to return the UI log receiver**

In `src-tauri/src/logging.rs`, remove `use tauri::{AppHandle, Emitter, Runtime};` and `ShellState` from imports. Replace with:

```rust
use tokio::sync::mpsc;
use tracing_appender::non_blocking::WorkerGuard;

use crate::app_state::LogSeverity;
```

Replace `init_logging` with:

```rust
pub struct LoggingRuntime {
    pub guard: WorkerGuard,
    pub ui_logs: mpsc::Receiver<UiLogEvent>,
}

pub fn init_logging<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<LoggingRuntime, Box<dyn Error>> {
    let log_path = diagnostic_log_path(app);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_env_filter());

    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(LevelFilter::DEBUG);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(true)
        .event_format(BracketedFormat)
        .with_filter(LevelFilter::DEBUG);

    let (ui_tx, ui_logs) = mpsc::channel(64);
    let ui_layer = UiLogLayer { tx: ui_tx }.with_filter(LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .with(ui_layer)
        .try_init()?;

    Ok(LoggingRuntime { guard, ui_logs })
}
```

Delete the old `ui_log_projector` function entirely.

- [ ] **Step 4: Update UI setup to manage the new logging runtime pieces**

In `src-tauri/src/ui/mod.rs`, replace setup lines 20-22 with:

```rust
            let logging_runtime = logging::init_logging(app.handle())?;
            app.manage(logging_runtime.guard);
            app.manage(std::sync::Mutex::new(Some(logging_runtime.ui_logs)));
```

Add the import needed by later tasks:

```rust
use tokio::sync::mpsc;
```

If the compiler needs a named managed type, add this type alias near the top of `ui/mod.rs`:

```rust
pub type UiLogReceiverState = std::sync::Mutex<Option<mpsc::Receiver<logging::UiLogEvent>>>;
```

Then manage it with:

```rust
            app.manage(UiLogReceiverState::new(Some(logging_runtime.ui_logs)));
```

- [ ] **Step 5: Run logging tests**

Run: `cargo nextest run -p advanced-show-control logging::tests`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/logging.rs src-tauri/src/ui/mod.rs
git commit -m "refactor: route ui logs to projector input"
```

---

### Task 4: Replace Shell-State Projector Spawn With Projector Module

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Test: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `projector::spawn_projector`, `projector::ProjectorInputs`, managed `UiLogReceiverState` from `ui/mod.rs`.
- Produces: connected runtime uses the projector module for runtime events and UI logs.

- [ ] **Step 1: Write failing command/projector boundary test**

In `src-tauri/src/commands.rs` tests, update helper `spawn_started_shell_state_projector` to be named `spawn_started_projector` and call `crate::projector::spawn_projector`. Add a regression test that proves a log receiver can be supplied to the runtime projector:

```rust
#[tokio::test]
async fn runtime_projector_accepts_log_input() {
    let app = mock_app();
    let handle = app.handle().clone();
    let event_bus = AppEventBus::default();
    let state = ShellState::new(event_bus.clone());
    let active_command_bus = ActiveCommandBus::default();
    let (log_tx, log_rx) = tokio::sync::mpsc::channel(8);
    let received = Arc::new(Mutex::new(Vec::<AppViewState>::new()));
    let received_events = received.clone();
    handle.listen_any("app-status-changed", move |event| {
        let payload: AppViewState = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        received_events.lock().unwrap().push(payload);
    });

    let projector = crate::projector::spawn_projector(crate::projector::ProjectorInputs {
        app: handle,
        shell_state: state,
        active_command_bus,
        generation: 0,
        events: event_bus.subscribe(),
        logs: log_rx,
        start_rx: {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(());
            rx
        },
    });

    log_tx
        .send(crate::logging::UiLogEvent {
            severity: LogSeverity::Info,
            message: "runtime projector log".to_string(),
        })
        .await
        .unwrap();
    tokio::time::sleep(crate::projector::PROJECTOR_INTERVAL + std::time::Duration::from_millis(60)).await;

    projector.abort();
    assert!(received.lock().unwrap().iter().any(|snapshot| {
        snapshot.logs.iter().any(|entry| entry.message == "runtime projector log")
    }));
}
```

- [ ] **Step 2: Run the failing boundary test**

Run: `cargo nextest run -p advanced-show-control commands::tests::runtime_projector_accepts_log_input`

Expected: FAIL until command tests import/use the new projector module and `LogSeverity` export consistently.

- [ ] **Step 3: Replace projector spawn in `install_connected_runtime`**

In `src-tauri/src/commands.rs`, add imports:

```rust
use crate::projector::{ProjectorInputs, spawn_projector};
use crate::ui::UiLogReceiverState;
```

Update `install_connected_runtime` to accept a `mpsc::Receiver<UiLogEvent>` or retrieve it from managed state at the call site. Prefer passing it explicitly from the connect command call site so the projector has no Tauri-managed-state dependency.

Replace lines 929-938 with:

```rust
    runtime_handles.projector = Some(spawn_projector(ProjectorInputs {
        app: app.clone(),
        shell_state,
        active_command_bus: lifecycle.command_bus_holder(),
        generation,
        events,
        logs: take_ui_log_receiver(app)?,
        start_rx: projector_start_rx,
    }));
```

Add helper in `commands.rs`:

```rust
fn take_ui_log_receiver<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<tokio::sync::mpsc::Receiver<crate::logging::UiLogEvent>, String> {
    let state = app.state::<UiLogReceiverState>();
    let mut guard = state
        .lock()
        .map_err(|_| "Failed to access UI log receiver".to_string())?;
    guard
        .take()
        .ok_or_else(|| "UI log receiver is already attached to the projector".to_string())
}
```

Delete the old `spawn_shell_state_projector` and `apply_projector_event` functions from `commands.rs` after tests have been migrated to the projector module.

- [ ] **Step 4: Preserve initial direct emit behavior**

Keep this block in `install_connected_runtime` unchanged for phase 14:

```rust
    // Emit the initial snapshot before any buffered bus events can be projected.
    emit_snapshot(app, &snapshot);
    let _ = projector_start_tx.send(());
```

Do not remove `emit_snapshot` in this task.

- [ ] **Step 5: Run command projector tests**

Run: `cargo nextest run -p advanced-show-control commands::tests::projector`

Expected: PASS after updating test names from `shell_state_projector` to `projector` where assertions look for tracing/log messages.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/ui/mod.rs
git commit -m "refactor: use projector module for runtime projection"
```

---

### Task 5: Docs, Static Guardrails, And Verification

**Files:**
- Modify: `docs/architecture.md`
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/projector/runtime.rs`
- Test: relevant Rust tests

**Interfaces:**
- Consumes: completed projector/logging boundary from tasks 1-4.
- Produces: documented phase 12-13 boundary and verification evidence.

- [ ] **Step 1: Add architecture doc update**

In `docs/architecture.md`, update the pending-work language near lines 41-43 and 117-119 to say:

```markdown
Low-risk show/app mutations, show-file import/export mapping, UI-requested recall validation/dispatch, projector-cache runtime projection, and projector-owned UI log input route through their target module boundaries. The Tauri adapter still returns/directly emits `AppViewState` snapshots until the projector-only and frontend command-contract phases remove that temporary behavior.

React command-result cleanup, `ShellState` removal, and `ActiveCommandBus` removal are still pending later phases. Projector-only `app-status-changed` emission is also pending: direct command emits remain transitional, but logging no longer emits `app-status-changed` directly.
```

- [ ] **Step 2: Add direct logging emit guardrail**

Keep the `logging_module_no_longer_contains_direct_app_status_emit_projector` test from Task 3. Add this test to `src-tauri/src/projector/runtime.rs`:

```rust
#[test]
fn projector_runtime_is_the_log_projection_emit_owner() {
    let logging_source = std::fs::read_to_string("src-tauri/src/logging.rs").unwrap();
    let projector_source = std::fs::read_to_string("src-tauri/src/projector/runtime.rs").unwrap();

    assert!(!logging_source.contains("app.emit(\"app-status-changed\""));
    assert!(projector_source.contains("app.emit(\"app-status-changed\""));
}
```

- [ ] **Step 3: Run targeted Rust checks**

Run:

```bash
cargo fmt --all -- --check
cargo nextest run -p advanced-show-control projector
cargo nextest run -p advanced-show-control logging
cargo nextest run -p advanced-show-control commands::tests::projector
```

Expected: all PASS.

- [ ] **Step 4: Run broader verification**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
cargo build -p advanced-show-control --bin lv1-probe
npm --prefix ui run typecheck
npm run tauri -- build
```

Expected: all PASS. The known non-fatal Tauri bundle identifier warning may still appear.

- [ ] **Step 5: Inspect git status and diff**

Run:

```bash
git status --short
git diff --stat
git diff -- docs/architecture.md src-tauri/src/projector src-tauri/src/logging.rs src-tauri/src/commands.rs src-tauri/src/ui/mod.rs src-tauri/src/app_state/mod.rs
```

Expected: only phase 12-13 files are changed.

- [ ] **Step 6: Commit**

```bash
git add docs/architecture.md src-tauri/src/projector src-tauri/src/logging.rs src-tauri/src/commands.rs src-tauri/src/ui/mod.rs src-tauri/src/app_state/mod.rs
git commit -m "docs: describe projector cache and log input"
```

---

## Self-Review Notes

- Spec coverage: this plan covers phase 12 projector cache/event accumulation/stale show snapshot/log cache/10 Hz emission and phase 13 logging input reroute. It deliberately excludes phase 14 projector-only removal of direct command emits, phase 15 React command contracts, phase 16 `ShellState` removal, and phase 17 `ActiveCommandBus` removal.
- Placeholder scan: no incomplete-marker tokens or unspecified edge-case steps remain.
- Type consistency: `ProjectionCache`, `ProjectorInputs`, `spawn_projector`, `UiLogEvent`, and `LoggingRuntime` are named consistently across tasks.
- Safety check: LV1/fade safety behavior remains event projection only; command, recall, fade, and LV1 protocol paths are not changed.
