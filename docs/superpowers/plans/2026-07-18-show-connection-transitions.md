# Show-Owned Connection Metadata Transitions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #56 by replacing lifecycle-composed metadata mutations with atomic successful and failed connection transitions owned by Show.

**Architecture:** Add three complete Show commands backed by pure atomic `ShowState` transitions. The actor publishes at most one full connection projection per transition, and lifecycle selects exactly one transition only after fresh LV1 validation.

**Tech Stack:** Rust 2024, Tokio actors, `AppEventBus`, cargo-nextest

## Global Constraints

- Execute after issue #58 and before issue #55; assume issue #53's `Result<(), String>` settings reply is already present.
- Successful metadata remains after fresh connected-state validation and before accepted scene-peer installation.
- Failed normal/startup connection clears connected identity; failed reconnect preserves it.
- Show owns mutation, aggregate `changed`, and projection publication; lifecycle must not compose low-level fields.
- Before advancing to issue #55, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Define Atomic State Transitions With Pure Tests

**Files:**
- Modify: `src-tauri/src/show/state.rs:1-150`
- Test: `src-tauri/src/show/state.rs`

**Interfaces:**
- Consumes: `Lv1SystemIdentity` and `ReconnectState`.
- Produces: `ShowState::complete_lv1_connection`, `ShowState::fail_lv1_connection`, and `ShowState::fail_lv1_reconnect`, each returning aggregate `bool changed`.

- [ ] **Step 1: Write failing pure transition tests**

Replace the empty state test module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn identity(uuid: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: Some(uuid.to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50_000,
        }
    }

    #[test]
    fn complete_connection_sets_identity_and_clears_transient_metadata_atomically() {
        let next = identity("new");
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            pending_lv1_identity: Some(next.clone()),
            reconnect: ReconnectState { active: true, attempt: 3 },
            ..Default::default()
        };

        assert!(state.complete_lv1_connection(next.clone()));
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, Some(next.clone()));
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert!(!state.complete_lv1_connection(next));
    }

    #[test]
    fn failed_connection_clears_all_connection_metadata() {
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            pending_lv1_identity: Some(identity("new")),
            reconnect: ReconnectState { active: true, attempt: 2 },
            ..Default::default()
        };

        assert!(state.fail_lv1_connection());
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, None);
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert!(!state.fail_lv1_connection());
    }

    #[test]
    fn failed_reconnect_preserves_connected_identity_and_clears_transient_metadata() {
        let connected = identity("old");
        let mut state = ShowState {
            connected_lv1_identity: Some(connected.clone()),
            pending_lv1_identity: Some(identity("new")),
            reconnect: ReconnectState { active: true, attempt: 4 },
            ..Default::default()
        };

        assert!(state.fail_lv1_reconnect());
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, Some(connected));
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert!(!state.fail_lv1_reconnect());
    }
}
```

- [ ] **Step 2: Run pure tests red**

Run `cargo nextest run -p advanced-show-control show::state`. Expected: compile failures for the three missing methods.

- [ ] **Step 3: Implement minimal atomic state methods**

Replace the four low-level connection metadata mutators with:

```rust
pub(crate) fn complete_lv1_connection(&mut self, identity: Lv1SystemIdentity) -> bool {
    let reconnect = ReconnectState::default();
    let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
        || self.pending_lv1_identity.is_some()
        || self.reconnect != reconnect;
    self.connected_lv1_identity = Some(identity);
    self.pending_lv1_identity = None;
    self.reconnect = reconnect;
    changed
}

pub(crate) fn fail_lv1_connection(&mut self) -> bool {
    let reconnect = ReconnectState::default();
    let changed = self.connected_lv1_identity.is_some()
        || self.pending_lv1_identity.is_some()
        || self.reconnect != reconnect;
    self.connected_lv1_identity = None;
    self.pending_lv1_identity = None;
    self.reconnect = reconnect;
    changed
}

pub(crate) fn fail_lv1_reconnect(&mut self) -> bool {
    let reconnect = ReconnectState::default();
    let changed = self.pending_lv1_identity.is_some() || self.reconnect != reconnect;
    self.pending_lv1_identity = None;
    self.reconnect = reconnect;
    changed
}
```

Do not update `last_event_at`; runtime disconnect events retain ownership of that timestamp.

- [ ] **Step 4: Run pure tests green**

Run `cargo nextest run -p advanced-show-control show::state`. Expected: all three transition tests pass.

---

### Task 2: Expose Complete Show Mailbox Transitions

**Files:**
- Modify: `src-tauri/src/show/commands.rs:1-70`
- Modify: `src-tauri/src/show/actor.rs:190-344`
- Modify: `src-tauri/src/show/handle.rs:31-204`
- Test: `src-tauri/src/show/handle.rs`

**Interfaces:**
- Consumes: Task 1's three pure state methods.
- Produces: `ShowCommand::{CompleteLv1Connection, FailLv1Connection, FailLv1Reconnect}`.

- [ ] **Step 1: Write failing mailbox tests for atomic state and one projection**

Add the imports `ConnectCommandResult` and `ShowCommandResult`, then add:

```rust
#[tokio::test]
async fn complete_connection_publishes_one_full_projection_and_noop_publishes_none() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    let identity = Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50_000,
    };

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::CompleteLv1Connection {
        identity: identity.clone(),
        reply: Some(reply),
    })
    .await
    .unwrap();
    assert_eq!(rx.await.unwrap(), ConnectCommandResult { changed: true });

    let AppEvent::Show(ShowEvent::StateChanged { reason, state }) =
        events.recv().await.unwrap()
    else {
        panic!("expected Show projection");
    };
    assert_eq!(reason, ShowProjectionReason::ConnectionMetadata);
    assert_eq!(state.connected_lv1_identity, Some(identity.clone()));
    assert_eq!(state.pending_lv1_identity, None);
    assert_eq!(state.reconnect, ReconnectState::default());
    assert!(events.try_recv().is_err());

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::CompleteLv1Connection {
        identity,
        reply: Some(reply),
    })
    .await
    .unwrap();
    assert_eq!(rx.await.unwrap(), ConnectCommandResult { changed: false });
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn failed_connection_clears_connected_identity_with_one_projection() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    let identity = Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50_000,
    };
    show.send(ShowCommand::CompleteLv1Connection { identity, reply: None })
        .await
        .unwrap();
    recv_show_event(&mut events, ShowProjectionReason::ConnectionMetadata).await;

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::FailLv1Connection { reply: Some(reply) })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap(), ShowCommandResult { changed: true });
    let AppEvent::Show(ShowEvent::StateChanged { state, .. }) = events.recv().await.unwrap() else {
        panic!("expected Show projection");
    };
    assert_eq!(state.connected_lv1_identity, None);
    assert_eq!(state.pending_lv1_identity, None);
    assert_eq!(state.reconnect, ReconnectState::default());
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn failed_reconnect_preserves_connected_identity_and_noop_publishes_none() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    let identity = Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50_000,
    };
    show.send(ShowCommand::CompleteLv1Connection {
        identity: identity.clone(),
        reply: None,
    })
    .await
    .unwrap();
    recv_show_event(&mut events, ShowProjectionReason::ConnectionMetadata).await;

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::FailLv1Reconnect { reply: Some(reply) })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap(), ShowCommandResult { changed: false });
    assert!(events.try_recv().is_err());

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::InitialProjectionState { reply }).await.unwrap();
    assert_eq!(rx.await.unwrap().connected_lv1_identity, Some(identity));
}
```

- [ ] **Step 2: Run mailbox tests red**

Run `cargo nextest run -p advanced-show-control show::handle`. Expected: compile failures for the three new command variants.

- [ ] **Step 3: Replace low-level command variants**

In `show/commands.rs`, replace `SetPendingLv1Identity`, `EstablishConnectedLv1Identity`, `ClearConnectedLv1Identity`, and `SetReconnectState` with:

```rust
CompleteLv1Connection {
    identity: Lv1SystemIdentity,
    reply: Option<oneshot::Sender<ConnectCommandResult>>,
},
FailLv1Connection {
    reply: Option<oneshot::Sender<ShowCommandResult>>,
},
FailLv1Reconnect {
    reply: Option<oneshot::Sender<ShowCommandResult>>,
},
```

Remove the now-unused `ReconnectState` command-module import.

- [ ] **Step 4: Add the three actor arms**

Replace the four low-level actor arms with:

```rust
ShowCommand::CompleteLv1Connection { identity, reply } => {
    let changed = state.complete_lv1_connection(identity);
    publish_if_changed(
        event_bus,
        ShowProjectionReason::ConnectionMetadata,
        state,
        changed,
    );
    if let Some(reply) = reply {
        let _ = reply.send(crate::show::ConnectCommandResult { changed });
    }
}
ShowCommand::FailLv1Connection { reply } => {
    let changed = state.fail_lv1_connection();
    publish_if_changed(
        event_bus,
        ShowProjectionReason::ConnectionMetadata,
        state,
        changed,
    );
    if let Some(reply) = reply {
        let _ = reply.send(ShowCommandResult { changed });
    }
}
ShowCommand::FailLv1Reconnect { reply } => {
    let changed = state.fail_lv1_reconnect();
    publish_if_changed(
        event_bus,
        ShowProjectionReason::ConnectionMetadata,
        state,
        changed,
    );
    if let Some(reply) = reply {
        let _ = reply.send(ShowCommandResult { changed });
    }
}
```

Update existing Show actor tests that seed connected identity to use `CompleteLv1Connection`; remove low-level reconnect seeding and rely on Task 1's pure test for transient reconnect-state coverage.

- [ ] **Step 5: Run Show tests green**

Run `cargo nextest run -p advanced-show-control show`. Expected: all state and actor mailbox tests pass with at most one projection per transition.

---

### Task 3: Route Lifecycle Through Complete Transitions

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:344-536,1344-1461,1687-1723`
- Test: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Consumes: Task 2's three Show commands.
- Produces: lifecycle-private `complete_lv1_connection_metadata` and `fail_lv1_connection_metadata` helpers that each send one Show command.

- [ ] **Step 1: Replace connected metadata composition**

Replace `apply_connected_lv1_metadata` with:

```rust
async fn complete_lv1_connection_metadata(
    &self,
    identity: crate::connection_state::Lv1SystemIdentity,
) -> Result<ConnectCommandResult, AppCommandError> {
    let (reply, rx) = oneshot::channel();
    self.show
        .send(ShowCommand::CompleteLv1Connection {
            identity,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)?;
    rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)
}
```

- [ ] **Step 2: Replace failed metadata composition**

Replace `apply_failed_connect_metadata` with:

```rust
async fn fail_lv1_connection_metadata(
    &self,
    failure_mode: ConnectFailureMode,
) -> Result<(), AppCommandError> {
    let (reply, rx) = oneshot::channel();
    let command = match failure_mode {
        ConnectFailureMode::ClearConnectedIdentity => {
            ShowCommand::FailLv1Connection { reply: Some(reply) }
        }
        ConnectFailureMode::PreserveConnectedIdentity => {
            ShowCommand::FailLv1Reconnect { reply: Some(reply) }
        }
    };
    self.show
        .send(command)
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)?;
    let _ = rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?;
    Ok(())
}
```

Update both production and test-only connection paths to call these helpers only after their existing fresh LV1 snapshot check. Keep failure order: clear runtime, send failed transition, log failure, return. Keep success order: validate, send complete transition, continue to generation acceptance.

- [ ] **Step 3: Update lifecycle tests to the complete commands**

Rename metadata helper tests to `complete_connection_metadata_is_applied_atomically` and `failed_reconnect_metadata_preserves_connected_identity`. Seed connected identity using `ShowCommand::CompleteLv1Connection`, call the relevant helper, request `InitialProjectionState`, and assert the three connection fields. Update `attempt_reconnect_uses_stored_connected_identity` to seed with `CompleteLv1Connection`.

- [ ] **Step 4: Run lifecycle and issue verification**

Run:

```bash
cargo nextest run -p advanced-show-control show lifecycle
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: Show transition and lifecycle ordering tests pass; searches find no remaining low-level metadata command variants.

- [ ] **Step 5: Run the issue smoke checkpoint**

Run `make smoke`, then read `logs/debug-smoke-report.txt`. Expected: authoritative suite success.

- [ ] **Step 6: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/show src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/show src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: move connection transitions into show"
```
