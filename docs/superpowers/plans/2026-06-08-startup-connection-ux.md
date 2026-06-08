# Startup Connection UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a full-screen LV1 connection chooser with UUID-based remembered auto-connect, scoped discovery polling, and a minimal unexpected-reconnect dialog.

**Architecture:** Rust owns connection preferences, discovery state, launch auto-connect, reconnect decisions, and runtime setup/teardown. React owns only local screen navigation and renders backend-projected connection data. Existing `Lv1Actor`, `FadeEngine`, `AppCommandBus`, and generation guards remain the runtime safety boundary.

**Tech Stack:** Rust/Tauri, Tokio, Serde JSON, React, TypeScript, Tailwind, existing `lv1_scene_fade_utility::lv1::discovery` APIs.

---

## File Structure

- Create `src-tauri/src/connection_preferences.rs`: read/write app-local connection preferences JSON.
- Create `src-tauri/src/connection_state.rs`: backend model for discovered LV1 systems, current target identity, pending target, and reconnect dialog state.
- Modify `src-tauri/src/app_state/view.rs`: add serializable connection-system and reconnect fields to `AppViewState`.
- Modify `src-tauri/src/app_state/shell.rs`: store connection UI state in `ShellInner` and project it into snapshots.
- Modify `src-tauri/src/commands.rs`: add discovery/connection commands, save preferences after successful connect, expose startup auto-connect, and distinguish manual disconnect from unexpected reconnect.
- Modify `src-tauri/src/main.rs`: register new Tauri commands and manage new state.
- Modify `ui/src/types.ts`: mirror new Rust view types.
- Replace `ui/src/components/ConnectionTab.tsx` with a full-screen `ConnectionScreen.tsx`.
- Modify `ui/src/components/Header.tsx`: make connection status clickable.
- Modify `ui/src/App.tsx`: remove Connection tab, add full-screen connection mode, discovery polling only while visible, startup auto-connect, and reconnect dialog.

---

### Task 1: Connection Preferences File

**Files:**
- Create: `src-tauri/src/connection_preferences.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write failing preference tests**

Create `src-tauri/src/connection_preferences.rs` with tests first:

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastConnectedLv1 {
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPreferences {
    pub last_connected_lv1: Option<LastConnectedLv1>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_preferences_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lv1-scene-fade-utility-preferences-{name}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn missing_preferences_file_loads_defaults() {
        let path = temp_preferences_path("missing");

        let preferences = read_connection_preferences(&path).unwrap();

        assert_eq!(preferences, ConnectionPreferences::default());
    }

    #[test]
    fn preferences_round_trip_last_connected_lv1() {
        let path = temp_preferences_path("round-trip");
        let preferences = ConnectionPreferences {
            last_connected_lv1: Some(LastConnectedLv1 {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        };

        write_connection_preferences(&path, &preferences).unwrap();
        let loaded = read_connection_preferences(&path).unwrap();

        assert_eq!(loaded, preferences);
        let _ = std::fs::remove_file(path);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri connection_preferences`

Expected: FAIL because `read_connection_preferences` and `write_connection_preferences` are not defined yet.

- [ ] **Step 3: Implement preferences read/write**

Add these functions above the test module:

```rust
pub fn read_connection_preferences(path: &Path) -> Result<ConnectionPreferences, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|err| format!("Failed to parse connection preferences: {err}")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(ConnectionPreferences::default())
        }
        Err(err) => Err(format!("Failed to read connection preferences: {err}")),
    }
}

pub fn write_connection_preferences(
    path: &Path,
    preferences: &ConnectionPreferences,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create preferences folder: {err}"))?;
    }
    let contents = serde_json::to_string_pretty(preferences)
        .map_err(|err| format!("Failed to serialize connection preferences: {err}"))?;
    std::fs::write(path, contents)
        .map_err(|err| format!("Failed to write connection preferences: {err}"))
}
```

Add the module to `src-tauri/src/main.rs`:

```rust
mod connection_preferences;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lv1-scene-fade-utility-tauri connection_preferences`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/connection_preferences.rs src-tauri/src/main.rs
git commit -m "feat: add connection preferences storage"
```

---

### Task 2: Backend Connection View State

**Files:**
- Create: `src-tauri/src/connection_state.rs`
- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write failing projection tests**

Add `src-tauri/src/connection_state.rs`:

```rust
use lv1_scene_fade_utility::lv1::discovery::DiscoveryEntry;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveredLv1Status {
    Available,
    Connecting,
    Connected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lv1SystemIdentity {
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLv1System {
    pub identity: Lv1SystemIdentity,
    pub latency_ms: Option<u64>,
    pub status: DiscoveredLv1Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectState {
    pub active: bool,
}

pub fn identity_from_discovery(entry: &DiscoveryEntry) -> Option<Lv1SystemIdentity> {
    let address = entry.addresses.first()?.clone();
    let port = entry.port?;
    Some(Lv1SystemIdentity {
        uuid: entry.uuid.clone(),
        host: entry.host.clone(),
        address,
        port,
    })
}
```

In `src-tauri/src/app_state/shell.rs`, add a test in the existing test module:

```rust
#[test]
fn snapshot_includes_discovered_lv1_systems_and_reconnect_state() {
    let mut inner = ShellInner::default();
    inner.discovered_lv1_systems = vec![crate::connection_state::DiscoveredLv1System {
        identity: crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        },
        latency_ms: Some(12),
        status: crate::connection_state::DiscoveredLv1Status::Connected,
    }];
    inner.connected_lv1_identity = Some(crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50000,
    });
    inner.reconnect_state = crate::connection_state::ReconnectState { active: true };

    let snapshot = snapshot_from_inner(&inner);

    assert_eq!(snapshot.discovered_lv1_systems.len(), 1);
    assert_eq!(snapshot.discovered_lv1_systems[0].identity.address, "192.168.1.35");
    assert_eq!(snapshot.connected_lv1_identity.unwrap().address, "192.168.1.35");
    assert!(snapshot.reconnect.active);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri snapshot_includes_discovered_lv1_systems_and_reconnect_state`

Expected: FAIL because the new fields do not exist.

- [ ] **Step 3: Add view fields and shell storage**

Add to `src-tauri/src/main.rs`:

```rust
mod connection_state;
```

Modify imports in `src-tauri/src/app_state/view.rs`:

```rust
use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
```

Add fields to `AppViewState`:

```rust
pub discovered_lv1_systems: Vec<DiscoveredLv1System>,
pub connected_lv1_identity: Option<Lv1SystemIdentity>,
pub pending_lv1_identity: Option<Lv1SystemIdentity>,
pub reconnect: ReconnectState,
```

Modify imports in `src-tauri/src/app_state/shell.rs`:

```rust
use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
```

Add fields to `ShellInner`:

```rust
pub(super) discovered_lv1_systems: Vec<DiscoveredLv1System>,
pub(super) connected_lv1_identity: Option<Lv1SystemIdentity>,
pub(super) pending_lv1_identity: Option<Lv1SystemIdentity>,
pub(super) reconnect_state: ReconnectState,
```

Add those fields to `snapshot_from_inner`:

```rust
discovered_lv1_systems: inner.discovered_lv1_systems.clone(),
connected_lv1_identity: inner.connected_lv1_identity.clone(),
pending_lv1_identity: inner.pending_lv1_identity.clone(),
reconnect: inner.reconnect_state.clone(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lv1-scene-fade-utility-tauri snapshot_includes_discovered_lv1_systems_and_reconnect_state`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/connection_state.rs src-tauri/src/app_state/view.rs src-tauri/src/app_state/shell.rs src-tauri/src/main.rs
git commit -m "feat: expose connection chooser state"
```

---

### Task 3: Discovery Command And Status Projection

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write failing discovery projection test**

Add to `src-tauri/src/app_state/shell.rs` tests:

```rust
#[tokio::test]
async fn set_discovered_lv1_systems_marks_connected_and_pending_rows() {
    let state = ShellState::default();
    let connected = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50000,
    };
    let pending = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-2".to_string()),
        host: Some("LV1-MON".to_string()),
        address: "192.168.1.36".to_string(),
        port: 50000,
    };
    state.set_connected_lv1_identity(Some(connected.clone())).await;
    state.set_pending_lv1_identity(Some(pending.clone())).await;

    let snapshot = state
        .set_discovered_lv1_systems(vec![
            crate::connection_state::DiscoveredLv1System {
                identity: connected,
                latency_ms: Some(10),
                status: crate::connection_state::DiscoveredLv1Status::Available,
            },
            crate::connection_state::DiscoveredLv1System {
                identity: pending,
                latency_ms: Some(20),
                status: crate::connection_state::DiscoveredLv1Status::Available,
            },
        ])
        .await;

    assert_eq!(snapshot.discovered_lv1_systems[0].status, crate::connection_state::DiscoveredLv1Status::Connected);
    assert_eq!(snapshot.discovered_lv1_systems[1].status, crate::connection_state::DiscoveredLv1Status::Connecting);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri set_discovered_lv1_systems_marks_connected_and_pending_rows`

Expected: FAIL because shell methods do not exist.

- [ ] **Step 3: Implement shell methods and discovery command**

Add methods to `impl ShellState` in `src-tauri/src/app_state/shell.rs`:

```rust
pub async fn set_connected_lv1_identity(&self, identity: Option<Lv1SystemIdentity>) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.connected_lv1_identity = identity;
    refresh_discovered_statuses(&mut inner);
    snapshot_from_inner(&inner)
}

pub async fn set_pending_lv1_identity(&self, identity: Option<Lv1SystemIdentity>) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.pending_lv1_identity = identity;
    refresh_discovered_statuses(&mut inner);
    snapshot_from_inner(&inner)
}

pub async fn set_discovered_lv1_systems(&self, systems: Vec<DiscoveredLv1System>) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.discovered_lv1_systems = systems;
    refresh_discovered_statuses(&mut inner);
    snapshot_from_inner(&inner)
}
```

Add helper in `shell.rs`:

```rust
fn refresh_discovered_statuses(inner: &mut ShellInner) {
    for system in &mut inner.discovered_lv1_systems {
        system.status = if Some(&system.identity) == inner.connected_lv1_identity.as_ref() {
            crate::connection_state::DiscoveredLv1Status::Connected
        } else if Some(&system.identity) == inner.pending_lv1_identity.as_ref() {
            crate::connection_state::DiscoveredLv1Status::Connecting
        } else {
            crate::connection_state::DiscoveredLv1Status::Available
        };
    }
}
```

Add command to `src-tauri/src/commands.rs`:

```rust
#[tauri::command]
pub async fn refresh_lv1_discovery(
    app: AppHandle,
    state: State<'_, ShellState>,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    let started = std::time::Instant::now();
    let entries = lv1_scene_fade_utility::lv1::discovery::discover(
        lv1_scene_fade_utility::lv1::discovery::DiscoverOptions {
            timeout: std::time::Duration::from_millis(timeout_ms.unwrap_or(1000)),
            ..Default::default()
        },
    )
    .map_err(|err| format!("Failed to discover LV1 systems: {err}"))?;

    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let systems = entries
        .iter()
        .filter_map(crate::connection_state::identity_from_discovery)
        .map(|identity| crate::connection_state::DiscoveredLv1System {
            identity,
            latency_ms: Some(latency_ms),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        })
        .collect();
    let snapshot = state.set_discovered_lv1_systems(systems).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}
```

Register `refresh_lv1_discovery` in `src-tauri/src/main.rs` inside `tauri::generate_handler!`.

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri set_discovered_lv1_systems_marks_connected_and_pending_rows`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/app_state/shell.rs src-tauri/src/main.rs
git commit -m "feat: add lv1 discovery view state"
```

---

### Task 4: Connect By Discovered Identity And Save Preferences

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write failing command exposure test**

Add to `src-tauri/src/commands.rs` tests:

```rust
#[test]
fn connection_chooser_commands_are_exposed() {
    let _ = refresh_lv1_discovery;
    let _ = connect_lv1_system;
    let _ = startup_auto_connect_lv1;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri connection_chooser_commands_are_exposed`

Expected: FAIL because `connect_lv1_system` and `startup_auto_connect_lv1` are not defined.

- [ ] **Step 3: Extract connect helper and add identity command**

In `src-tauri/src/commands.rs`, extract the body of `connect_lv1` after target resolution into:

```rust
async fn connect_to_target<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    identity: crate::connection_state::Lv1SystemIdentity,
) -> Result<AppViewState, String> {
    let event_bus = AppEventBus::default();
    let (_, disconnected_snapshot) = state.disconnect().await;
    emit_snapshot(&app, &disconnected_snapshot);
    let (generation, connecting_snapshot) = state.begin_connecting().await;
    state.set_pending_lv1_identity(Some(identity.clone())).await;
    emit_snapshot(&app, &connecting_snapshot);
    let events = event_bus.subscribe();
    let shell_state = state.clone();
    let lv1 = spawn_actor(identity.address.clone(), identity.port, event_bus.clone());
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_lv1(Some(lv1.clone())).await;
    let fade_command_bus = command_bus.clone();
    let fade = spawn_engine(command_bus, event_bus.clone());
    fade_command_bus.set_fade(Some(fade.clone())).await;
    let mut runtime_handles = RuntimeHandles {
        active_generation: 0,
        lv1: Some(lv1.clone()),
        fade: Some(fade),
        command_bus: Some(fade_command_bus.clone()),
        projector: None,
        scene_recall_fader: Some(spawn_scene_recall_fader(
            shell_state.clone(),
            generation,
            fade_command_bus.clone(),
            event_bus.clone(),
        )),
    };
    let initial_snapshot = lv1.get_state().await;
    let snapshot = match state.begin_connection_for_generation(generation, initial_snapshot).await {
        Some(snapshot) => snapshot,
        None => {
            runtime_handles.abort_all().await;
            state.set_pending_lv1_identity(None).await;
            let snapshot = state.snapshot().await;
            emit_snapshot(&app, &snapshot);
            return Ok(snapshot);
        }
    };
    let snapshot = state.set_connected_lv1_identity(Some(identity)).await;
    state.set_pending_lv1_identity(None).await;
    install_connected_runtime(&app, &state, shell_state, generation, snapshot, events, runtime_handles, &active_command_bus).await
}
```

Then add command:

```rust
#[tauri::command]
pub async fn connect_lv1_system(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    identity: crate::connection_state::Lv1SystemIdentity,
) -> Result<AppViewState, String> {
    connect_to_target(app, (*state).clone(), (*active_command_bus).clone(), identity).await
}
```

Update old `connect_lv1` to build an identity from resolved host/port and call `connect_to_target`, preserving the command for tests and any existing UI call sites.

- [ ] **Step 4: Save preferences after successful connection**

Inside `connect_lv1_system`, after `connect_to_target` returns `Ok(snapshot)` and `snapshot.connection == AppConnectionState::Connected`, write preferences using Tauri app config dir:

```rust
let preferences = crate::connection_preferences::ConnectionPreferences {
    last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
        uuid: identity.uuid.clone(),
        host: identity.host.clone(),
        address: identity.address.clone(),
        port: identity.port,
    }),
};
let preferences_path = app
    .path()
    .app_config_dir()
    .map_err(|err| format!("Failed to resolve app config dir: {err}"))?
    .join("preferences.json");
crate::connection_preferences::write_connection_preferences(&preferences_path, &preferences)?;
```

Import `tauri::Manager` if required for `app.path()`.

- [ ] **Step 5: Register and test**

Register `connect_lv1_system` and `startup_auto_connect_lv1` placeholder in `main.rs`. Add a placeholder command for startup for now:

```rust
#[tauri::command]
pub async fn startup_auto_connect_lv1(state: State<'_, ShellState>) -> Result<AppViewState, String> {
    Ok(state.snapshot().await)
}
```

Run: `cargo test -p lv1-scene-fade-utility-tauri connection_chooser_commands_are_exposed`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: connect lv1 from discovered system"
```

---

### Task 5: Startup Auto-Connect By Remembered UUID

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write unit test for UUID matching helper**

Add helper tests in `src-tauri/src/commands.rs` tests:

```rust
#[test]
fn remembered_uuid_matches_discovered_identity_without_host_fallback() {
    let preferences = crate::connection_preferences::ConnectionPreferences {
        last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
            uuid: Some("uuid-1".to_string()),
            host: Some("Old Host".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        }),
    };
    let systems = vec![crate::connection_state::DiscoveredLv1System {
        identity: crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("New Host".to_string()),
            address: "10.0.0.20".to_string(),
            port: 50000,
        },
        latency_ms: Some(10),
        status: crate::connection_state::DiscoveredLv1Status::Available,
    }];

    let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

    assert_eq!(matched.address, "10.0.0.20");
}

#[test]
fn remembered_uuid_absent_does_not_match_same_address() {
    let preferences = crate::connection_preferences::ConnectionPreferences {
        last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        }),
    };
    let systems = vec![crate::connection_state::DiscoveredLv1System {
        identity: crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-2".to_string()),
            host: Some("LV1".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        },
        latency_ms: Some(10),
        status: crate::connection_state::DiscoveredLv1Status::Available,
    }];

    assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri remembered_uuid`

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Implement helper and startup command**

Add helper near commands:

```rust
fn remembered_auto_connect_target(
    preferences: &crate::connection_preferences::ConnectionPreferences,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Option<crate::connection_state::Lv1SystemIdentity> {
    let remembered_uuid = preferences.last_connected_lv1.as_ref()?.uuid.as_ref()?;
    systems
        .iter()
        .find(|system| system.identity.uuid.as_ref() == Some(remembered_uuid))
        .map(|system| system.identity.clone())
}
```

Replace `startup_auto_connect_lv1` placeholder with command that reads preferences, discovers once, updates discovery state, and connects only when UUID matches:

```rust
#[tauri::command]
pub async fn startup_auto_connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<AppViewState, String> {
    let preferences_path = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("Failed to resolve app config dir: {err}"))?
        .join("preferences.json");
    let preferences = crate::connection_preferences::read_connection_preferences(&preferences_path)?;
    let snapshot = refresh_lv1_discovery(app.clone(), state.clone(), Some(1000)).await?;
    if let Some(identity) = remembered_auto_connect_target(&preferences, &snapshot.discovered_lv1_systems) {
        return connect_lv1_system(app, state, active_command_bus, identity).await;
    }
    Ok(snapshot)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri remembered_uuid`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: auto connect remembered lv1"
```

---

### Task 6: Unexpected Reconnect State

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing reconnect state test**

Add to `src-tauri/src/app_state/events_tests.rs`:

```rust
#[tokio::test]
async fn lv1_disconnected_event_enters_reconnect_state() {
    let state = super::ShellState::default();
    state
        .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        }))
        .await;
    let (generation, _) = state.begin_connecting().await;
    let snapshot = state
        .apply_lv1_event_for_generation(
            generation,
            &lv1_scene_fade_utility::lv1::events::Lv1Event::Disconnected,
        )
        .await
        .unwrap();

    assert!(snapshot.reconnect.active);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility-tauri lv1_disconnected_event_enters_reconnect_state`

Expected: FAIL because disconnect event currently clears snapshot without reconnect state.

- [ ] **Step 3: Add reconnect state setters**

Add to `ShellState` in `shell.rs`:

```rust
pub async fn set_reconnect_active(&self, active: bool) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.reconnect_state.active = active;
    snapshot_from_inner(&inner)
}
```

Modify `src-tauri/src/app_state/events.rs` handling for `Lv1Event::Disconnected` so unexpected LV1 event sets reconnect active instead of treating it like manual disconnect:

```rust
Lv1Event::Disconnected => {
    inner.lv1_snapshot = None;
    inner.reconnect_state.active = inner.connected_lv1_identity.is_some();
    push_log(
        inner,
        LogSource::Lv1,
        LogSeverity::Warning,
        "LV1 disconnected".to_string(),
    );
}
```

Modify `ShellState::disconnect` in `events.rs` to set `reconnect_state.active = false` and clear `connected_lv1_identity` for manual disconnect.

- [ ] **Step 4: Add reconnect timeout command**

Add command in `commands.rs`:

```rust
#[tauri::command]
pub async fn reconnect_timed_out(
    app: AppHandle,
    state: State<'_, ShellState>,
) -> Result<AppViewState, String> {
    let snapshot = state.set_reconnect_active(false).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}
```

Register `reconnect_timed_out` in `main.rs`.

- [ ] **Step 5: Run targeted tests**

Run: `cargo test -p lv1-scene-fade-utility-tauri lv1_disconnected_event_enters_reconnect_state`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state/events.rs src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat: show reconnect state on lv1 disconnect"
```

---

### Task 7: TypeScript Types And Full-Screen Connection UI

**Files:**
- Modify: `ui/src/types.ts`
- Delete: `ui/src/components/ConnectionTab.tsx`
- Create: `ui/src/components/ConnectionScreen.tsx`

- [ ] **Step 1: Update TypeScript types**

Modify `ui/src/types.ts`:

```ts
export type DiscoveredLv1Status = "available" | "connecting" | "connected" | "unavailable";

export type Lv1SystemIdentity = {
  uuid: string | null;
  host: string | null;
  address: string;
  port: number;
};

export type DiscoveredLv1System = {
  identity: Lv1SystemIdentity;
  latencyMs: number | null;
  status: DiscoveredLv1Status;
};

export type ReconnectState = {
  active: boolean;
};
```

Add to `AppViewState`:

```ts
discoveredLv1Systems: DiscoveredLv1System[];
connectedLv1Identity: Lv1SystemIdentity | null;
pendingLv1Identity: Lv1SystemIdentity | null;
reconnect: ReconnectState;
```

Add defaults to `disconnectedAppViewState`:

```ts
discoveredLv1Systems: [],
connectedLv1Identity: null,
pendingLv1Identity: null,
reconnect: { active: false },
```

- [ ] **Step 2: Create ConnectionScreen**

Create `ui/src/components/ConnectionScreen.tsx`:

```tsx
import type { AppViewState, DiscoveredLv1System, Lv1SystemIdentity } from "../types";

export function ConnectionScreen(props: {
  appState: AppViewState;
  commandError: string | null;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  return (
    <main className="min-h-screen bg-slate-950 p-6 text-slate-100">
      <section className="mx-auto grid max-w-5xl gap-5">
        <div>
          <p className="text-sm uppercase tracking-[0.25em] text-cyan-300">LV1 Connection</p>
          <h1 className="mt-2 text-3xl font-semibold">Choose an LV1 system</h1>
          <p className="mt-2 text-slate-400">Tap a discovered system to connect.</p>
        </div>

        {props.commandError && (
          <p className="rounded-lg border border-red-800 bg-red-950 px-3 py-2 text-sm text-red-100">
            {props.commandError}
          </p>
        )}

        <div className="grid gap-3">
          {props.appState.discoveredLv1Systems.length === 0 ? (
            <div className="rounded-xl border border-slate-800 bg-slate-900 p-6 text-slate-400">
              Searching for LV1 systems...
            </div>
          ) : (
            props.appState.discoveredLv1Systems.map((system) => (
              <SystemRow
                key={systemKey(system)}
                system={system}
                onSelectSystem={props.onSelectSystem}
                onResume={props.onResume}
              />
            ))
          )}
        </div>
      </section>
    </main>
  );
}

function SystemRow(props: {
  system: DiscoveredLv1System;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  const { system } = props;
  const isConnected = system.status === "connected";
  return (
    <button
      className="grid gap-3 rounded-xl border border-slate-800 bg-slate-900 p-5 text-left hover:border-cyan-700 hover:bg-slate-900/80 md:grid-cols-[1fr_auto] md:items-center"
      onClick={() => (isConnected ? props.onResume() : props.onSelectSystem(system.identity))}
    >
      <div>
        <div className="text-lg font-semibold text-slate-100">{system.identity.host ?? "LV1 System"}</div>
        <div className="mt-1 text-sm text-slate-400">
          {system.identity.address}:{system.identity.port}
        </div>
      </div>
      <div className="flex flex-wrap gap-2 text-sm">
        <span className="rounded-full border border-slate-700 px-3 py-1 text-slate-300">
          {system.latencyMs === null ? "Latency unknown" : `${system.latencyMs} ms`}
        </span>
        <span className="rounded-full border border-cyan-700 px-3 py-1 text-cyan-100">{system.status}</span>
      </div>
    </button>
  );
}

function systemKey(system: DiscoveredLv1System) {
  return system.identity.uuid ?? `${system.identity.address}:${system.identity.port}`;
}
```

- [ ] **Step 3: Remove old ConnectionTab import target**

Delete `ui/src/components/ConnectionTab.tsx` after `App.tsx` is updated in the next task.

- [ ] **Step 4: Run typecheck and expect App import failure until next task**

Run: `npm run typecheck`

Expected: FAIL because `App.tsx` still imports `ConnectionTab`. This confirms the next task must update app routing.

---

### Task 8: React Screen Flow, Polling, And Reconnect Dialog

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/Header.tsx`
- Modify: `ui/src/commands.ts`

- [ ] **Step 1: Add typed command helpers**

Modify `ui/src/commands.ts` to export direct invoke helpers:

```ts
import type { AppViewState, Lv1SystemIdentity } from "./types";

export async function startupAutoConnectLv1() {
  return invoke<AppViewState>("startup_auto_connect_lv1");
}

export async function refreshLv1Discovery() {
  return invoke<AppViewState>("refresh_lv1_discovery", { timeoutMs: 1000 });
}

export async function connectLv1System(identity: Lv1SystemIdentity) {
  return invoke<AppViewState>("connect_lv1_system", { identity });
}
```

- [ ] **Step 2: Make Header connection status clickable**

Modify `Header` props:

```ts
onOpenConnection: () => void;
```

Replace the connection `StatusBadge` with a button:

```tsx
<button onClick={props.onOpenConnection} className="rounded-full focus:outline-none focus:ring-2 focus:ring-cyan-400">
  <StatusBadge label={props.appState.connection} tone={props.appState.connection === "connected" ? "good" : "neutral"} />
</button>
```

- [ ] **Step 3: Update App routing**

Rewrite the connection-related state in `ui/src/App.tsx`:

```tsx
type MainTab = "scene" | "logs";

const [activeTab, setActiveTab] = useState<MainTab>("scene");
const [showConnection, setShowConnection] = useState(true);
```

Remove `host`, `port`, and the old `connect()` function.

On initial mount, call startup auto-connect:

```tsx
useEffect(() => {
  let cancelled = false;

  async function start() {
    setCommandError(null);
    try {
      const snapshot = await startupAutoConnectLv1();
      if (cancelled) return;
      setAppState(snapshot);
      setShowConnection(snapshot.connection !== "connected");
    } catch (error) {
      if (!cancelled) {
        setCommandError(String(error));
        setShowConnection(true);
      }
    }
  }

  void start();
  const unlistenPromise = listen<AppViewState>("app-status-changed", (event) => {
    if (!cancelled) setAppState(event.payload);
  });
  return () => {
    cancelled = true;
    void unlistenPromise.then((unlisten) => void unlisten());
  };
}, []);
```

Add discovery polling only while Connection is visible:

```tsx
useEffect(() => {
  if (!showConnection) return;
  let cancelled = false;
  async function refresh() {
    try {
      const snapshot = await refreshLv1Discovery();
      if (!cancelled) setAppState(snapshot);
    } catch (error) {
      if (!cancelled) setCommandError(String(error));
    }
  }
  void refresh();
  const timer = window.setInterval(() => void refresh(), 5000);
  return () => {
    cancelled = true;
    window.clearInterval(timer);
  };
}, [showConnection]);
```

Render `ConnectionScreen` before the main app when `showConnection` is true:

```tsx
if (showConnection) {
  return (
    <ConnectionScreen
      appState={appState}
      commandError={commandError}
      onResume={() => setShowConnection(false)}
      onSelectSystem={async (identity) => {
        setCommandError(null);
        try {
          const snapshot = await connectLv1System(identity);
          setAppState(snapshot);
          if (snapshot.connection === "connected") setShowConnection(false);
        } catch (error) {
          setCommandError(String(error));
        }
      }}
    />
  );
}
```

Remove the Connection tab button and render only Scene/Logs tabs. Pass `onOpenConnection={() => setShowConnection(true)}` to `Header`.

- [ ] **Step 4: Add reconnect dialog and timeout**

In `App.tsx`, overlay:

```tsx
{appState.reconnect.active && (
  <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/70">
    <div className="rounded-xl border border-slate-700 bg-slate-900 px-8 py-6 text-xl font-semibold text-slate-100 shadow-2xl">
      Reconnecting...
    </div>
  </div>
)}
```

Add effect:

```tsx
useEffect(() => {
  if (!appState.reconnect.active) return;
  const timer = window.setTimeout(() => {
    void runSnapshotCommand("reconnect_timed_out", undefined, setAppState, setCommandError).then(() => {
      setShowConnection(true);
    });
  }, 15000);
  return () => window.clearTimeout(timer);
}, [appState.reconnect.active]);
```

- [ ] **Step 5: Run typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/App.tsx ui/src/types.ts ui/src/commands.ts ui/src/components/Header.tsx ui/src/components/ConnectionScreen.tsx ui/src/components/ConnectionTab.tsx
git commit -m "feat: add full screen connection chooser"
```

---

### Task 9: Final Verification And Docs Cleanup

**Files:**
- Modify: `IDEAS.md`
- Optionally modify: `docs/architecture.md` if implementation adds a named connection coordinator module.

- [ ] **Step 1: Remove completed idea**

Remove this completed line from `IDEAS.md`:

```md
- Improve startup and connection UX. The app should open on the connection screen, let the user choose from auto-discovered LV1 systems, remember the last connected system in a config file, auto-connect on launch when that system is available, and return to the connection screen whenever LV1 is disconnected.
```

- [ ] **Step 2: Run frontend verification**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 3: Run backend verification**

Run: `cargo test --workspace`

Expected: PASS.

- [ ] **Step 4: Inspect worktree**

Run: `git status --short`

Expected: only intended files modified. Existing unrelated `docs/architecture.md` changes may still appear; do not stage them unless they were made for this implementation.

- [ ] **Step 5: Commit**

```bash
git add IDEAS.md
git commit -m "docs: mark connection ux idea complete"
```

---

## Self-Review

- Spec coverage: The plan covers full-screen Connection mode, row click-to-connect, current-system resume, scoped discovery polling, UUID-only launch auto-connect, preferences storage in app config, unexpected reconnect dialog, manual disconnect behavior, and no soft lock.
- Placeholder scan: No open-ended placeholders remain. Failing-test steps fail through missing functions or missing fields, then define the concrete implementation in the same task.
- Type consistency: Rust view types use camelCase serialization and TypeScript mirrors the same names: `discoveredLv1Systems`, `connectedLv1Identity`, `pendingLv1Identity`, and `reconnect.active`.
