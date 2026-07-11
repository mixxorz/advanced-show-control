# Startup Auto-Connect Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore safe startup auto-connect using remembered LV1 identity stored privately in `settings.json`, remove obsolete preference code, and verify the live path through `make smoke`.

**Architecture:** `SettingsActor` owns a persisted document containing public `AppSettings` plus private remembered connection identity. Lifecycle reads and updates that identity through explicit mailbox commands, refreshes show-owned discovery, selects only a UUID or unique exact-hostname match, and uses the existing generation-guarded connection path. The frontend remains listener-driven; the debug smoke app verifies same-process disconnect and startup auto-connect through production commands.

**Tech Stack:** Rust, Tokio actors and oneshot replies, Serde JSON, Tauri, React/TypeScript, Vitest, debug hardware smoke runner

## Global Constraints

- Do not migrate or read the obsolete `preferences.json` file.
- If `settings.json` cannot be parsed as the current persisted document, discard it in memory and use default public settings with no remembered identity.
- Keep remembered identity private from `AppViewState` and frontend settings replacement.
- Match startup targets by UUID first, then one exact trimmed hostname; never match by address and port.
- Preserve generation guards, stale-runtime rejection, disconnect behavior, and the rule that no fader command is sent before a valid connected runtime is installed.
- Failed, cancelled, ambiguous, unmatched, or stale connection attempts must not replace remembered identity.
- A remembered-identity write failure must be user-visible but must not tear down or report failure for an already established connection.
- Use `tracing` with stable `event` fields and do not duplicate lifecycle connection success logs.
- Rust coverage must use pure unit tests or actor tests through mailboxes, `AppEventBus`, and tracing; do not inspect side-effecting actor internals.

---

## File Map

- `src-tauri/src/settings/types.rs`: public settings types plus crate-private persisted settings document.
- `src-tauri/src/settings/state.rs`: load/write the complete document while preserving private identity across public settings replacements.
- `src-tauri/src/settings/commands.rs`: explicit get/store remembered identity mailbox commands.
- `src-tauri/src/settings/actor.rs`: command handling, persistence error reporting, and actor tests.
- `src-tauri/src/settings/mod.rs`: export remembered identity types only where lifecycle needs them.
- `src-tauri/src/lifecycle/mod.rs`: pure safe-target selection, discovery refresh, connection orchestration, and post-connect identity storage.
- `src-tauri/src/connection_preferences.rs`: delete obsolete standalone preference storage.
- `src-tauri/src/lib.rs`: remove the obsolete module export.
- `src-tauri/src/ui/mod.rs`: no behavioral change expected; confirms production settings directory remains lifecycle input.
- `src-tauri/src/ui/debug.rs`: no behavioral change expected; confirms debug settings directory uses the debug app config.
- `ui/src/debug/main.tsx`: add same-process startup auto-connect smoke scenario.
- `docs/architecture.md`: document private connection metadata ownership in `SettingsActor`.

---

### Task 1: Persist Remembered Identity Through SettingsActor

**Files:**
- Modify: `src-tauri/src/settings/types.rs`
- Modify: `src-tauri/src/settings/state.rs`
- Modify: `src-tauri/src/settings/commands.rs`
- Modify: `src-tauri/src/settings/actor.rs`
- Modify: `src-tauri/src/settings/mod.rs`
- Modify: `docs/architecture.md:89`
- Delete: `src-tauri/src/connection_preferences.rs`
- Modify: `src-tauri/src/lib.rs:1`

**Interfaces:**
- Consumes: `crate::connection_state::Lv1SystemIdentity`.
- Produces: `SettingsCommand::GetLastConnectedLv1 { reply: oneshot::Sender<Option<Lv1SystemIdentity>> }`.
- Produces: `SettingsCommand::SetLastConnectedLv1 { identity: Lv1SystemIdentity, reply: oneshot::Sender<Result<SettingsCommandResult, String>> }`.
- Preserves: `SettingsCommand::GetSettings` and `SettingsCommand::ReplaceSettings` public behavior.

- [ ] **Step 1: Add failing settings document and actor tests**

In `src-tauri/src/settings/state.rs`, add pure unit tests that construct a temporary settings directory and prove:

```rust
#[test]
fn invalid_persisted_document_resets_public_and_private_settings() {
    let dir = temp_settings_dir("invalid-document");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.json"), r#"{"lastConnectedLv1":42}"#).unwrap();

    let state = SettingsState::load(dir);

    assert_eq!(state.settings(), AppSettings::default());
    assert_eq!(state.last_connected_lv1(), None);
}

#[test]
fn replacing_public_settings_preserves_remembered_identity() {
    let dir = temp_settings_dir("preserve-identity");
    let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");
    let mut state = SettingsState::load(dir.clone());
    state.set_last_connected_lv1(identity.clone()).unwrap();

    state
        .replace_settings(AppSettings {
            auto_save_sessions: true,
            ..Default::default()
        })
        .unwrap();

    let reloaded = SettingsState::load(dir);
    assert!(reloaded.settings().auto_save_sessions);
    assert_eq!(reloaded.last_connected_lv1(), Some(identity));
}
```

In `src-tauri/src/settings/actor.rs`, add an actor test that uses only mailbox commands:

```rust
#[tokio::test]
async fn actor_stores_and_returns_last_connected_lv1() {
    let event_bus = AppEventBus::default();
    let dir = temp_settings_dir("connected-identity");
    let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
    task.spawn();
    let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");

    let (reply, rx) = oneshot::channel();
    handle
        .send(SettingsCommand::SetLastConnectedLv1 {
            identity: identity.clone(),
            reply,
        })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap().unwrap(), SettingsCommandResult { changed: true });

    let (reply, rx) = oneshot::channel();
    handle
        .send(SettingsCommand::GetLastConnectedLv1 { reply })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap(), Some(identity));
    assert!(dir.join("settings.json").exists());
}
```

Add local `identity` and `temp_settings_dir` helpers in each test module rather than exposing production test helpers.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control settings
```

Expected: compilation fails because `last_connected_lv1`, `set_last_connected_lv1`, `GetLastConnectedLv1`, and `SetLastConnectedLv1` do not exist.

- [ ] **Step 3: Add the private persisted document and state operations**

In `src-tauri/src/settings/types.rs`, add a crate-private document that keeps the existing JSON keys flat:

```rust
use crate::connection_state::Lv1SystemIdentity;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersistedSettings {
    #[serde(flatten)]
    pub settings: AppSettings,
    pub last_connected_lv1: Option<Lv1SystemIdentity>,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            settings: AppSettings::default(),
            last_connected_lv1: None,
        }
    }
}

impl PersistedSettings {
    pub fn normalized(mut self) -> Self {
        self.settings = self.settings.normalized();
        self
    }
}
```

Do not add `last_connected_lv1` to `AppSettings`.

In `src-tauri/src/settings/state.rs`, replace the separate `settings` field with `document: PersistedSettings`, deserialize `PersistedSettings`, and write the entire document for either update. Add:

```rust
pub fn last_connected_lv1(&self) -> Option<Lv1SystemIdentity> {
    self.document.last_connected_lv1.clone()
}

pub fn set_last_connected_lv1(
    &mut self,
    identity: Lv1SystemIdentity,
) -> Result<bool, String> {
    if self.document.last_connected_lv1.as_ref() == Some(&identity) {
        return Ok(false);
    }
    let mut updated = self.document.clone();
    updated.last_connected_lv1 = Some(identity);
    write_settings_file(&self.file_path, &updated)?;
    self.document = updated;
    Ok(true)
}
```

Make `replace_settings` clone the document, replace only `updated.settings`, write successfully, then assign it. This preserves in-memory state on write failure. Update `write_settings_file` to serialize `PersistedSettings`; keep the current `settings_write_failed` logging.

- [ ] **Step 4: Add explicit actor commands**

In `src-tauri/src/settings/commands.rs`, import `Lv1SystemIdentity` and add:

```rust
GetLastConnectedLv1 {
    reply: oneshot::Sender<Option<Lv1SystemIdentity>>,
},
SetLastConnectedLv1 {
    identity: Lv1SystemIdentity,
    reply: oneshot::Sender<Result<SettingsCommandResult, String>>,
},
```

In `src-tauri/src/settings/actor.rs`, handle both variants. `GetLastConnectedLv1` returns the private state without publishing an event. `SetLastConnectedLv1` calls the state method and returns `SettingsCommandResult`. On success, do not publish `SettingsEvent::StateChanged` because projected public settings did not change. Log only failures here through the existing state write error; do not add a duplicate success log.

- [ ] **Step 5: Remove obsolete preference storage and update architecture**

Delete `src-tauri/src/connection_preferences.rs` and remove:

```rust
pub mod connection_preferences;
```

from `src-tauri/src/lib.rs`.

Update the `SettingsActor` paragraph in `docs/architecture.md` to state that it also owns private remembered LV1 identity in `settings.json`, that this metadata is not projected as public settings, and that lifecycle accesses it through explicit settings commands.

- [ ] **Step 6: Run settings tests and formatting**

Run:

```bash
cargo nextest run -p advanced-show-control settings
cargo fmt --all -- --check
```

Expected: all settings tests pass and formatting reports no differences.

- [ ] **Step 7: Commit the settings ownership change**

```bash
git add src-tauri/src/settings src-tauri/src/lib.rs src-tauri/src/connection_preferences.rs docs/architecture.md
git commit -m "feat: persist LV1 identity in settings"
```

---

### Task 2: Restore Safe Startup Target Selection

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:261-592`

**Interfaces:**
- Consumes: Task 1's `SettingsCommand::GetLastConnectedLv1` and `SettingsCommand::SetLastConnectedLv1`.
- Consumes: `ShowCommand::RefreshLv1Discovery` and `ShowCommand::InitialProjectionState`.
- Produces: `fn startup_auto_connect_target(remembered: &Lv1SystemIdentity, systems: &[DiscoveredLv1System]) -> Option<Lv1SystemIdentity>`.
- Preserves: `connect_lv1_system`, `attempt_reconnect_lv1`, and `connect_to_identity` command results and generation behavior.

- [ ] **Step 1: Add failing pure matching tests**

In the existing `lifecycle::tests` module, add helpers for available and unavailable discovered systems and tests with these assertions:

```rust
#[test]
fn startup_target_prefers_uuid_over_hostname() {
    let remembered = identity(Some("uuid-1"), Some("LV1-FOH"), "192.168.1.35");
    let systems = vec![
        system(Some("uuid-2"), Some("LV1-FOH"), "10.0.0.20", DiscoveredLv1Status::Available),
        system(Some("uuid-1"), Some("Renamed"), "10.0.0.21", DiscoveredLv1Status::Available),
    ];

    assert_eq!(
        startup_auto_connect_target(&remembered, &systems)
            .unwrap()
            .address,
        "10.0.0.21"
    );
}

#[test]
fn startup_target_uses_one_exact_trimmed_hostname() {
    let remembered = identity(None, Some(" LV1-FOH "), "192.168.1.35");
    let systems = vec![system(
        None,
        Some("LV1-FOH"),
        "10.0.0.20",
        DiscoveredLv1Status::Available,
    )];

    assert_eq!(
        startup_auto_connect_target(&remembered, &systems)
            .unwrap()
            .address,
        "10.0.0.20"
    );
}

#[test]
fn startup_target_rejects_ambiguous_or_address_only_matches() {
    let remembered = identity(None, Some("LV1-FOH"), "10.0.0.20");
    let duplicate_hosts = vec![
        system(None, Some("LV1-FOH"), "10.0.0.20", DiscoveredLv1Status::Available),
        system(None, Some("LV1-FOH"), "10.0.0.21", DiscoveredLv1Status::Available),
    ];
    assert!(startup_auto_connect_target(&remembered, &duplicate_hosts).is_none());

    let address_only = vec![system(
        None,
        Some("Different"),
        "10.0.0.20",
        DiscoveredLv1Status::Available,
    )];
    assert!(startup_auto_connect_target(&remembered, &address_only).is_none());
}
```

Add a fourth test proving unavailable UUID and hostname matches are ignored.

- [ ] **Step 2: Run matching tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control startup_target
```

Expected: compilation fails because `startup_auto_connect_target` does not exist.

- [ ] **Step 3: Implement the minimal pure selector**

Near the lifecycle connection methods, add:

```rust
fn startup_auto_connect_target(
    remembered: &crate::connection_state::Lv1SystemIdentity,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Option<crate::connection_state::Lv1SystemIdentity> {
    let available: Vec<_> = systems
        .iter()
        .filter(|system| {
            system.status == crate::connection_state::DiscoveredLv1Status::Available
        })
        .collect();

    if let Some(uuid) = remembered.uuid.as_deref() {
        if let Some(system) = available
            .iter()
            .find(|system| system.identity.uuid.as_deref() == Some(uuid))
        {
            return Some(system.identity.clone());
        }
    }

    let host = remembered.host.as_deref()?.trim();
    if host.is_empty() {
        return None;
    }
    let mut matches = available.into_iter().filter(|system| {
        system.identity.host.as_deref().map(str::trim) == Some(host)
    });
    let target = matches.next()?.identity.clone();
    matches.next().is_none().then_some(target)
}
```

Do not add IP/port fallback.

- [ ] **Step 4: Add failing lifecycle actor tests for settings-backed startup decisions**

Extend lifecycle test setup so a test can receive a real `SettingsHandle` backed by a temporary directory. Add tests through actor mailboxes that prove:

```rust
#[tokio::test]
async fn startup_without_remembered_identity_does_not_advance_generation() {
    let app = mock_app();
    let lifecycle = lifecycle_for_test(AppEventBus::default());
    let before = lifecycle.active_generation().await;

    let result = lifecycle
        .startup_auto_connect_lv1(app.handle().clone())
        .await
        .unwrap();

    assert!(!result.changed);
    assert_eq!(lifecycle.active_generation().await, before);
}
```

Extract this private helper so deterministic tests can exercise the post-discovery decision without opening a network discovery socket:

```rust
async fn startup_auto_connect_with_discovered<R: Runtime>(
    &self,
    app: AppHandle<R>,
    remembered: crate::connection_state::Lv1SystemIdentity,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Result<ConnectCommandResult, String>
```

Add a test that stores remembered identity through `SettingsCommand`, passes duplicate matching systems to `startup_auto_connect_with_discovered`, and asserts both the generation and remembered identity remain unchanged:

```rust
#[tokio::test]
async fn ambiguous_startup_match_preserves_generation_and_remembered_identity() {
    let app = mock_app();
    let lifecycle = lifecycle_for_test(AppEventBus::default());
    let remembered = identity(None, Some("LV1-FOH"), "192.168.1.35");
    set_last_connected_lv1(&lifecycle.settings, remembered.clone()).await;
    let systems = vec![
        system(None, Some("LV1-FOH"), "10.0.0.20", DiscoveredLv1Status::Available),
        system(None, Some("LV1-FOH"), "10.0.0.21", DiscoveredLv1Status::Available),
    ];
    let before = lifecycle.active_generation().await;

    let result = lifecycle
        .startup_auto_connect_with_discovered(
            app.handle().clone(),
            remembered.clone(),
            &systems,
        )
        .await
        .unwrap();

    assert!(!result.changed);
    assert_eq!(lifecycle.active_generation().await, before);
    assert_eq!(get_last_connected_lv1(&lifecycle.settings).await, Some(remembered));
}
```

Define `set_last_connected_lv1` and `get_last_connected_lv1` test helpers by sending the exact Task 1 mailbox commands. Keep real network discovery out of deterministic actor tests.

Add a connection-transaction test using the existing fake LV1/fade handles and `finish_connect_transaction_inner` seam. After a successful current-generation transaction, request `GetLastConnectedLv1` and assert it equals the confirmed identity. Add a stale-generation counterpart and assert identity remains unchanged.

- [ ] **Step 5: Run lifecycle tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
```

Expected: the new settings-backed decision and post-connect persistence assertions fail because lifecycle still reads `ShowState.connected_lv1_identity` and never writes settings metadata.

- [ ] **Step 6: Read and write remembered identity through SettingsActor**

Replace startup's call to `connected_lv1_identity()` with a private helper that sends `SettingsCommand::GetLastConnectedLv1` and maps mailbox failures to clear strings:

```rust
async fn last_connected_lv1_identity(
    &self,
) -> Result<Option<crate::connection_state::Lv1SystemIdentity>, String> {
    let (reply, rx) = oneshot::channel();
    self.settings
        .send(SettingsCommand::GetLastConnectedLv1 { reply })
        .await
        .map_err(|_| "Settings are unavailable".to_string())?;
    rx.await
        .map_err(|_| "Settings reply channel is closed".to_string())
}
```

Keep the existing show-backed `connected_lv1_identity()` for reconnect behavior; reconnect concerns the current runtime identity, while startup concerns persisted identity.

After `install_accepted_scene_recall_fader` succeeds and before returning a successful connection result, send `SettingsCommand::SetLastConnectedLv1`. If it fails, emit one user-visible error:

```rust
tracing::error!(
    event = "last_connected_lv1_save_failed",
    error = %error,
    "Connected to LV1, but the connection could not be remembered for next startup"
);
```

Do not return the persistence error from the successful connection command and do not clear the installed runtime. Apply the same behavior in `finish_connect_transaction_inner` so actor tests cover production ordering.

- [ ] **Step 7: Refresh discovery and connect only after safe selection**

Change `startup_auto_connect_lv1` to:

1. Read remembered identity from settings.
2. Return unchanged when absent.
3. Send `ShowCommand::RefreshLv1Discovery { timeout_ms: None, reply: Some(reply) }` and propagate discovery failure.
4. Request `ShowCommand::InitialProjectionState` and pass `discovered_lv1_systems` to `startup_auto_connect_target`.
5. Log a `DEBUG` event such as `startup_auto_connect_no_match` and return unchanged when no target exists.
6. Only then abort the current runtime, begin a generation, and call `connect_to_identity` with the discovered target.

Use `ConnectFailureMode::ClearConnectedIdentity` for projected current-state cleanup; this must not clear the settings-owned remembered identity.

- [ ] **Step 8: Run focused backend verification**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control settings
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: all commands pass with no warnings or formatting changes.

- [ ] **Step 9: Commit lifecycle behavior**

```bash
git add src-tauri/src/lifecycle/mod.rs
git commit -m "fix: restore safe startup auto-connect"
```

---

### Task 3: Add Same-Process Hardware Smoke Coverage

**Files:**
- Modify: `ui/src/debug/main.tsx:18-100`
- Modify: `docs/architecture.md:240-250`

**Interfaces:**
- Consumes: production Tauri commands `connect_lv1_system`, `disconnect_lv1`, and `startup_auto_connect_lv1`.
- Consumes: production `app-status-changed` snapshots.
- Produces: authoritative `TEST startup-auto-connect PASS|FAIL` line in `logs/debug-smoke-report.txt`.

- [ ] **Step 1: Add the smoke test status entry and capture original identity**

Add `"startup-auto-connect"` immediately after `"connection"` in the `tests` array. Introduce:

```typescript
let connectedIdentity: Lv1SystemIdentity | undefined;
```

In the existing connection test, after the connected snapshot arrives, require and save the projected identity rather than trusting only the selected discovery row:

```typescript
connectedIdentity = await waitFor(
  () => state?.connectedLv1Identity,
  "projected connected LV1 identity",
);
if (!connectedIdentity.uuid) {
  throw new Error("startup auto-connect smoke requires an LV1 UUID");
}
```

- [ ] **Step 2: Add the production-command startup auto-connect scenario**

Immediately after the existing connection test and before `setup()`, add:

```typescript
await test("startup-auto-connect", async () => {
  const expectedUuid = connectedIdentity?.uuid;
  if (!expectedUuid) {
    throw new Error("connected LV1 UUID is unavailable");
  }

  await invoke("disconnect_lv1");
  await waitFor(() => state?.connection === "disconnected", "LV1 disconnected");

  await invoke("startup_auto_connect_lv1");
  const reconnected = await waitFor(
    () =>
      state?.connection === "connected" &&
      state.connectedLv1Identity?.uuid === expectedUuid
        ? state.connectedLv1Identity
        : undefined,
    "startup auto-connected LV1 identity",
  );
  await log(`AUTO_CONNECTED ${label(reconnected)}`);
});
```

The following existing `setup()` production commands serve as the runtime-usability assertion. Do not add a debug-only connection command.

- [ ] **Step 3: Update smoke architecture coverage**

In `docs/architecture.md`'s debug smoke behavior list, add startup auto-connect after discovery/connection and state that it is a same-process disconnect/reconnect check using production commands and connected identity projection.

- [ ] **Step 4: Run frontend static and unit checks**

Run:

```bash
npm --prefix ui run format:check
npm --prefix ui run lint
npm --prefix ui run typecheck
npm --prefix ui run test -- AppRuntime.test.tsx smokeScenes.test.ts
```

Expected: all commands pass. No Storybook or visual snapshot update is expected because production UI rendering did not change.

- [ ] **Step 5: Run hardware smoke and inspect the authoritative report**

Run:

```bash
make smoke
```

Then open `logs/debug-smoke-report.txt` and verify it contains both:

```text
TEST startup-auto-connect PASS
SUITE PASS
```

Do not claim smoke success from terminal output alone. If no LV1-compatible target is available, record the hardware verification as blocked rather than deleting or weakening the scenario.

- [ ] **Step 6: Commit smoke coverage**

```bash
git add ui/src/debug/main.tsx docs/architecture.md
git commit -m "test: smoke startup auto-connect"
```

---

### Task 4: Final Verification and Issue Closure Evidence

**Files:**
- Verify only; modify files only if a verification failure exposes a defect in the preceding tasks.

**Interfaces:**
- Consumes: all prior task outputs.
- Produces: CI-style and hardware evidence for GitHub issue #43 acceptance criteria.

- [ ] **Step 1: Run the frontend startup modal tests explicitly**

```bash
npm --prefix ui run test -- AppRuntime.test.tsx
```

Expected: tests proving connected projection closes the startup modal and command failure keeps it open pass.

- [ ] **Step 2: Run standard non-visual verification**

```bash
make check
```

Expected: formatting, linting, Rust/frontend tests, Storybook tests, and builds all pass.

- [ ] **Step 3: Re-run hardware smoke after all fixes**

```bash
make smoke
```

Inspect `logs/debug-smoke-report.txt` and require `TEST startup-auto-connect PASS` plus `SUITE PASS`.

- [ ] **Step 4: Inspect final repository state**

```bash
git status --short
```

Expected: no uncommitted files from issue #43; unrelated concurrent work may remain and must not be staged, reverted, or modified.

- [ ] **Step 5: Commit any verification-only correction separately**

Only if verification required a code correction, stage each corrected issue #43 path explicitly after inspecting `git diff`, then commit with `git commit -m "fix: complete startup auto-connect verification"`. If no correction was needed, do not create an empty commit.
