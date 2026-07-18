# Private Settings Persistence Interfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #53 by making the persisted settings document implementation-private and narrowing remembered-identity command replies to `Result<(), String>`.

**Architecture:** Move the private on-disk schema into `settings/state.rs`, preserving its flattened JSON shape. Keep state-level changed detection for write avoidance, but discard it at the actor mailbox boundary while retaining generation-safe successful no-ops.

**Tech Stack:** Rust 2024, Serde JSON, Tokio actors, cargo-nextest

## Global Constraints

- Execute after issue #57 and before issue #59.
- `SettingsCommandResult { changed }` remains the frontend result for `ReplaceSettings`.
- Remembered identity remains private, generation guarded, absent from `AppSettings` and `SettingsEvent`, and preserved during public settings replacement.
- A failed write must not mutate in-memory remembered identity; a stale generation must return `Ok(())` without writing.
- Before advancing to issue #59, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Privatize The Document And Narrow The Mailbox Contract

**Files:**
- Modify: `src-tauri/src/settings/mod.rs:8-16`
- Modify: `src-tauri/src/settings/types.rs:175-188`
- Modify: `src-tauri/src/settings/state.rs:1-162`
- Modify: `src-tauri/src/settings/commands.rs:9-32`
- Modify: `src-tauri/src/settings/actor.rs:7-145,190-506`
- Modify: `src-tauri/src/lifecycle/mod.rs:606-625,1028-1042`
- Test: `src-tauri/src/settings/state.rs`
- Test: `src-tauri/src/settings/actor.rs`
- Test: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Consumes: `SettingsState::set_last_connected_lv1(...) -> Result<bool, String>` and `RuntimeGeneration::if_current`.
- Produces: `SettingsCommand::SetLastConnectedLv1 { reply: oneshot::Sender<Result<(), String>> }`; `PersistedSettings` has no module-facade visibility.

- [ ] **Step 1: Characterize the persisted schema**

Add this pure state test before moving the type:

```rust
#[test]
fn remembered_identity_uses_the_existing_flat_private_schema() {
    let dir = temp_settings_dir("flat-private-schema");
    let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");
    let mut state = SettingsState::load(dir.clone());
    state
        .set_last_connected_lv1(identity)
        .expect("remembered identity should save");

    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("settings.json"))
            .expect("settings document should exist"),
    )
    .expect("settings document should be JSON");
    assert_eq!(document["lastConnectedLv1"]["uuid"], "uuid-1");
    assert_eq!(document["lastConnectedLv1"]["host"], "LV1-FOH");
    assert!(document.get("settings").is_none());
}
```

Run `cargo nextest run -p advanced-show-control settings::state`; expected: the characterization test passes against the current schema.

- [ ] **Step 2: Write failing actor contract and safety tests**

Change existing `SetLastConnectedLv1` assertions to expect `()` rather than `SettingsCommandResult`. Add:

```rust
#[tokio::test]
async fn actor_treats_stale_remembered_identity_update_as_successful_noop() {
    let event_bus = AppEventBus::default();
    let dir = temp_settings_dir("stale-remembered-identity");
    let (handle, task, _) = build_settings_actor(dir, event_bus);
    task.spawn();
    let runtime_generation = crate::runtime::generation::RuntimeGeneration::default();
    runtime_generation.advance().await;
    let (reply, rx) = oneshot::channel();
    handle
        .send(SettingsCommand::SetLastConnectedLv1 {
            identity: identity("uuid-new", "LV1-FOH", "192.168.1.36"),
            runtime_generation,
            expected_generation: 0,
            reply,
        })
        .await
        .expect("stale identity command should send");
    assert_eq!(rx.await.expect("stale identity reply should arrive"), Ok(()));

    let (reply, rx) = oneshot::channel();
    handle.send(SettingsCommand::GetLastConnectedLv1 { reply }).await.unwrap();
    assert_eq!(rx.await.unwrap(), None);
}
```

Add this failed-write actor test to prove in-memory state changes only after a successful write:

```rust
#[tokio::test]
async fn actor_preserves_remembered_identity_when_replacement_write_fails() {
    let event_bus = AppEventBus::default();
    let dir = temp_settings_dir("failed-remembered-identity-write");
    let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
    task.spawn();
    let runtime_generation = runtime_generation();
    let original = identity("uuid-old", "LV1-FOH", "192.168.1.35");

    let (reply, rx) = oneshot::channel();
    handle
        .send(SettingsCommand::SetLastConnectedLv1 {
            identity: original.clone(),
            runtime_generation: runtime_generation.clone(),
            expected_generation: 0,
            reply,
        })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap(), Ok(()));

    std::fs::remove_file(dir.join("settings.json")).unwrap();
    std::fs::remove_dir(&dir).unwrap();
    std::fs::write(&dir, "not a directory").unwrap();

    let (reply, rx) = oneshot::channel();
    handle
        .send(SettingsCommand::SetLastConnectedLv1 {
            identity: identity("uuid-new", "LV1-FOH", "192.168.1.36"),
            runtime_generation,
            expected_generation: 0,
            reply,
        })
        .await
        .unwrap();
    assert!(rx.await.unwrap().is_err());

    let (reply, rx) = oneshot::channel();
    handle.send(SettingsCommand::GetLastConnectedLv1 { reply }).await.unwrap();
    assert_eq!(rx.await.unwrap(), Some(original));
}
```

- [ ] **Step 3: Run actor tests red**

Run:

```bash
cargo nextest run -p advanced-show-control settings::actor
```

Expected: compilation/assertion failures show `SetLastConnectedLv1` still replies with `SettingsCommandResult`.

- [ ] **Step 4: Move the persisted type into state implementation**

Remove `pub(crate) use types::PersistedSettings;` from `settings/mod.rs` and remove `PersistedSettings` from `settings/types.rs`. In `settings/state.rs`, import Serde and define:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    #[serde(flatten)]
    settings: AppSettings,
    last_connected_lv1: Option<Lv1SystemIdentity>,
}

impl PersistedSettings {
    fn normalized(mut self) -> Self {
        self.settings = self.settings.normalized();
        self
    }
}
```

Change `use super::{AppSettings, PersistedSettings};` to `use super::AppSettings;`. Keep `SettingsState::set_last_connected_lv1` returning `Result<bool, String>` so unchanged identities still avoid writes.

- [ ] **Step 5: Narrow the command reply and actor mapping**

Change the enum field to:

```rust
SetLastConnectedLv1 {
    identity: Lv1SystemIdentity,
    runtime_generation: RuntimeGeneration,
    expected_generation: u64,
    reply: oneshot::Sender<Result<(), String>>,
},
```

Replace the actor result mapping with:

```rust
let result = runtime_generation
    .if_current(expected_generation, || {
        state.set_last_connected_lv1(identity).map(|_changed| ())
    })
    .await
    .unwrap_or(Ok(()));
let _ = reply.send(result);
```

Keep `ReplaceSettings` and `SettingsCommandResult` unchanged.

- [ ] **Step 6: Simplify the lifecycle reply without changing generation inputs**

End `remember_last_connected_lv1` with:

```rust
rx.await
    .map_err(|_| "Settings reply channel is closed".to_string())?
```

Remove only the old `.map(|_| ())`. Continue passing the current `RuntimeGeneration`, the accepted `expected_generation`, and the exact connected identity.

- [ ] **Step 7: Run focused verification**

Run:

```bash
cargo nextest run -p advanced-show-control settings lifecycle
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: state schema, actor privacy, failed-write, stale-generation, and lifecycle persistence tests pass.

- [ ] **Step 8: Run the issue smoke checkpoint**

Run `make smoke`, then read `logs/debug-smoke-report.txt`. Expected: authoritative suite success; rerun after any required fix.

- [ ] **Step 9: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/settings src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/settings src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: narrow settings persistence interfaces"
```
