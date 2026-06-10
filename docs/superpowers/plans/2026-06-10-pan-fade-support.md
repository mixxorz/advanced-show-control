# Pan Fade Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add scene-scoped fading for LV1 pan-family parameters while preserving the existing fader safety model.

**Architecture:** Extend the existing fader-only data flow into parameter-aware targets instead of adding a second engine. `Lv1Actor` mirrors pan-family state, `ShowState` stores scene targets, `SceneRecallFader` builds validated `FadeTarget`s, and `FadeEngine` drives all fader/pan-family writes through one scheduler.

**Tech Stack:** Rust core crate `advanced-show-control`, Tauri shell crate `advanced-show-control-tauri`, React/TypeScript UI, Tokio actor channels, serde camelCase JSON.

---

## Reference Spec

Read first: `docs/superpowers/specs/2026-06-10-pan-fade-support-design.md`.

Key protocol facts to preserve:

- Fader: `/Notify/Track/Out/Gain` and `/Set/Track/Out/Gain`.
- Pan: `/Notify/Track/Pan` and `/Set/Track/Pan`, observed `-45..45`.
- Balance: `/Notify/Balance` and `/Set/Track/Pan/Balance`, observed `-45..45`.
- Width: `/Notify/PanArcWidth` and `/Set/Track/Pan/Width`, observed `-1.4..1.4`.
- Stereo default width is `1.0`; active width `0.0` means collapsed mono width.
- Do not infer width availability from the numeric value alone. Use channel mode plus the notification active/valid flag where available.

## File Map

- Modify `src/lv1/types.rs`: add `PanMode` and optional pan-family fields to `ChannelInfo`.
- Modify `src/lv1/state.rs`: parse `/Channels`, `/Notify/Track/Pan`, `/Notify/Balance`, and `/Notify/PanArcWidth` into live state.
- Modify `src/lv1/commands.rs`, `src/lv1/handle.rs`, `src/lv1/actor.rs`: add pan-family set commands.
- Modify `src/runtime/commands.rs`: expose `set_fade_value` or parameter-specific set methods for `FadeEngine`.
- Modify `src/show/types.rs`: add stored `pan`, `balance`, `width`, `pan_mode`, and `scope_toggles.pan`.
- Modify `src/show/capture.rs`: store live pan-family values.
- Modify `src/show/state.rs`: keep scope toggle mutation behavior and tests current.
- Modify `src/scene_recall/policy.rs`: validate enabled scopes and build fader/pan-family targets.
- Modify `src/fade/types.rs`: add `FadeParameter` and generalize `FadeTarget`.
- Modify `src/fade/tick.rs`: generalize `ActiveChannel` into `ActiveTarget` with parameter-aware interpolation/override/send delta.
- Modify `src/fade/state.rs` and `src/fade/actor.rs`: key active fades by `(group, channel, parameter)` and group pan-family overrides.
- Modify `tests/fade_engine.rs`: cover parameter-specific overlap, final sends, and override grouping.
- Modify `src-tauri/src/app_state/view.rs`, `projection.rs`, `show_file.rs`, and mapping tests: project new fields to/from the UI/show file.
- Modify `src-tauri/src/commands.rs`: expose `set_scene_scope_pan_enabled`.
- Modify `ui/src/types.ts`, `ui/src/commands.ts`, `ui/src/components/SceneTab.tsx`: add the `PAN` toggle and display pan-family stored values.
- Modify `docs/architecture.md` and `PHASES.md` only if implementation changes current behavior or phase status beyond the spec.
- Remove or revert experimental `pan-probe` code in `src/main.rs` before final verification unless the user explicitly wants to keep it.

---

### Task 1: Add Stored And Live Pan-Family Types

**Files:**

- Modify: `src/lv1/types.rs`
- Modify: `src/show/types.rs`
- Modify: `src-tauri/src/show_file.rs`
- Modify: `ui/src/types.ts`

- [ ] **Step 1: Add failing Rust serialization tests**

Add tests in `src/show/types.rs` under `mod tests`:

```rust
#[test]
fn scene_config_serializes_pan_family_fields_for_frontend_camel_case() {
    let config = SceneConfig {
        scene_id: "0::Intro".to_string(),
        scene_index: 0,
        scene_name: "Intro".to_string(),
        duration_ms: 1000,
        channel_configs: vec![ChannelConfig {
            group: 0,
            channel: 1,
            fader_db: Some(-6.0),
            pan: Some(-12.0),
            balance: Some(3.0),
            width: Some(1.2),
            pan_mode: Some(crate::lv1::types::PanMode::Stereo),
        }],
        scoped_channels: vec![ChannelRef { group: 0, channel: 1 }],
        scope_toggles: SceneScopeToggles {
            faders: true,
            pan: true,
        },
    };

    let json = serde_json::to_value(config).unwrap();

    assert_eq!(json["channelConfigs"][0]["faderDb"], -6.0);
    assert_eq!(json["channelConfigs"][0]["pan"], -12.0);
    assert_eq!(json["channelConfigs"][0]["balance"], 3.0);
    assert_eq!(json["channelConfigs"][0]["width"], 1.2);
    assert_eq!(json["channelConfigs"][0]["panMode"], "stereo");
    assert_eq!(json["scopeToggles"]["faders"], true);
    assert_eq!(json["scopeToggles"]["pan"], true);
}

#[test]
fn missing_pan_scope_defaults_to_false() {
    let json = r#"
    {
      "sceneId":"0::Intro",
      "sceneIndex":0,
      "sceneName":"Intro",
      "durationMs":1000,
      "channelConfigs":[{"group":0,"channel":1,"faderDb":-6.0}],
      "scopedChannels":[{"group":0,"channel":1}],
      "scopeToggles":{"faders":true}
    }
    "#;

    let config: SceneConfig = serde_json::from_str(json).unwrap();

    assert!(config.scope_toggles.faders);
    assert!(!config.scope_toggles.pan);
    assert_eq!(config.channel_configs[0].pan, None);
    assert_eq!(config.channel_configs[0].balance, None);
    assert_eq!(config.channel_configs[0].width, None);
    assert_eq!(config.channel_configs[0].pan_mode, None);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control show::types::tests -- --nocapture`

Expected: compile failure because `PanMode`, `ChannelConfig.pan`, `ChannelConfig.balance`, `ChannelConfig.width`, `ChannelConfig.pan_mode`, and `SceneScopeToggles.pan` do not exist.

- [ ] **Step 3: Implement the stored and live types**

In `src/lv1/types.rs`, add:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanMode {
    Mono,
    Stereo,
}
```

Update `ChannelInfo` in `src/lv1/types.rs`:

```rust
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
    pub muted: bool,
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<PanMode>,
}
```

Update `ChannelConfig` and `SceneScopeToggles` in `src/show/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
    #[serde(default)]
    pub pan: Option<f64>,
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub pan_mode: Option<crate::lv1::types::PanMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneScopeToggles {
    pub faders: bool,
    #[serde(default)]
    pub pan: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self {
            faders: true,
            pan: false,
        }
    }
}
```

Update `ui/src/types.ts`:

```ts
export type PanMode = "mono" | "stereo";

export type ChannelConfig = {
  group: number;
  channel: number;
  faderDb: number | null;
  pan: number | null;
  balance: number | null;
  width: number | null;
  panMode: PanMode | null;
};

export type SceneScopeToggles = {
  faders: boolean;
  pan: boolean;
};
```

- [ ] **Step 4: Fix compile errors from new fields**

Every `ChannelInfo` literal must include:

```rust
pan: None,
balance: None,
width: None,
pan_mode: None,
```

Every fader-only `ChannelConfig` literal must include:

```rust
pan: None,
balance: None,
width: None,
```

Every explicit `SceneScopeToggles { faders: value }` literal must become:

```rust
SceneScopeToggles {
    faders: value,
    pan: false,
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p advanced-show-control show::types::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lv1/types.rs src/show/types.rs ui/src/types.ts
git commit -m "feat: add pan family scene types"
```

---

### Task 2: Parse Pan-Family LV1 State

**Files:**

- Modify: `src/lv1/state.rs`
- Modify: `src/lv1/types.rs`
- Test: existing `#[cfg(test)]` tests in `src/lv1/state.rs`

- [ ] **Step 1: Add failing LV1 state tests**

Add tests in `src/lv1/state.rs`:

```rust
#[test]
fn parses_pan_balance_and_width_notifications() {
    let mut state = Lv1State::default();
    state.channels = vec![ChannelInfo {
        group: 0,
        channel: 4,
        name: "Stereo".to_string(),
        gain_db: -6.0,
        muted: false,
        pan: None,
        balance: None,
        width: None,
        pan_mode: Some(PanMode::Stereo),
    }];

    state.handle_osc_message("/Notify/Track/Pan", &[OscArg::Int(0), OscArg::Int(4), OscArg::Double(-12.0), OscArg::Int(1)]);
    state.handle_osc_message("/Notify/Balance", &[OscArg::Int(0), OscArg::Int(4), OscArg::Double(3.0), OscArg::Int(1)]);
    state.handle_osc_message("/Notify/PanArcWidth", &[OscArg::Int(0), OscArg::Int(4), OscArg::Double(1.2), OscArg::Int(1)]);

    let channel = &state.channels[0];
    assert_eq!(channel.pan, Some(-12.0));
    assert_eq!(channel.balance, Some(3.0));
    assert_eq!(channel.width, Some(1.2));
}

#[test]
fn ignores_inactive_width_notification() {
    let mut state = Lv1State::default();
    state.channels = vec![ChannelInfo {
        group: 0,
        channel: 4,
        name: "Mono".to_string(),
        gain_db: -6.0,
        muted: false,
        pan: None,
        balance: None,
        width: None,
        pan_mode: Some(PanMode::Mono),
    }];

    state.handle_osc_message("/Notify/PanArcWidth", &[OscArg::Int(0), OscArg::Int(4), OscArg::Double(857.142857), OscArg::Int(0)]);

    assert_eq!(state.channels[0].width, None);
}
```

Use the actual helper names already present in `src/lv1/state.rs`; if `handle_osc_message` is named differently, keep the same assertions and wire them through the existing parser entry point.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control lv1::state::tests -- --nocapture`

Expected: FAIL because pan-family notifications are not parsed yet.

- [ ] **Step 3: Implement notification parsing**

In `src/lv1/state.rs`, add match arms equivalent to:

```rust
"/Notify/Track/Pan" => {
    if let (Some(group), Some(channel), Some(value)) = (int_arg(args, 0), int_arg(args, 1), number_arg(args, 2)) {
        update_channel(channels, group, channel, |ch| ch.pan = Some(value));
    }
}
"/Notify/Balance" => {
    if let (Some(group), Some(channel), Some(value)) = (int_arg(args, 0), int_arg(args, 1), number_arg(args, 2)) {
        update_channel(channels, group, channel, |ch| ch.balance = Some(value));
    }
}
"/Notify/PanArcWidth" => {
    if let (Some(group), Some(channel), Some(value), Some(active)) = (int_arg(args, 0), int_arg(args, 1), number_arg(args, 2), int_arg(args, 3)) {
        if active != 0 {
            update_channel(channels, group, channel, |ch| ch.width = Some(value));
        }
    }
}
```

Use existing parser helper names from `src/lv1/state.rs`. Do not add special handling for sentinel numeric values.

- [ ] **Step 4: Parse `/Channels` field mapping**

When constructing `ChannelInfo` from `/Channels`, populate:

```rust
let pan_mode = match mode_field {
    1 => Some(PanMode::Mono),
    2 => Some(PanMode::Stereo),
    _ => None,
};
let pan = Some(pan_field);
let balance = if pan_mode == Some(PanMode::Stereo) { Some(field_4) } else { None };
let width = None;
```

Keep width sourced from `/Notify/PanArcWidth`, not `/Channels`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p advanced-show-control lv1::state::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lv1/state.rs src/lv1/types.rs
git commit -m "feat: mirror pan family lv1 state"
```

---

### Task 3: Add LV1 Pan-Family Write Commands

**Files:**

- Modify: `src/lv1/commands.rs`
- Modify: `src/lv1/handle.rs`
- Modify: `src/lv1/actor.rs`
- Modify: `src/runtime/commands.rs`

- [ ] **Step 1: Add failing command bus tests**

In `src/runtime/commands.rs`, add tests beside existing `set_gain` tests:

```rust
#[tokio::test]
async fn set_pan_routes_to_lv1_actor() {
    let event_bus = AppEventBus::new();
    let command_bus = AppCommandBus::new(event_bus);
    let (lv1_handle, mut lv1_rx) = test_lv1_handle();
    command_bus.set_lv1(Some(lv1_handle)).await;

    let result = command_bus.set_pan(0, 4, -12.0).await;

    assert!(result.is_ok());
    assert!(matches!(lv1_rx.recv().await.unwrap(), Lv1Command::SetPan { group: 0, channel: 4, value, .. } if value == -12.0));
}

#[tokio::test]
async fn set_balance_routes_to_lv1_actor() {
    let event_bus = AppEventBus::new();
    let command_bus = AppCommandBus::new(event_bus);
    let (lv1_handle, mut lv1_rx) = test_lv1_handle();
    command_bus.set_lv1(Some(lv1_handle)).await;

    let result = command_bus.set_balance(0, 4, 3.0).await;

    assert!(result.is_ok());
    assert!(matches!(lv1_rx.recv().await.unwrap(), Lv1Command::SetBalance { group: 0, channel: 4, value, .. } if value == 3.0));
}

#[tokio::test]
async fn set_width_routes_to_lv1_actor() {
    let event_bus = AppEventBus::new();
    let command_bus = AppCommandBus::new(event_bus);
    let (lv1_handle, mut lv1_rx) = test_lv1_handle();
    command_bus.set_lv1(Some(lv1_handle)).await;

    let result = command_bus.set_width(0, 4, 1.2).await;

    assert!(result.is_ok());
    assert!(matches!(lv1_rx.recv().await.unwrap(), Lv1Command::SetWidth { group: 0, channel: 4, value, .. } if value == 1.2));
}
```

Use the existing helper name for a fake LV1 handle in this file. If it is not named `test_lv1_handle`, reuse the existing helper rather than creating a duplicate.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control runtime::commands::tests -- --nocapture`

Expected: compile failure because the new commands and methods do not exist.

- [ ] **Step 3: Add LV1 commands and handle methods**

Add to `Lv1Command` in `src/lv1/commands.rs`:

```rust
SetPan {
    group: i32,
    channel: i32,
    value: f64,
    reply: oneshot::Sender<Result<(), Lv1ActorError>>,
},
SetBalance {
    group: i32,
    channel: i32,
    value: f64,
    reply: oneshot::Sender<Result<(), Lv1ActorError>>,
},
SetWidth {
    group: i32,
    channel: i32,
    value: f64,
    reply: oneshot::Sender<Result<(), Lv1ActorError>>,
},
```

Add matching methods to `Lv1ActorHandle` in `src/lv1/handle.rs` with the same reply pattern as `set_gain`.

- [ ] **Step 4: Send correct OSC addresses in actor**

In `src/lv1/actor.rs`, handle the commands with:

```rust
client.send("/Set/Track/Pan", &[OscArg::Int(group), OscArg::Int(channel), OscArg::Double(value)]).await
client.send("/Set/Track/Pan/Balance", &[OscArg::Int(group), OscArg::Int(channel), OscArg::Double(value)]).await
client.send("/Set/Track/Pan/Width", &[OscArg::Int(group), OscArg::Int(channel), OscArg::Double(value)]).await
```

Use the existing error mapping path for `SetGain`.

- [ ] **Step 5: Add runtime command bus methods**

In `src/runtime/commands.rs`, add:

```rust
pub async fn set_pan(&self, group: i32, channel: i32, value: f64) -> Result<(), AppCommandError> { /* same pattern as set_gain */ }
pub async fn set_balance(&self, group: i32, channel: i32, value: f64) -> Result<(), AppCommandError> { /* same pattern as set_gain */ }
pub async fn set_width(&self, group: i32, channel: i32, value: f64) -> Result<(), AppCommandError> { /* same pattern as set_gain */ }
```

Use `publish_failure(&self.event_bus, "set_pan", &result)`, `"set_balance"`, and `"set_width"` respectively.

- [ ] **Step 6: Run tests**

Run: `cargo test -p advanced-show-control runtime::commands::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lv1/commands.rs src/lv1/handle.rs src/lv1/actor.rs src/runtime/commands.rs
git commit -m "feat: add pan family lv1 commands"
```

---

### Task 4: Refactor Fade Engine To Parameter-Aware Fader Targets

**Files:**

- Modify: `src/fade/types.rs`
- Modify: `src/fade/tick.rs`
- Modify: `src/fade/state.rs`
- Modify: `src/fade/actor.rs`
- Modify: `tests/fade_engine.rs`
- Modify: call sites in `src/scene_recall/policy.rs`, `src/scene_recall/actor.rs`, `src/runtime/commands.rs`, and `src/main.rs` if still using `FadeTarget { target_db }`.

- [ ] **Step 1: Add failing identity test**

In `tests/fade_engine.rs`, add:

```rust
#[tokio::test]
async fn fader_targets_are_keyed_by_parameter() {
    let target = FadeTarget {
        group: 0,
        channel: 4,
        parameter: FadeParameter::FaderDb,
        target: -12.0,
    };

    assert_eq!(target.key(), FadeTargetKey { group: 0, channel: 4, parameter: FadeParameter::FaderDb });
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control --test fade_engine fader_targets_are_keyed_by_parameter -- --nocapture`

Expected: compile failure because `FadeParameter`, `FadeTargetKey`, `FadeTarget.parameter`, `FadeTarget.target`, and `FadeTarget::key` do not exist.

- [ ] **Step 3: Add parameter-aware target types**

Replace `FadeTarget` in `src/fade/types.rs` with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FadeParameter {
    FaderDb,
    Pan,
    Balance,
    Width,
}

impl FadeParameter {
    pub fn is_pan_family(self) -> bool {
        matches!(self, Self::Pan | Self::Balance | Self::Width)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FadeTargetKey {
    pub group: i32,
    pub channel: i32,
    pub parameter: FadeParameter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub parameter: FadeParameter,
    pub target: f64,
}

impl FadeTarget {
    pub fn key(&self) -> FadeTargetKey {
        FadeTargetKey {
            group: self.group,
            channel: self.channel,
            parameter: self.parameter,
        }
    }
}
```

- [ ] **Step 4: Refactor tick state without behavior change**

Rename `ActiveChannel` to `ActiveTarget` and fields from `*_db` to generic values:

```rust
pub(crate) struct ActiveTarget {
    pub(crate) scene: FadeSceneIdentity,
    pub(crate) group: i32,
    pub(crate) channel: i32,
    pub(crate) parameter: FadeParameter,
    pub(crate) start: f64,
    pub(crate) target: f64,
    pub(crate) expected: f64,
    pub(crate) curve: FadeCurve,
    pub(crate) duration: Duration,
    pub(crate) started_at: Instant,
}
```

For this task, only support `FadeParameter::FaderDb` in `is_override` and `next_send`. Keep using fader position-space threshold and `MIN_SEND_DELTA_POS`.

- [ ] **Step 5: Update current fader call sites**

Replace every `FadeTarget { group, channel, target_db }` with:

```rust
FadeTarget {
    group,
    channel,
    parameter: FadeParameter::FaderDb,
    target: target_db,
}
```

Update tests that assert `target_db` to assert `target` and `parameter == FadeParameter::FaderDb`.

- [ ] **Step 6: Run existing fade tests**

Run: `cargo test -p advanced-show-control --test fade_engine -- --nocapture`

Expected: PASS with only fader behavior active.

- [ ] **Step 7: Commit**

```bash
git add src/fade/types.rs src/fade/tick.rs src/fade/state.rs src/fade/actor.rs src/scene_recall/policy.rs src/scene_recall/actor.rs src/runtime/commands.rs src/main.rs tests/fade_engine.rs
git commit -m "refactor: key fades by parameter"
```

---

### Task 5: Add Pan-Family Fade Sends And Overrides

**Files:**

- Modify: `src/fade/tick.rs`
- Modify: `src/fade/state.rs`
- Modify: `src/fade/actor.rs`
- Modify: `tests/fade_engine.rs`

- [ ] **Step 1: Add failing pan-family send test**

In `tests/fade_engine.rs`, add a test that starts a fade with targets:

```rust
vec![
    FadeTarget { group: 0, channel: 4, parameter: FadeParameter::Pan, target: -12.0 },
    FadeTarget { group: 0, channel: 4, parameter: FadeParameter::Balance, target: 3.0 },
    FadeTarget { group: 0, channel: 4, parameter: FadeParameter::Width, target: 1.2 },
]
```

Assert the fake LV1 command receiver observes `set_pan`, `set_balance`, and `set_width` writes with the final exact values when `duration_ms == 0`.

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p advanced-show-control --test fade_engine pan_family_duration_zero_sends_exact_values -- --nocapture`

Expected: FAIL because `FadeEngine` still sends only fader commands.

- [ ] **Step 3: Implement parameter-specific sends**

In `src/fade/actor.rs` or the helper that performs target sends, route by parameter:

```rust
match target.parameter {
    FadeParameter::FaderDb => command_bus.set_gain(target.group, target.channel, value).await,
    FadeParameter::Pan => command_bus.set_pan(target.group, target.channel, value).await,
    FadeParameter::Balance => command_bus.set_balance(target.group, target.channel, value).await,
    FadeParameter::Width => command_bus.set_width(target.group, target.channel, value).await,
}
```

- [ ] **Step 4: Implement linear pan-family interpolation and deltas**

In `src/fade/tick.rs`, add constants:

```rust
pub const MIN_SEND_DELTA_LINEAR: f64 = 0.002;
pub const OVERRIDE_THRESHOLD_LINEAR: f64 = 0.02;
```

Use fader-law position space only for `FadeParameter::FaderDb`. For pan, balance, and width use direct absolute value differences.

- [ ] **Step 5: Add failing pan-family override grouping test**

In `tests/fade_engine.rs`, add a test that starts concurrent fader, pan, balance, and width fades for the same channel, then injects a manual `/Notify/Balance` event. Assert pan, balance, and width stop while fader continues.

- [ ] **Step 6: Implement pan-family override grouping**

When a reported update is an override for a pan-family target, remove all active targets where:

```rust
active.group == group
    && active.channel == channel
    && active.parameter.is_pan_family()
```

Do not remove `FadeParameter::FaderDb` for the same channel.

- [ ] **Step 7: Run fade tests**

Run: `cargo test -p advanced-show-control --test fade_engine -- --nocapture`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/fade/tick.rs src/fade/state.rs src/fade/actor.rs tests/fade_engine.rs
git commit -m "feat: fade pan family parameters"
```

---

### Task 6: Store Pan-Family Scene Targets

**Files:**

- Modify: `src/show/capture.rs`
- Modify: `src/show/state.rs`
- Modify: `src/show/types.rs`

- [ ] **Step 1: Add failing store capture test**

In `src/show/state.rs` tests, add:

```rust
#[test]
fn store_scene_config_captures_pan_family_values() {
    let mut state = ShowState::default();
    state.scene_configs.push(SceneConfig {
        scene_id: "1::Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        duration_ms: 1000,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    });

    state.store_scene_config("1::Intro", &[ChannelInfo {
        group: 0,
        channel: 4,
        name: "Stereo".to_string(),
        gain_db: -6.0,
        muted: false,
        pan: Some(-12.0),
        balance: Some(3.0),
        width: Some(1.2),
        pan_mode: Some(PanMode::Stereo),
    }]).unwrap();

    let stored = &state.scene_configs[0].channel_configs[0];
    assert_eq!(stored.fader_db, Some(-6.0));
    assert_eq!(stored.pan, Some(-12.0));
    assert_eq!(stored.balance, Some(3.0));
    assert_eq!(stored.width, Some(1.2));
    assert_eq!(stored.pan_mode, Some(PanMode::Stereo));
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p advanced-show-control show::state::tests::store_scene_config_captures_pan_family_values -- --nocapture`

Expected: FAIL until capture copies the new fields.

- [ ] **Step 3: Implement capture**

In `src/show/capture.rs`, update `ChannelConfig` construction:

```rust
ChannelConfig {
    group: channel.group,
    channel: channel.channel,
    fader_db: Some(channel.gain_db),
    pan: channel.pan,
    balance: channel.balance,
    width: channel.width,
    pan_mode: channel.pan_mode,
}
```

- [ ] **Step 4: Add PAN scope setter test and implementation**

Mirror the existing fader scope setter with:

```rust
pub fn set_scene_scope_pan_enabled(&mut self, scene_id: &str, enabled: bool) -> Result<bool, String> {
    let scene = self
        .get_scene_config_mut(scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;
    if scene.scope_toggles.pan == enabled {
        Ok(false)
    } else {
        scene.scope_toggles.pan = enabled;
        Ok(true)
    }
}
```

- [ ] **Step 5: Run show tests**

Run: `cargo test -p advanced-show-control show -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/show/capture.rs src/show/state.rs src/show/types.rs
git commit -m "feat: store pan family scene targets"
```

---

### Task 7: Build Pan-Family Recall Targets

**Files:**

- Modify: `src/scene_recall/policy.rs`

- [ ] **Step 1: Add failing recall policy tests**

Add tests in `src/scene_recall/policy.rs`:

```rust
#[test]
fn pan_scope_builds_mono_pan_target_without_fader() {
    let mut scene_config = config(1000, Some(-6.0));
    scene_config.scope_toggles = SceneScopeToggles { faders: false, pan: true };
    scene_config.channel_configs[0].pan = Some(-12.0);
    scene_config.channel_configs[0].pan_mode = Some(PanMode::Mono);

    let decision = decide_scene_recall(input_with(scene_config, vec![mono_channel()]));

    let RecallPolicyDecision::Start(fade) = decision else { panic!("expected start"); };
    assert_eq!(fade.targets, vec![FadeTarget { group: 0, channel: 2, parameter: FadeParameter::Pan, target: -12.0 }]);
}

#[test]
fn pan_scope_builds_stereo_pan_balance_width_targets() {
    let mut scene_config = config(1000, Some(-6.0));
    scene_config.scope_toggles = SceneScopeToggles { faders: false, pan: true };
    scene_config.channel_configs[0].pan = Some(-12.0);
    scene_config.channel_configs[0].balance = Some(3.0);
    scene_config.channel_configs[0].width = Some(1.2);
    scene_config.channel_configs[0].pan_mode = Some(PanMode::Stereo);

    let decision = decide_scene_recall(input_with(scene_config, vec![stereo_channel()]));

    let RecallPolicyDecision::Start(fade) = decision else { panic!("expected start"); };
    assert_eq!(fade.targets, vec![
        FadeTarget { group: 0, channel: 2, parameter: FadeParameter::Pan, target: -12.0 },
        FadeTarget { group: 0, channel: 2, parameter: FadeParameter::Balance, target: 3.0 },
        FadeTarget { group: 0, channel: 2, parameter: FadeParameter::Width, target: 1.2 },
    ]);
}

#[test]
fn both_scopes_disabled_skips_without_blocking() {
    let mut scene_config = config(1000, Some(-6.0));
    scene_config.scope_toggles = SceneScopeToggles { faders: false, pan: false };

    let decision = decide_scene_recall(input_with(scene_config, vec![mono_channel()]));

    assert!(matches!(decision, RecallPolicyDecision::Skip { .. }));
}
```

Define `mono_channel`, `stereo_channel`, and `input_with` using existing test helper style in the same file.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control scene_recall::policy::tests -- --nocapture`

Expected: FAIL because policy still only handles fader scope.

- [ ] **Step 3: Implement scope branching**

In `decide_scene_recall`:

```rust
if !config.scope_toggles.faders && !config.scope_toggles.pan {
    return skipped("all scene scopes are disabled");
}
```

For each scoped channel:

```rust
if config.scope_toggles.faders {
    let Some(target) = stored.fader_db else { return blocked(...); };
    targets.push(FadeTarget { group: scoped.group, channel: scoped.channel, parameter: FadeParameter::FaderDb, target });
}

if config.scope_toggles.pan {
    let Some(pan_mode) = stored.pan_mode else { return blocked(...); };
    let Some(pan) = stored.pan else { return blocked(...); };
    targets.push(FadeTarget { group: scoped.group, channel: scoped.channel, parameter: FadeParameter::Pan, target: pan });
    if pan_mode == PanMode::Stereo {
        let Some(balance) = stored.balance else { return blocked(...); };
        let Some(width) = stored.width else { return blocked(...); };
        targets.push(FadeTarget { group: scoped.group, channel: scoped.channel, parameter: FadeParameter::Balance, target: balance });
        targets.push(FadeTarget { group: scoped.group, channel: scoped.channel, parameter: FadeParameter::Width, target: width });
    }
}
```

Keep live topology validation before building targets. Do not abort fades in blocked/skipped paths; the actor already handles that by only starting fade on `Start`.

- [ ] **Step 4: Run policy tests**

Run: `cargo test -p advanced-show-control scene_recall::policy::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/scene_recall/policy.rs
git commit -m "feat: build pan family recall targets"
```

---

### Task 8: Project Pan-Family Data Through Tauri Shell And Show File

**Files:**

- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/projection.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping_tests.rs`
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Add failing mapping tests**

In `src-tauri/src/app_state/show_file_mapping_tests.rs`, add assertions that a scene config with `scope_toggles.pan == true` and channel `pan`, `balance`, `width`, `pan_mode` round-trips through show file mapping.

Expected shape in serialized show file structs:

```rust
assert_eq!(file_scene.scope_toggles.pan, true);
assert_eq!(file_scene.channel_configs[0].pan, Some(-12.0));
assert_eq!(file_scene.channel_configs[0].balance, Some(3.0));
assert_eq!(file_scene.channel_configs[0].width, Some(1.2));
assert_eq!(file_scene.channel_configs[0].pan_mode, Some(PanMode::Stereo));
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p advanced-show-control-tauri show_file_mapping -- --nocapture`

Expected: FAIL because Tauri show file/view structs do not include the new fields.

- [ ] **Step 3: Add fields to Tauri view/show-file structs**

Mirror the core names using camelCase serde:

```rust
pub pan: Option<f64>,
pub balance: Option<f64>,
pub width: Option<f64>,
pub pan_mode: Option<PanMode>,
```

Add `pub pan: bool` to scope toggle structs with `#[serde(default)]` where persisted.

- [ ] **Step 4: Wire the command**

In `src-tauri/src/commands.rs`, add `set_scene_scope_pan_enabled` mirroring `set_scene_scope_faders_enabled` and calling the new show-state method.

- [ ] **Step 5: Run Tauri tests**

Run: `cargo test -p advanced-show-control-tauri show_file_mapping commands::tests app_state -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state/view.rs src-tauri/src/app_state/projection.rs src-tauri/src/app_state/show_file_mapping.rs src-tauri/src/app_state/show_file_mapping_tests.rs src-tauri/src/show_file.rs src-tauri/src/commands.rs
git commit -m "feat: project pan family scene data"
```

---

### Task 9: Add PAN Toggle And UI Display

**Files:**

- Modify: `ui/src/types.ts`
- Modify: `ui/src/commands.ts`
- Modify: `ui/src/components/SceneTab.tsx`
- Modify: `ui/src/format.ts` if formatting helpers are useful.

- [ ] **Step 1: Add command binding**

In `ui/src/commands.ts`, add:

```ts
export function setSceneScopePanEnabled(sceneId: string, enabled: boolean) {
  return invoke<AppViewState>("set_scene_scope_pan_enabled", { sceneId, enabled });
}
```

Use the same naming/import style as `setSceneScopeFadersEnabled`.

- [ ] **Step 2: Update `SceneTab` props**

Add a callback prop:

```ts
setSceneScopePanEnabled: (sceneId: string, enabled: boolean) => void;
```

Thread it from `ui/src/App.tsx` using the same `runSnapshotCommand` pattern as fader scope.

- [ ] **Step 3: Add PAN summary and toggle**

In `SceneTab.tsx`, update the row summary to include:

```tsx
PAN {scene.scopeToggles.pan ? "on" : "off"}
```

Add a button next to `FADERS`:

```tsx
<button
  type="button"
  className={selected.scopeToggles.pan ? activeClassName : inactiveClassName}
  onClick={() => props.setSceneScopePanEnabled(selected.sceneId, !selected.scopeToggles.pan)}
>
  PAN {selected.scopeToggles.pan ? "ON" : "OFF"}
</button>
```

Reuse the existing visual style for the fader toggle. Do not create a separate scoped channel grid.

- [ ] **Step 4: Update channel tooltip**

Where channel tiles currently show fader dB in the `title`, append available pan-family values:

```ts
const panParts = [
  config.pan === null ? null : `pan ${config.pan.toFixed(2)}`,
  config.balance === null ? null : `bal ${config.balance.toFixed(2)}`,
  config.width === null ? null : `width ${config.width.toFixed(2)}`,
].filter(Boolean);
```

Keep the tile layout unchanged unless space already exists for a secondary line.

- [ ] **Step 5: Run frontend checks**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/types.ts ui/src/commands.ts ui/src/components/SceneTab.tsx ui/src/App.tsx ui/src/format.ts
git commit -m "feat: add pan scene scope ui"
```

---

### Task 10: Cleanup Experimental Probe And Documentation

**Files:**

- Modify: `src/main.rs`
- Modify: `docs/architecture.md`
- Modify: `PHASES.md`
- Modify: `IDEAS.md` only for future pan-related ideas discovered during implementation.

- [ ] **Step 1: Remove throwaway `pan-probe` CLI unless explicitly kept**

In `src/main.rs`, remove the experimental `pan-probe` subcommand and helpers added during protocol discovery. Keep committed production CLI behavior intact.

- [ ] **Step 2: Run CLI compile check**

Run: `cargo test -p advanced-show-control --bin advanced-show-control -- --nocapture`

Expected: PASS or no tests run with successful compile.

- [ ] **Step 3: Update architecture docs**

In `docs/architecture.md`, update the fade and LV1 actor sections with these facts:

```markdown
- `FadeEngine` fades parameter-aware targets keyed by `(group, channel, FadeParameter)`.
- Fader targets use fader-law interpolation and override detection.
- Pan, balance, and width use direct linear interpolation and direct override thresholds.
- Pan-family manual override cancels pan, balance, and width for that channel without cancelling fader fades.
- `Lv1Actor` mirrors `/Notify/Track/Pan`, `/Notify/Balance`, and `/Notify/PanArcWidth` and sends the matching `/Set/Track/Pan*` commands.
```

- [ ] **Step 4: Update phase notes**

If `PHASES.md` tracks current planned work, mark pan fade support as implemented or in progress according to the actual branch state. Do not add unrelated roadmap items.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs docs/architecture.md PHASES.md IDEAS.md
git commit -m "docs: document pan fade support"
```

---

### Task 11: Full Verification

**Files:**

- No source edits unless verification finds failures.

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 2: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 4: Frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 5: Frontend build**

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Final diff review**

Run: `git status --short`

Expected: only intended files changed or clean after commits.

Run: `git diff --stat HEAD~10..HEAD`

Expected: changes match this plan: core pan-family data, LV1 parsing/writes, fade engine parameterization, recall policy, Tauri projection, UI toggle, docs.

---

## Self-Review Notes

- Spec coverage: data model, protocol, LV1 mirror, show storage, recall validation, fade engine, override grouping, UI, and verification are covered by Tasks 1-11.
- Isolation: the plan separates safe refactor work from behavior additions, so existing fader tests can pass before pan-family fades are enabled.
- Safety: blocked/skipped recalls still do not start fade commands; pan-family override grouping explicitly avoids cancelling faders.
- Known implementation detail: exact helper names in existing tests may differ; keep the assertions and use the local helper style rather than introducing duplicate test scaffolding.
