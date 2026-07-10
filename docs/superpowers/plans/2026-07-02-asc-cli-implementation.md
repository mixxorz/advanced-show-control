# ASC CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS-first Docker-like `asc`/`ascd` CLI system that controls LV1 directly, keeps a persistent daemon connection, exposes documented LV1 OSC writes, exposes authoritative read commands, and verifies end-to-end behavior on real disposable LV1 hardware.

**Architecture:** Add focused Rust library modules under `src-tauri/src/asc_cli/` for protocol, command mapping, IPC, daemon state, CLI parsing, and smoke orchestration. Add `src-tauri/src/bin/asc.rs` as a thin client and `src-tauri/src/bin/ascd.rs` as the daemon that owns discovery, target persistence, the LV1 actor, reconnect behavior, and authoritative state reads. Extend `lv1` state/types/parsers/commands only where the shared LV1 runtime needs new authoritative state or new documented writes.

**Tech Stack:** Rust 2024, Tokio, Clap derive, Serde/serde_json, Unix domain sockets via `tokio::net::UnixListener` and `UnixStream`, existing LV1 OSC/TCP/discovery/actor modules, Cargo nextest, real LV1 hardware smoke through `make cli-smoke`.

## Global Constraints

- The first implementation is macOS-first; keep IPC behind a small transport boundary so Windows named pipes can be added later without changing command routing.
- The CLI is a raw LV1 power tool and must not enforce app lockout, show-file state, app-managed scene fade policy, exact scene identity validation, or fade generation guards.
- Read commands must return authoritative LV1-mirrored state only and must not return last-written fallback values.
- Named write commands must cover every documented client-to-LV1 OSC command in `docs/lv1-osc.md`.
- `asc osc send` must support arbitrary OSC messages using documented argument notation.
- Completion requires real-hardware verification against the currently available disposable LV1 target.
- Rust tests must be pure unit tests, actor-style tests through mailboxes/event bus/fakes, or smoke tests through a binary/app path.
- Use `cargo nextest run ...` for Rust tests; avoid `cargo test` unless nextest cannot support the harness.
- Commit after each independently passing task; stage only files changed for that task.

---

## File Structure

- Create `src-tauri/src/asc_cli/mod.rs`: module root and public re-exports for CLI/daemon internals.
- Create `src-tauri/src/asc_cli/protocol.rs`: JSON IPC request/response types, command enums, error codes, output mode, and shared DTOs.
- Create `src-tauri/src/asc_cli/osc_args.rs`: parser for raw OSC argument notation such as `i:0`, `d:-12`, `s:name`, `T`, and `F`.
- Create `src-tauri/src/asc_cli/commands.rs`: maps high-level CLI daemon requests to LV1 actor commands or raw OSC writes.
- Create `src-tauri/src/asc_cli/queries.rs`: authoritative state query helpers over `Lv1StateSnapshot`.
- Create `src-tauri/src/asc_cli/paths.rs`: per-user socket, config, state, and log paths.
- Create `src-tauri/src/asc_cli/ipc.rs`: Unix socket client/server helpers for newline-delimited JSON requests and responses.
- Create `src-tauri/src/asc_cli/daemon.rs`: daemon runtime, selected target persistence, LV1 actor ownership, request dispatch, and shutdown.
- Create `src-tauri/src/asc_cli/client.rs`: `asc` client behavior, daemon auto-start, request sending, and response formatting.
- Create `src-tauri/src/asc_cli/cli.rs`: Clap parser for the `asc` binary and conversion into protocol requests.
- Create `src-tauri/src/asc_cli/smoke.rs`: hardware smoke runner used by `asc cli-smoke` or a dedicated smoke binary path.
- Create `src-tauri/src/bin/asc.rs`: user-facing CLI client entrypoint.
- Create `src-tauri/src/bin/ascd.rs`: daemon entrypoint.
- Modify `src-tauri/src/lib.rs`: expose `asc_cli` module.
- Modify `src-tauri/src/lv1/mod.rs`: export any LV1 types/functions needed by ASC CLI modules.
- Modify `src-tauri/src/lv1/types.rs`: add serializable authoritative state structs for newly mirrored LV1 notifications.
- Modify `src-tauri/src/lv1/parsers.rs`: add parser helpers for newly mirrored notifications.
- Modify `src-tauri/src/lv1/state.rs`: update `ActorState` and `handle_message` to store new authoritative state.
- Modify `src-tauri/src/lv1/events.rs`: add LV1 events for newly mirrored state only where existing event consumers need fact notifications.
- Modify `src-tauri/src/lv1/commands.rs`: add command variants or raw OSC write support for documented client-to-LV1 writes not covered today.
- Modify `src-tauri/src/lv1/actor.rs`: route new LV1 command variants to encoded OSC frames.
- Modify `src-tauri/src/lv1/tcp.rs`: add reusable documented OSC encode helpers where useful.
- Modify `Makefile`: add `cli`, `cli-smoke`, and help text.
- Modify `docs/roadmap.md`: note the ASC CLI daemon as planned/in-progress or completed when implementation finishes.

---

### Task 1: IPC Protocol And Raw OSC Argument Parser

**Files:**
- Create: `src-tauri/src/asc_cli/mod.rs`
- Create: `src-tauri/src/asc_cli/protocol.rs`
- Create: `src-tauri/src/asc_cli/osc_args.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `asc_cli::protocol::{AscRequest, AscResponse, AscCommand, AscErrorCode, AscError, OutputMode}`.
- Produces: `asc_cli::osc_args::parse_osc_arg(input: &str) -> Result<crate::lv1::osc::OscArg, AscError>`.
- Produces: `asc_cli::osc_args::parse_osc_args(inputs: &[String]) -> Result<Vec<crate::lv1::osc::OscArg>, AscError>`.

- [ ] **Step 1: Add failing protocol serialization tests**

Add this test module to the new `src-tauri/src/asc_cli/protocol.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_status_request_with_json_output_mode() {
        let request = AscRequest {
            id: "req-1".to_string(),
            output: OutputMode::Json,
            command: AscCommand::Status,
        };

        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["id"], "req-1");
        assert_eq!(value["output"], "json");
        assert_eq!(value["command"]["type"], "status");
    }

    #[test]
    fn serializes_structured_error_response() {
        let response = AscResponse::error(
            "req-2".to_string(),
            AscErrorCode::Lv1NotConnected,
            "LV1 is not connected".to_string(),
        );

        let value = serde_json::to_value(&response).unwrap();

        assert_eq!(value["id"], "req-2");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "lv1_not_connected");
        assert_eq!(value["error"]["message"], "LV1 is not connected");
    }
}
```

- [ ] **Step 2: Add failing OSC argument parser tests**

Add this test module to the new `src-tauri/src/asc_cli/osc_args.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::osc::OscArg;

    #[test]
    fn parses_documented_osc_argument_notation() {
        assert_eq!(parse_osc_arg("i:-1").unwrap(), OscArg::Int(-1));
        assert_eq!(parse_osc_arg("h:9000000000").unwrap(), OscArg::Int64(9_000_000_000));
        assert_eq!(parse_osc_arg("f:1.5").unwrap(), OscArg::Float(1.5));
        assert_eq!(parse_osc_arg("d:-12.25").unwrap(), OscArg::Double(-12.25));
        assert_eq!(parse_osc_arg("s:Lead Vox").unwrap(), OscArg::String("Lead Vox".to_string()));
        assert_eq!(parse_osc_arg("T").unwrap(), OscArg::True);
        assert_eq!(parse_osc_arg("F").unwrap(), OscArg::False);
    }

    #[test]
    fn rejects_unknown_or_invalid_osc_argument_notation() {
        assert_eq!(parse_osc_arg("x:1").unwrap_err().code, AscErrorCode::OscEncodeFailed);
        assert_eq!(parse_osc_arg("i:not-int").unwrap_err().code, AscErrorCode::OscEncodeFailed);
        assert_eq!(parse_osc_arg("true").unwrap_err().code, AscErrorCode::OscEncodeFailed);
    }
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::protocol asc_cli::osc_args`

Expected: FAIL because `asc_cli` modules and types do not exist yet.

- [ ] **Step 4: Implement minimal protocol and parser modules**

Create `src-tauri/src/asc_cli/mod.rs`:

```rust
pub mod osc_args;
pub mod protocol;
```

Add `pub mod asc_cli;` to `src-tauri/src/lib.rs` next to the other module declarations.

Create `src-tauri/src/asc_cli/protocol.rs` with these initial types:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    Human,
    Json,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AscRequest {
    pub id: String,
    pub output: OutputMode,
    pub command: AscCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AscCommand {
    Status,
    RawOsc { address: String, args: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AscErrorCode {
    DaemonUnavailable,
    Lv1NotConnected,
    TargetNotSelected,
    UnsupportedRead,
    StateUnavailable,
    OscEncodeFailed,
    CommandFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AscError {
    pub code: AscErrorCode,
    pub message: String,
}

impl AscError {
    pub fn new(code: AscErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AscResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AscError>,
}

impl AscResponse {
    pub fn ok(id: String, result: serde_json::Value) -> Self {
        Self { id, ok: true, result: Some(result), error: None }
    }

    pub fn error(id: String, code: AscErrorCode, message: String) -> Self {
        Self { id, ok: false, result: None, error: Some(AscError { code, message }) }
    }
}
```

Create `src-tauri/src/asc_cli/osc_args.rs`:

```rust
use crate::asc_cli::protocol::{AscError, AscErrorCode};
use crate::lv1::osc::OscArg;

pub fn parse_osc_args(inputs: &[String]) -> Result<Vec<OscArg>, AscError> {
    inputs.iter().map(|input| parse_osc_arg(input)).collect()
}

pub fn parse_osc_arg(input: &str) -> Result<OscArg, AscError> {
    if input == "T" {
        return Ok(OscArg::True);
    }
    if input == "F" {
        return Ok(OscArg::False);
    }
    let (prefix, value) = input.split_once(':').ok_or_else(|| invalid_arg(input))?;
    match prefix {
        "i" => value.parse::<i32>().map(OscArg::Int).map_err(|_| invalid_arg(input)),
        "h" => value.parse::<i64>().map(OscArg::Int64).map_err(|_| invalid_arg(input)),
        "f" => value.parse::<f32>().map(OscArg::Float).map_err(|_| invalid_arg(input)),
        "d" => value.parse::<f64>().map(OscArg::Double).map_err(|_| invalid_arg(input)),
        "s" => Ok(OscArg::String(value.to_string())),
        _ => Err(invalid_arg(input)),
    }
}

fn invalid_arg(input: &str) -> AscError {
    AscError::new(
        AscErrorCode::OscEncodeFailed,
        format!("invalid OSC argument notation: {input}"),
    )
}
```

- [ ] **Step 5: Run tests and verify they pass**

Run: `cargo nextest run -p advanced-show-control asc_cli::protocol asc_cli::osc_args`

Expected: PASS.

- [ ] **Step 6: Run formatting and commit**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/lib.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/asc_cli/protocol.rs src-tauri/src/asc_cli/osc_args.rs
git commit -m "feat: add asc cli protocol types"
```

---

### Task 2: Extend LV1 Authoritative State Mirror

**Files:**
- Modify: `src-tauri/src/lv1/types.rs`
- Modify: `src-tauri/src/lv1/parsers.rs`
- Modify: `src-tauri/src/lv1/state.rs`
- Modify: `src-tauri/src/lv1/events.rs`

**Interfaces:**
- Consumes: existing `lv1::osc::OscArg` and `lv1::state::handle_message`.
- Produces: expanded `Lv1StateSnapshot` with fields `aux_tracks`, `aux_sends`, `solo_states`, `user_keys`, `spill_buttons`, `mute_groups`, `internal_assigns`, `tempo`, `layers`, `current_layer`, and `diagnostics`.
- Produces: parser functions `parse_aux_tracks`, `parse_user_key_info`, `parse_layers`, and numeric helpers for `f`, `d`, or `i` values.

- [ ] **Step 1: Write failing parser tests for newly documented notifications**

Add tests to `src-tauri/src/lv1/parsers.rs`:

```rust
#[test]
fn parses_aux_tracks_batch() {
    let args = vec![
        OscArg::Int(2),
        OscArg::Int(0), OscArg::Int(2), OscArg::String("Fx 1".to_string()),
        OscArg::Int(1), OscArg::Int(2), OscArg::String("Mon 1".to_string()),
    ];

    let tracks = parse_aux_tracks(&args).unwrap();

    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].index, 0);
    assert_eq!(tracks[0].group, 2);
    assert_eq!(tracks[0].name, "Fx 1");
}

#[test]
fn parses_user_key_info() {
    let info = parse_user_key_info(&[
        OscArg::Int(3),
        OscArg::String("Tap".to_string()),
        OscArg::String("Tap Tempo".to_string()),
        OscArg::Int(1),
    ]).unwrap();

    assert_eq!(info.index, 3);
    assert_eq!(info.short_name, "Tap");
    assert_eq!(info.function, "Tap Tempo");
    assert!(info.assigned);
}

#[test]
fn parses_numeric_values_from_float_double_or_int() {
    assert_eq!(parse_numeric(&OscArg::Float(1.5)).unwrap(), 1.5);
    assert_eq!(parse_numeric(&OscArg::Double(-2.25)).unwrap(), -2.25);
    assert_eq!(parse_numeric(&OscArg::Int(7)).unwrap(), 7.0);
}
```

- [ ] **Step 2: Write failing state mirror tests**

Add tests to `src-tauri/src/lv1/state.rs`:

```rust
#[tokio::test]
async fn mirrors_aux_send_gain_and_on_state() {
    let bus = AppEventBus::new();
    let mut state = ActorState::new(bus, 1);

    handle_message(&mut state, &crate::lv1::osc::OscMessage {
        address: "/Notify/Aux/Send/On".to_string(),
        args: vec![OscArg::Int(0), OscArg::Int(1), OscArg::Int(2), OscArg::True],
    });
    handle_message(&mut state, &crate::lv1::osc::OscMessage {
        address: "/Notify/Aux/Send/Gain".to_string(),
        args: vec![OscArg::Int(0), OscArg::Int(1), OscArg::Int(2), OscArg::Double(-6.0)],
    });

    let snapshot = state.snapshot();
    let send = snapshot.aux_sends.get(&(0, 1, 2)).unwrap();
    assert_eq!(send.on, Some(true));
    assert_eq!(send.gain_db, Some(-6.0));
}

#[tokio::test]
async fn mirrors_tempo_mute_group_and_spill_state() {
    let bus = AppEventBus::new();
    let mut state = ActorState::new(bus, 1);

    handle_message(&mut state, &crate::lv1::osc::OscMessage {
        address: "/Notify/Tempo".to_string(),
        args: vec![OscArg::Float(120.5)],
    });
    handle_message(&mut state, &crate::lv1::osc::OscMessage {
        address: "/Notify/MuteGroup".to_string(),
        args: vec![OscArg::Int(1), OscArg::True],
    });
    handle_message(&mut state, &crate::lv1::osc::OscMessage {
        address: "/Notify/SpillButton".to_string(),
        args: vec![OscArg::Int(0), OscArg::Int(2), OscArg::Int(1)],
    });

    let snapshot = state.snapshot();
    assert_eq!(snapshot.tempo_bpm, Some(120.5));
    assert_eq!(snapshot.mute_groups.get(&1), Some(&true));
    assert_eq!(snapshot.spill_buttons.get(&(0, 2)), Some(&1));
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control lv1::parsers lv1::state`

Expected: FAIL because new state fields and parsers do not exist.

- [ ] **Step 4: Add serializable state types and snapshot fields**

In `src-tauri/src/lv1/types.rs`, add `Serialize` and `Deserialize` derives to public snapshot DTOs that the CLI will output. Add these focused types:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxTrackInfo {
    pub index: i32,
    pub group: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxSendState {
    pub source_group: i32,
    pub source_channel: i32,
    pub aux_index: i32,
    pub on: Option<bool>,
    pub gain_db: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserKeyInfo {
    pub index: i32,
    pub short_name: String,
    pub function: String,
    pub assigned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalAssignState {
    pub group: i32,
    pub channel: i32,
    pub assign_type: i32,
    pub sub: i32,
    pub state: i32,
    pub valid: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerSlot {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub page: i32,
    pub is_custom: bool,
    pub slots: Vec<LayerSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentLayerState {
    pub mixer_page: i32,
    pub layer_index: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Lv1DiagnosticsState {
    pub service_version: Option<String>,
    pub current_session_id: Option<String>,
    pub number_of_channels: Option<i32>,
}
```

Extend `Lv1StateSnapshot` with map fields. Use `std::collections::BTreeMap` so JSON output order is deterministic:

```rust
pub aux_tracks: Vec<AuxTrackInfo>,
pub aux_sends: BTreeMap<(i32, i32, i32), AuxSendState>,
pub solo_states: BTreeMap<(i32, i32), bool>,
pub user_keys: BTreeMap<i32, UserKeyInfo>,
pub spill_buttons: BTreeMap<(i32, i32), i32>,
pub mute_groups: BTreeMap<i32, bool>,
pub internal_assigns: BTreeMap<(i32, i32, i32, i32), InternalAssignState>,
pub tempo_bpm: Option<f64>,
pub layers: BTreeMap<(i32, bool), LayerInfo>,
pub current_layer: Option<CurrentLayerState>,
pub diagnostics: Lv1DiagnosticsState,
```

- [ ] **Step 5: Implement parser helpers**

In `src-tauri/src/lv1/parsers.rs`, implement the helpers referenced by tests:

```rust
pub fn parse_numeric(arg: &OscArg) -> Result<f64, &'static str> {
    match arg {
        OscArg::Float(value) => Ok(f64::from(*value)),
        OscArg::Double(value) => Ok(*value),
        OscArg::Int(value) => Ok(f64::from(*value)),
        _ => Err("numeric value must be float, double, or int"),
    }
}
```

Implement `parse_aux_tracks`, `parse_user_key_info`, and simple parse helpers for internal assign, current layer, and layer payloads. For `/Notify/Layers`, preserve known fields and parse `(group, channel)` pairs after the header as slots; keep names out until their exact payload position is confirmed by hardware smoke logs.

- [ ] **Step 6: Store new authoritative state in ActorState**

In `src-tauri/src/lv1/state.rs`, add the same fields to `ActorState`, initialize them in `ActorState::new`, clone them in `snapshot`, and add `handle_message` branches for:

```rust
"/Aux/Tracks"
"/Notify/Aux/Send/On"
"/Notify/Aux/Send/Gain"
"/Notify/Solo"
"/Notify/UserKeyInfo"
"/Notify/SpillButton"
"/Notify/MuteGroup"
"/Notify/InternalAssign"
"/Notify/Tempo"
"/Notify/Layers"
"/Notify/CurrentLayer"
"/Notify/ServiceVersion"
"/Notify/CurrentSessionID"
"/Notify/NumberOfChannels"
```

For aux sends, update an existing entry or create one with the tuple keys and only the published field set. Do not infer aux send pan.

- [ ] **Step 7: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control lv1::parsers lv1::state`

Expected: PASS.

- [ ] **Step 8: Run lint target for changed Rust code and commit**

Run: `cargo clippy -p advanced-show-control --all-targets -- -D warnings`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/lv1/types.rs src-tauri/src/lv1/parsers.rs src-tauri/src/lv1/state.rs src-tauri/src/lv1/events.rs
git commit -m "feat: mirror documented lv1 state"
```

---

### Task 3: LV1 Write Command Mapping For Documented OSC

**Files:**
- Create: `src-tauri/src/asc_cli/commands.rs`
- Modify: `src-tauri/src/asc_cli/mod.rs`
- Modify: `src-tauri/src/asc_cli/protocol.rs`
- Modify: `src-tauri/src/lv1/commands.rs`
- Modify: `src-tauri/src/lv1/actor.rs`
- Modify: `src-tauri/src/lv1/tcp.rs`
- Modify: `src-tauri/src/lv1/mod.rs`

**Interfaces:**
- Consumes: `AscCommand` variants from Task 1.
- Produces: `asc_cli::commands::to_lv1_command(command: &AscCommand, reply: Option<oneshot::Sender<Result<(), Lv1ActorError>>>) -> Result<Lv1Command, AscError>` for actor-routed writes.
- Produces: `Lv1Command::SendOsc { address: String, args: Vec<OscArg>, reply: Option<oneshot::Sender<Result<(), Lv1ActorError>>> }` for raw and less common documented writes.

- [ ] **Step 1: Write failing command mapping tests**

Create `src-tauri/src/asc_cli/commands.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc_cli::protocol::{AscCommand, TrackParameterCommand};
    use crate::lv1::commands::Lv1Command;
    use crate::lv1::osc::OscArg;

    #[test]
    fn maps_track_gain_to_existing_lv1_command() {
        let command = AscCommand::Track(TrackParameterCommand::Gain { group: 0, channel: 1, db: -12.0 });

        let mapped = to_lv1_command(&command, None).unwrap();

        match mapped {
            Lv1Command::SetGain { group, channel, gain_db, .. } => {
                assert_eq!((group, channel, gain_db), (0, 1, -12.0));
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn maps_raw_osc_to_send_osc_command() {
        let command = AscCommand::RawOsc {
            address: "/Set/Track/Out/Gain".to_string(),
            args: vec!["i:0".to_string(), "i:1".to_string(), "d:-12".to_string()],
        };

        let mapped = to_lv1_command(&command, None).unwrap();

        match mapped {
            Lv1Command::SendOsc { address, args, .. } => {
                assert_eq!(address, "/Set/Track/Out/Gain");
                assert_eq!(args, vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(-12.0)]);
            }
            _ => panic!("unexpected command variant"),
        }
    }
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::commands`

Expected: FAIL because command variants and mapper do not exist.

- [ ] **Step 3: Expand protocol command enums**

In `src-tauri/src/asc_cli/protocol.rs`, add serializable command enums for documented writes:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum TrackParameterCommand {
    Gain { group: i32, channel: i32, db: f64 },
    Mute { group: i32, channel: i32, on: bool },
    Solo { group: i32, channel: i32, on: bool },
    Pan { group: i32, channel: i32, degrees: f64 },
    Balance { group: i32, channel: i32, degrees: f64 },
    Width { group: i32, channel: i32, value: f64 },
    Name { group: i32, channel: i32, name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum AuxCommand {
    Tracks,
    SendOn { source_group: i32, source_channel: i32, aux: i32, on: bool },
    SendGain { source_group: i32, source_channel: i32, aux: i32, db: f64 },
    SendPan { source_group: i32, source_channel: i32, aux: i32, pan: f64 },
}
```

Add `SceneRecall`, `UserKeySet`, `SpillSet`, `MuteGroupSet`, and `TempoTap` variants to `AscCommand`.

- [ ] **Step 4: Add raw OSC support to LV1 actor**

In `src-tauri/src/lv1/commands.rs`, add:

```rust
SendOsc {
    address: String,
    args: Vec<crate::lv1::osc::OscArg>,
    reply: Option<oneshot::Sender<Result<(), Lv1ActorError>>>,
},
```

In `src-tauri/src/lv1/actor.rs`, route `SendOsc` in disconnected drain paths like other mutating commands, and in the connected command handling encode it with `encode_frame(&address, &args)` and enqueue the bytes.

- [ ] **Step 5: Implement `to_lv1_command` mapping**

In `src-tauri/src/asc_cli/mod.rs`, add `pub mod commands;`.

In `src-tauri/src/asc_cli/commands.rs`, map named commands to existing `SetGain`, `SetPan`, `SetBalance`, `SetWidth`, `SetMute`, `RecallScene` where available, and use `SendOsc` for documented commands without dedicated variants:

```rust
pub fn to_lv1_command(
    command: &AscCommand,
    reply: Option<tokio::sync::oneshot::Sender<Result<(), crate::lv1::Lv1ActorError>>>,
) -> Result<crate::lv1::Lv1Command, AscError> {
    match command {
        AscCommand::RawOsc { address, args } => Ok(crate::lv1::Lv1Command::SendOsc {
            address: address.clone(),
            args: crate::asc_cli::osc_args::parse_osc_args(args)?,
            reply,
        }),
        AscCommand::Track(TrackParameterCommand::Gain { group, channel, db }) => Ok(crate::lv1::Lv1Command::SetGain {
            group: *group,
            channel: *channel,
            gain_db: *db,
            reply,
        }),
        AscCommand::Track(TrackParameterCommand::Name { group, channel, name }) => Ok(crate::lv1::Lv1Command::SendOsc {
            address: "/Set/TrackName".to_string(),
            args: vec![OscArg::Int(*group), OscArg::Int(*channel), OscArg::String(name.clone())],
            reply,
        }),
        other => map_remaining_documented_write(other, reply),
    }
}
```

Implement `map_remaining_documented_write` with exact addresses from `docs/lv1-osc.md`: `/Set/Solo`, `/ClearAllSolo`, `/Get/Aux/Tracks`, `/Set/Aux/Send/On`, `/Set/Aux/Send/Gain`, `/Set/Aux/Send/Pan`, `/Set/UserKey`, `/Set/SpillButton`, `/Set/MuteGroup`, and `/TapTempo`.

- [ ] **Step 6: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control asc_cli::commands lv1::handle lv1::actor`

Expected: PASS.

- [ ] **Step 7: Run clippy and commit**

Run: `cargo clippy -p advanced-show-control --all-targets -- -D warnings`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/asc_cli/protocol.rs src-tauri/src/asc_cli/commands.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/lv1/commands.rs src-tauri/src/lv1/actor.rs src-tauri/src/lv1/tcp.rs src-tauri/src/lv1/mod.rs
git commit -m "feat: map asc commands to lv1 osc"
```

---

### Task 4: Authoritative Read Query Helpers

**Files:**
- Create: `src-tauri/src/asc_cli/queries.rs`
- Modify: `src-tauri/src/asc_cli/mod.rs`
- Modify: `src-tauri/src/asc_cli/protocol.rs`

**Interfaces:**
- Consumes: expanded `Lv1StateSnapshot` from Task 2.
- Produces: `asc_cli::queries::query_snapshot(snapshot: &Lv1StateSnapshot, query: &ReadQuery) -> Result<serde_json::Value, AscError>`.
- Produces: `ReadQuery` variants for scenes, current scene, channels, track values, aux tracks, aux send state, user keys, spill, mute groups, tempo, full state, and watch setup.

- [ ] **Step 1: Write failing query tests**

Create `src-tauri/src/asc_cli/queries.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, PanMode};

    fn snapshot_with_channel() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: vec![ChannelInfo {
                group: 0,
                channel: 1,
                name: "Lead Vox".to_string(),
                gain_db: -12.0,
                muted: false,
                pan: Some(5.0),
                balance: None,
                width: None,
                pan_mode: Some(PanMode::Mono),
            }],
            ..Lv1StateSnapshot::default_for_tests()
        }
    }

    #[test]
    fn returns_track_gain_from_authoritative_snapshot() {
        let value = query_snapshot(&snapshot_with_channel(), &ReadQuery::TrackGain { group: 0, channel: 1 }).unwrap();
        assert_eq!(value, serde_json::json!({ "group": 0, "channel": 1, "db": -12.0 }));
    }

    #[test]
    fn unavailable_read_fails_without_inference() {
        let err = query_snapshot(&snapshot_with_channel(), &ReadQuery::TrackWidth { group: 0, channel: 1 }).unwrap_err();
        assert_eq!(err.code, AscErrorCode::StateUnavailable);
    }

    #[test]
    fn unsupported_aux_send_pan_read_fails() {
        let err = query_snapshot(&snapshot_with_channel(), &ReadQuery::AuxSendPan { source_group: 0, source_channel: 1, aux: 2 }).unwrap_err();
        assert_eq!(err.code, AscErrorCode::UnsupportedRead);
    }
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::queries`

Expected: FAIL because query types and test default snapshot helper do not exist.

- [ ] **Step 3: Add `ReadQuery` protocol variants**

In `src-tauri/src/asc_cli/protocol.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "camelCase")]
pub enum ReadQuery {
    Scenes,
    CurrentScene,
    Channels,
    Track { group: i32, channel: i32 },
    TrackGain { group: i32, channel: i32 },
    TrackMute { group: i32, channel: i32 },
    TrackSolo { group: i32, channel: i32 },
    TrackPan { group: i32, channel: i32 },
    TrackBalance { group: i32, channel: i32 },
    TrackWidth { group: i32, channel: i32 },
    AuxTracks,
    AuxSendOn { source_group: i32, source_channel: i32, aux: i32 },
    AuxSendGain { source_group: i32, source_channel: i32, aux: i32 },
    AuxSendPan { source_group: i32, source_channel: i32, aux: i32 },
    UserKeys,
    Spill { bank: i32, slot: i32 },
    MuteGroups,
    Tempo,
    State,
}
```

Add `AscCommand::Read { query: ReadQuery }`.

- [ ] **Step 4: Implement query helpers**

In `src-tauri/src/asc_cli/mod.rs`, add `pub mod queries;`.

In `src-tauri/src/lv1/types.rs`, add a test-only helper used by query tests:

```rust
#[cfg(test)]
impl Lv1StateSnapshot {
    pub fn default_for_tests() -> Self {
        Self {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            aux_tracks: Vec::new(),
            aux_sends: Default::default(),
            solo_states: Default::default(),
            user_keys: Default::default(),
            spill_buttons: Default::default(),
            mute_groups: Default::default(),
            internal_assigns: Default::default(),
            tempo_bpm: None,
            layers: Default::default(),
            current_layer: None,
            diagnostics: Default::default(),
        }
    }
}
```

Implement `query_snapshot` so every read either returns a serialized authoritative value or an `AscError` with `StateUnavailable` or `UnsupportedRead`. `ReadQuery::AuxSendPan` must always return `UnsupportedRead` until protocol mapping is confirmed.

- [ ] **Step 5: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control asc_cli::queries`

Expected: PASS.

- [ ] **Step 6: Commit**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/asc_cli/queries.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/asc_cli/protocol.rs src-tauri/src/lv1/types.rs
git commit -m "feat: add asc authoritative read queries"
```

---

### Task 5: Unix Socket IPC And Daemon Runtime

**Files:**
- Create: `src-tauri/src/asc_cli/paths.rs`
- Create: `src-tauri/src/asc_cli/ipc.rs`
- Create: `src-tauri/src/asc_cli/daemon.rs`
- Create: `src-tauri/src/bin/ascd.rs`
- Modify: `src-tauri/src/asc_cli/mod.rs`

**Interfaces:**
- Consumes: `AscRequest`, `AscResponse`, `AscCommand`, `commands::to_lv1_command`, and `queries::query_snapshot`.
- Produces: `asc_cli::daemon::run_daemon(options: DaemonOptions) -> Result<(), AscError>`.
- Produces: `asc_cli::ipc::{send_request, serve}` helpers over newline-delimited JSON Unix sockets.

- [ ] **Step 1: Write failing IPC round-trip test**

Create `src-tauri/src/asc_cli/ipc.rs` with this test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc_cli::protocol::{AscCommand, AscRequest, OutputMode};

    #[tokio::test]
    async fn sends_one_json_request_and_reads_one_json_response() {
        let path = std::env::temp_dir().join(format!("asc-ipc-test-{}.sock", std::process::id()));
        let listener = tokio::net::UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let request = read_request(stream).await.unwrap();
            assert_eq!(request.id, "req-1");
            AscResponse::ok(request.id, serde_json::json!({ "connected": true }))
        });

        let response = send_request(&path, &AscRequest {
            id: "req-1".to_string(),
            output: OutputMode::Json,
            command: AscCommand::Status,
        }).await.unwrap();

        assert!(response.ok);
        assert_eq!(server.await.unwrap().id, "req-1");
        let _ = std::fs::remove_file(path);
    }
}
```

- [ ] **Step 2: Write failing daemon request dispatch test with fake state**

In `src-tauri/src/asc_cli/daemon.rs`, write a pure or actor-style test that constructs a `DaemonState` with no selected target and verifies `Status` returns a response and mutating commands return `target_not_selected`.

```rust
#[tokio::test]
async fn daemon_status_works_without_selected_target() {
    let mut state = DaemonState::new_for_tests();
    let response = state.handle_request(AscRequest {
        id: "status".to_string(),
        output: OutputMode::Json,
        command: AscCommand::Status,
    }).await;

    assert!(response.ok);
    assert_eq!(response.id, "status");
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::ipc asc_cli::daemon`

Expected: FAIL because IPC and daemon runtime do not exist.

- [ ] **Step 4: Implement path and IPC helpers**

In `src-tauri/src/asc_cli/paths.rs`, create `AscPaths` with `config_dir`, `state_path`, `log_dir`, and `socket_path`. Use `dirs::config_dir()` for config and `std::env::temp_dir()` for a per-user socket path containing the numeric UID when available. On macOS, `std::env::var("USER")` is acceptable for the first per-user suffix.

In `ipc.rs`, implement newline-delimited JSON using `tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader}`. Each request and response is one JSON line.

- [ ] **Step 5: Implement daemon runtime skeleton**

In `daemon.rs`, implement:

```rust
pub struct DaemonOptions {
    pub paths: AscPaths,
}

pub async fn run_daemon(options: DaemonOptions) -> Result<(), AscError> {
    let state = DaemonState::load(options.paths).await?;
    serve_daemon_socket(state).await
}
```

`DaemonState` should load persisted selected target JSON if present, keep optional `Lv1ActorHandle`, and dispatch:

- `Status`: return daemon/LV1 status JSON.
- `Read`: get LV1 snapshot via `Lv1Command::GetState`, then call `query_snapshot`.
- Mutating commands: require selected target and connected actor, map to `Lv1Command`, send, flush when a reply is available.
- `DaemonStop`: return success and break server loop.

- [ ] **Step 6: Add `ascd` binary**

Create `src-tauri/src/bin/ascd.rs`:

```rust
use advanced_show_control::asc_cli::daemon::{run_daemon, DaemonOptions};
use advanced_show_control::asc_cli::paths::AscPaths;

#[tokio::main]
async fn main() {
    if let Err(err) = run_daemon(DaemonOptions { paths: AscPaths::default() }).await {
        eprintln!("ascd failed: {}", err.message);
        std::process::exit(1);
    }
}
```

- [ ] **Step 7: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control asc_cli::ipc asc_cli::daemon`

Expected: PASS.

- [ ] **Step 8: Build daemon binary and commit**

Run: `cargo build -p advanced-show-control --bin ascd`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/asc_cli/paths.rs src-tauri/src/asc_cli/ipc.rs src-tauri/src/asc_cli/daemon.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/bin/ascd.rs
git commit -m "feat: add asc daemon ipc runtime"
```

---

### Task 6: ASC CLI Parser, Auto-Start Client, And Formatting

**Files:**
- Create: `src-tauri/src/asc_cli/cli.rs`
- Create: `src-tauri/src/asc_cli/client.rs`
- Create: `src-tauri/src/bin/asc.rs`
- Modify: `src-tauri/src/asc_cli/mod.rs`

**Interfaces:**
- Consumes: `AscRequest`, `AscCommand`, `ReadQuery`, IPC `send_request`, and `AscPaths`.
- Produces: `asc_cli::cli::parse_cli_from<I, T>(args: I) -> Result<Cli, clap::Error>`.
- Produces: `asc_cli::client::run_cli_from<I, T>(args: I) -> i32`.

- [ ] **Step 1: Write failing CLI parser tests**

Create `src-tauri/src/asc_cli/cli.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_gain_command() {
        let cli = parse_cli_from([
            "asc", "track", "gain", "--group", "0", "--channel", "1", "--db", "-12",
        ]).unwrap();

        let request = cli.into_request("req-1".to_string()).unwrap();
        assert!(matches!(request.command, AscCommand::Track(TrackParameterCommand::Gain { group: 0, channel: 1, db }) if db == -12.0));
    }

    #[test]
    fn parses_raw_osc_send_command() {
        let cli = parse_cli_from([
            "asc", "osc", "send", "/Set/Track/Out/Gain", "i:0", "i:1", "d:-12",
        ]).unwrap();

        let request = cli.into_request("req-2".to_string()).unwrap();
        assert!(matches!(request.command, AscCommand::RawOsc { .. }));
    }

    #[test]
    fn parses_json_read_command() {
        let cli = parse_cli_from(["asc", "--json", "get", "tempo"]).unwrap();
        let request = cli.into_request("req-3".to_string()).unwrap();
        assert_eq!(request.output, OutputMode::Json);
        assert!(matches!(request.command, AscCommand::Read { query: ReadQuery::Tempo }));
    }
}
```

- [ ] **Step 2: Write failing formatter tests**

In `src-tauri/src/asc_cli/client.rs`, test human and JSON response formatting:

```rust
#[test]
fn formats_json_response_as_single_json_line() {
    let response = AscResponse::ok("req".to_string(), serde_json::json!({ "connected": true }));
    let text = format_response(&response, OutputMode::Json).unwrap();
    assert_eq!(text, "{\"connected\":true}\n");
}

#[test]
fn formats_error_for_humans() {
    let response = AscResponse::error("req".to_string(), AscErrorCode::Lv1NotConnected, "LV1 is not connected".to_string());
    let text = format_response(&response, OutputMode::Human).unwrap();
    assert_eq!(text, "error: LV1 is not connected\n");
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::cli asc_cli::client`

Expected: FAIL because parser and formatter do not exist.

- [ ] **Step 4: Implement Clap parser**

Use `clap::{Parser, Subcommand}` and keep command conversion in `cli.rs`. Include commands from the spec:

- `daemon status`, `daemon stop`
- `discover`
- `connect --host --port` and `connect <discovered-id>`
- `status`
- `scene recall --index`
- `track gain|mute|solo|pan|balance|width|name`
- `aux tracks|send-on|send-gain|send-pan`
- `user-key set`
- `spill set`
- `mute-group set`
- `tempo tap`
- `osc send`
- `get ...`
- `state --json`
- `watch --json`
- `cli-smoke`

Keep `watch` initially implemented as a request that returns an explicit `unsupported_read` response until streaming IPC is added in a later task in this plan.

- [ ] **Step 5: Implement client auto-start**

In `client.rs`, implement:

```rust
pub async fn send_with_auto_start(paths: &AscPaths, request: &AscRequest) -> Result<AscResponse, AscError> {
    match crate::asc_cli::ipc::send_request(&paths.socket_path, request).await {
        Ok(response) => Ok(response),
        Err(_) => {
            start_daemon(paths)?;
            wait_for_socket(paths).await?;
            crate::asc_cli::ipc::send_request(&paths.socket_path, request).await
        }
    }
}
```

Use `std::process::Command::new(current_exe_dir.join("ascd"))` to start the daemon. If the sibling binary is missing in development, fall back to `cargo run --bin ascd` only when `ASC_CLI_DEV_AUTOSTART=1` is set.

- [ ] **Step 6: Add `asc` binary**

Create `src-tauri/src/bin/asc.rs`:

```rust
#[tokio::main]
async fn main() {
    let code = advanced_show_control::asc_cli::client::run_cli_from(std::env::args_os()).await;
    std::process::exit(code);
}
```

- [ ] **Step 7: Run targeted tests and build client**

Run: `cargo nextest run -p advanced-show-control asc_cli::cli asc_cli::client`

Expected: PASS.

Run: `cargo build -p advanced-show-control --bin asc --bin ascd`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/asc_cli/cli.rs src-tauri/src/asc_cli/client.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/bin/asc.rs
git commit -m "feat: add asc cli client"
```

---

### Task 7: Discovery, Target Persistence, Connect, And Reconnect Reuse

**Files:**
- Modify: `src-tauri/src/asc_cli/daemon.rs`
- Modify: `src-tauri/src/asc_cli/protocol.rs`
- Modify: `src-tauri/src/asc_cli/paths.rs`

**Interfaces:**
- Consumes: existing `lv1::discover`, `lv1::resolve_target`, and `lv1::build_actor`.
- Produces: persisted `SelectedTarget { host: String, port: u16, discovered_id: Option<String> }` JSON.
- Produces: daemon command handling for `Discover`, `Connect`, and reconnect on startup.

- [ ] **Step 1: Write failing persistence tests**

In `daemon.rs`, add tests using a temp config directory:

```rust
#[tokio::test]
async fn persists_and_loads_selected_target() {
    let dir = std::env::temp_dir().join(format!("asc-target-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");

    let target = SelectedTarget { host: "192.168.1.10".to_string(), port: 12345, discovered_id: Some("lv1-a".to_string()) };
    save_selected_target(&path, &target).await.unwrap();
    let loaded = load_selected_target(&path).await.unwrap().unwrap();

    assert_eq!(loaded, target);
    let _ = std::fs::remove_dir_all(dir);
}
```

- [ ] **Step 2: Write failing connect dispatch test**

In `daemon.rs`, add a test that sends `AscCommand::Connect { host, port, discovered_id }` to `DaemonState::new_for_tests()` and asserts the selected target is persisted and status includes it. Use a fake actor builder in the test state so no real TCP connection is attempted.

- [ ] **Step 3: Run tests and verify they fail**

Run: `cargo nextest run -p advanced-show-control asc_cli::daemon`

Expected: FAIL because persistence and connect handling are incomplete.

- [ ] **Step 4: Add protocol commands**

In `protocol.rs`, add:

```rust
Discover { timeout_ms: u64 },
Connect { host: String, port: u16, discovered_id: Option<String> },
```

Add response DTOs for discovered systems and selected target.

- [ ] **Step 5: Implement selected target persistence**

Use `tokio::fs::create_dir_all`, `tokio::fs::write`, and `serde_json::to_vec_pretty`. Persist only selected target and daemon preferences, not LV1 mirrored state.

- [ ] **Step 6: Implement connect and startup reconnect**

When `ascd` starts, load the selected target. If present, build the LV1 actor immediately with a new `AppEventBus` and generation `1`. On `Connect`, update the target file, stop/drop the old handle by replacing daemon runtime state, and build a new actor for the selected target.

Use existing actor reconnect behavior for TCP reconnect loops. The daemon should not rediscover for later commands once a target is selected.

- [ ] **Step 7: Implement discover request handling**

Use `lv1::discover(DiscoverOptions { timeout: Duration::from_millis(timeout_ms), filter_host: None })` following the existing `lv1-probe` pattern. Return structured discovery results.

- [ ] **Step 8: Run targeted tests and commit**

Run: `cargo nextest run -p advanced-show-control asc_cli::daemon`

Expected: PASS.

Run: `cargo build -p advanced-show-control --bin asc --bin ascd`

Expected: PASS.

Commit:

```bash
git add src-tauri/src/asc_cli/daemon.rs src-tauri/src/asc_cli/protocol.rs src-tauri/src/asc_cli/paths.rs
git commit -m "feat: persist asc lv1 target"
```

---

### Task 8: Watch, Make Targets, Documentation Updates, And CLI Smoke Runner

**Files:**
- Create: `src-tauri/src/asc_cli/smoke.rs`
- Modify: `src-tauri/src/asc_cli/daemon.rs`
- Modify: `src-tauri/src/asc_cli/client.rs`
- Modify: `src-tauri/src/asc_cli/cli.rs`
- Modify: `src-tauri/src/asc_cli/mod.rs`
- Modify: `src-tauri/src/bin/asc.rs`
- Modify: `Makefile`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: built `asc` and `ascd` binaries.
- Produces: `make cli-smoke` real-hardware verification target.
- Produces: `asc watch` polling output using authoritative snapshots.

- [ ] **Step 1: Write failing smoke command construction tests**

Create `src-tauri/src/asc_cli/smoke.rs` with pure tests for the smoke script steps:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_plan_includes_required_hardware_checks() {
        let steps = smoke_steps("target-id");
        let names: Vec<_> = steps.iter().map(|step| step.name.as_str()).collect();

        assert!(names.contains(&"discover"));
        assert!(names.contains(&"connect"));
        assert!(names.contains(&"status"));
        assert!(names.contains(&"read_gain_before_write"));
        assert!(names.contains(&"named_write_gain"));
        assert!(names.contains(&"read_gain_after_write"));
        assert!(names.contains(&"raw_osc_write"));
        assert!(names.contains(&"daemon_stop"));
        assert!(names.contains(&"auto_start_reconnect"));
    }
}
```

- [ ] **Step 2: Run test and verify it fails**

Run: `cargo nextest run -p advanced-show-control asc_cli::smoke`

Expected: FAIL because smoke module does not exist.

- [ ] **Step 3: Implement `asc watch` as polling snapshots**

Implement `watch` in the client as repeated `ReadQuery::State` requests at a default 500 ms interval. For the first version, it can run until interrupted and print each changed JSON snapshot. Human mode may print compact status lines; JSON mode prints one JSON object per line.

- [ ] **Step 4: Implement smoke runner**

In `smoke.rs`, implement a runner that shells out to the built `asc` binary. The runner should:

- Run `asc discover --json` and parse the first target unless `ASC_CLI_SMOKE_HOST` and `ASC_CLI_SMOKE_PORT` are set.
- Run `asc connect ...`.
- Run `asc status --json` and require connected or connecting with a bounded wait loop.
- Run `asc get track gain --group ${ASC_CLI_SMOKE_GROUP:-0} --channel ${ASC_CLI_SMOKE_CHANNEL:-0} --json`.
- Run `asc track gain` to a test value.
- Poll `asc get track gain` until LV1 publishes the changed value.
- Run `asc osc send /Set/Track/Out/Gain i:<group> i:<channel> d:<restore_or_second_value>`.
- Poll the read command again.
- Run `asc daemon stop`.
- Run `asc status --json` and verify this auto-starts the daemon and reconnects from persisted target.

Write an authoritative report to `logs/cli-smoke-report.txt` with `PASS` or `FAIL` and each command/result.

- [ ] **Step 5: Add Makefile targets**

Modify `Makefile`:

```make
.PHONY: cli cli-smoke

cli:
	cargo build --bin asc --bin ascd

cli-smoke: cli
	cargo run --bin asc -- cli-smoke
```

Add help text for `make cli` and `make cli-smoke`.

- [ ] **Step 6: Update roadmap**

In `docs/roadmap.md`, add one bullet under `MVP Roadmap` after the bundling item: `Add ASC CLI power-tool support with an auto-started daemon, documented LV1 read/write command coverage, and mandatory disposable-hardware smoke verification.` Do not add unrelated release goals.

- [ ] **Step 7: Run targeted tests and build**

Run: `cargo nextest run -p advanced-show-control asc_cli::smoke asc_cli::client asc_cli::cli`

Expected: PASS.

Run: `make cli`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/asc_cli/smoke.rs src-tauri/src/asc_cli/daemon.rs src-tauri/src/asc_cli/client.rs src-tauri/src/asc_cli/cli.rs src-tauri/src/asc_cli/mod.rs src-tauri/src/bin/asc.rs Makefile docs/roadmap.md
git commit -m "feat: add asc cli smoke target"
```

---

### Task 9: Final Verification On Unit Tests And Real LV1 Hardware

**Files:**
- No source changes expected unless verification finds defects.
- If defects are found, fix them in the smallest relevant files and commit with a `fix:` message.

**Interfaces:**
- Consumes: all tasks above.
- Produces: verified ASC CLI feature with passing software checks and real-hardware smoke evidence.

- [ ] **Step 1: Run Rust formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 2: Run Rust lint**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Run Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 4: Build workspace**

Run: `cargo build --workspace`

Expected: PASS.

- [ ] **Step 5: Run the real hardware CLI smoke**

Run: `make cli-smoke`

Expected: PASS and `logs/cli-smoke-report.txt` contains an overall `PASS` result.

If the command output is noisy or truncated, read `logs/cli-smoke-report.txt` before claiming success.

- [ ] **Step 6: Inspect git status and final diff**

Run: `git status --short`

Expected: no unrelated changes. If smoke generated `logs/cli-smoke-report.txt` and the file is intentionally untracked, leave it untracked unless the repo already tracks smoke reports.

Run: `git diff --stat HEAD~8..HEAD`

Expected: only ASC CLI, LV1 mirror, Makefile, and roadmap changes from this plan.

- [ ] **Step 7: Commit verification fixes if any**

If any verification step required fixes, run the relevant targeted tests again and commit:

```bash
git add <fixed-files>
git commit -m "fix: stabilize asc cli verification"
```

- [ ] **Step 8: Prepare final completion summary**

Report:

- The final commit range.
- The exact verification commands run.
- The result from `logs/cli-smoke-report.txt`.
- Any known limitations, especially unsupported authoritative reads such as aux send pan.
