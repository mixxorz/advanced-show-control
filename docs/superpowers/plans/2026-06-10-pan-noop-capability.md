# PAN No-Op Capability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make PAN scope no-op for unavailable pan-family controls instead of blocking scene recall.

**Architecture:** Treat LV1 pan mode `i:0` as an explicit no-pan capability and build PAN fade targets only from stored values that exist. Keep fader recall validation strict. Update frontend grouping so official Link/DCAs channels are no longer shown as `Unknown`.

**Tech Stack:** Rust core crate, Tauri Rust shell crate, React/TypeScript frontend, Cargo tests, npm typecheck/build.

---

## File Structure

- Modify `src/lv1/types.rs`: add `PanMode::None` with serde camelCase support.
- Modify `src/lv1/parsers.rs`: parse `/Channels` pan mode field `i:0` as `Some(PanMode::None)` and keep unknown values as `None`.
- Modify `src/scene_recall/policy.rs`: make PAN target building no-op for missing pan-family values and explicit no-pan mode; return `Skip` instead of `Blocked` when no targets are produced.
- Modify `src/show/types.rs`: update serialization tests for the new `PanMode::None` variant.
- Modify `ui/src/format.ts`: label group `12` as `Link/DCAs` and make group ordering deterministic.
- No new files are needed.

---

### Task 1: Parse Explicit No-Pan Mode

**Files:**
- Modify: `src/lv1/types.rs`
- Modify: `src/lv1/parsers.rs`
- Modify: `src/show/types.rs`

- [ ] **Step 1: Write failing parser test for pan mode `i:0`**

Add this test to `src/lv1/parsers.rs` inside `mod tests` after `parses_integer_pan_degrees_in_channels_batch`:

```rust
#[test]
fn parses_no_pan_mode_in_channels_batch() {
    let args = make_channel_args(&[("Link 1", 12, 0, -9.1, 0.0, 0, 0.0)]);

    let channels = parse_channels_batch(&args).unwrap();

    assert_eq!(channels[0].pan_mode, Some(crate::lv1::types::PanMode::None));
    assert_eq!(channels[0].balance, None);
}
```

- [ ] **Step 2: Write failing serialization test for no-pan mode**

Add this test to `src/show/types.rs` inside `mod tests` after `scene_config_serializes_pan_family_fields_for_frontend_camel_case`:

```rust
#[test]
fn scene_config_serializes_no_pan_mode_for_frontend_camel_case() {
    let config = SceneConfig {
        scene_id: "0::Intro".to_string(),
        scene_index: 0,
        scene_name: "Intro".to_string(),
        duration_ms: 1000,
        channel_configs: vec![ChannelConfig {
            group: 12,
            channel: 0,
            fader_db: Some(-6.0),
            pan: None,
            balance: None,
            width: None,
            pan_mode: Some(crate::lv1::types::PanMode::None),
        }],
        scoped_channels: vec![ChannelRef { group: 12, channel: 0 }],
        scope_toggles: SceneScopeToggles { faders: true, pan: true },
    };

    let json = serde_json::to_value(config).unwrap();

    assert_eq!(json["channelConfigs"][0]["panMode"], "none");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::parsers::tests::parses_no_pan_mode_in_channels_batch
cargo nextest run -p advanced-show-control show::types::tests::scene_config_serializes_no_pan_mode_for_frontend_camel_case
```

Expected: both tests fail because `PanMode::None` does not exist.

- [ ] **Step 4: Add `PanMode::None`**

Update `src/lv1/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanMode {
    None,
    Mono,
    Stereo,
}
```

- [ ] **Step 5: Parse `i:0` as no-pan mode**

Update the pan mode match in `src/lv1/parsers.rs`:

```rust
let pan_mode = match args[base + 16] {
    OscArg::Int(0) => Some(crate::lv1::types::PanMode::None),
    OscArg::Int(1) => Some(crate::lv1::types::PanMode::Mono),
    OscArg::Int(2) => Some(crate::lv1::types::PanMode::Stereo),
    OscArg::Int(_) => None,
    _ => return Err("channel pan mode must be an int"),
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::parsers::tests::parses_no_pan_mode_in_channels_batch
cargo nextest run -p advanced-show-control show::types::tests::scene_config_serializes_no_pan_mode_for_frontend_camel_case
```

Expected: both tests pass.

- [ ] **Step 7: Run targeted parser/show tests**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::parsers::tests
cargo nextest run -p advanced-show-control show::types::tests
```

Expected: parser and show type tests pass.

- [ ] **Step 8: Commit**

Run:

```bash
git add src/lv1/types.rs src/lv1/parsers.rs src/show/types.rs
git commit -m "feat: parse no-pan mode"
```

---

### Task 2: Make PAN Recall Build Available Targets Only

**Files:**
- Modify: `src/scene_recall/policy.rs`

- [ ] **Step 1: Add failing test for no-pan scoped channel no-op**

Add this test to `src/scene_recall/policy.rs` inside `mod tests` near the other pan-only tests:

```rust
#[test]
fn pan_only_no_pan_mode_skips_without_blocking() {
    let mut scene_config = config(
        1000,
        None,
        None,
        None,
        None,
        Some(crate::lv1::types::PanMode::None),
    );
    scene_config.scope_toggles.faders = false;
    scene_config.scope_toggles.pan = true;

    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: SceneState { index: 1, name: "Intro".to_string() },
        lv1_snapshot: snapshot(
            Some(SceneState { index: 1, name: "Intro".to_string() }),
            vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Ch 2".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: Some(crate::lv1::types::PanMode::None),
            }],
        ),
        lockout: false,
        scene_config: Some(scene_config),
    });

    assert!(matches!(decision, RecallPolicyDecision::Skip { reason } if reason == "no applicable targets"));
}
```

- [ ] **Step 2: Add failing test for stereo missing width using pan and balance only**

Replace existing test `pan_only_stereo_missing_width_blocks` with:

```rust
#[test]
fn pan_only_stereo_missing_width_uses_available_targets() {
    let mut scene_config = config(
        1000,
        None,
        Some(0.25),
        Some(-0.5),
        None,
        Some(crate::lv1::types::PanMode::Stereo),
    );
    scene_config.scope_toggles.faders = false;
    scene_config.scope_toggles.pan = true;

    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: SceneState { index: 1, name: "Intro".to_string() },
        lv1_snapshot: snapshot(
            Some(SceneState { index: 1, name: "Intro".to_string() }),
            vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Ch 2".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: Some(0.0),
                balance: Some(0.0),
                width: None,
                pan_mode: Some(crate::lv1::types::PanMode::Stereo),
            }],
        ),
        lockout: false,
        scene_config: Some(scene_config),
    });

    let RecallPolicyDecision::Start(config) = decision else {
        panic!("expected start decision, got {decision:?}");
    };
    assert_eq!(config.targets.len(), 2);
    assert!(config.targets.iter().any(|target| {
        target.group == 0
            && target.channel == 2
            && target.parameter == FadeParameter::Pan
            && target.target == 0.25
    }));
    assert!(config.targets.iter().any(|target| {
        target.group == 0
            && target.channel == 2
            && target.parameter == FadeParameter::Balance
            && target.target == -0.5
    }));
    assert!(!config.targets.iter().any(|target| target.parameter == FadeParameter::Width));
}
```

- [ ] **Step 3: Add failing test for fader strictness staying unchanged**

Add this test near existing fader validation tests:

```rust
#[test]
fn fader_scope_still_blocks_missing_fader_value() {
    let mut scene_config = config(
        1000,
        None,
        None,
        None,
        None,
        Some(crate::lv1::types::PanMode::None),
    );
    scene_config.scope_toggles.faders = true;
    scene_config.scope_toggles.pan = false;

    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: SceneState { index: 1, name: "Intro".to_string() },
        lv1_snapshot: snapshot(
            Some(SceneState { index: 1, name: "Intro".to_string() }),
            vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Ch 2".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: Some(crate::lv1::types::PanMode::None),
            }],
        ),
        lockout: false,
        scene_config: Some(scene_config),
    });

    assert!(matches!(decision, RecallPolicyDecision::Blocked { reason } if reason == "scoped channel group=0 channel=2 has no stored fader value"));
}
```

- [ ] **Step 4: Run tests to verify PAN tests fail and fader test passes or compiles**

Run:

```bash
cargo nextest run -p advanced-show-control scene_recall::policy::tests
```

Expected: the new PAN behavior tests fail under current strict validation. The fader strictness test should pass once `PanMode::None` exists.

- [ ] **Step 5: Change PAN target building to no-op missing values**

Replace the `if pan_enabled { ... }` block in `decide_scene_recall` with:

```rust
if pan_enabled {
    match stored.pan_mode.as_ref() {
        Some(crate::lv1::types::PanMode::None) => {}
        Some(crate::lv1::types::PanMode::Mono) => {
            if let Some(pan) = stored.pan {
                targets.push(FadeTarget {
                    group: scoped.group,
                    channel: scoped.channel,
                    parameter: FadeParameter::Pan,
                    target: pan,
                });
            }
        }
        Some(crate::lv1::types::PanMode::Stereo) => {
            if let Some(pan) = stored.pan {
                targets.push(FadeTarget {
                    group: scoped.group,
                    channel: scoped.channel,
                    parameter: FadeParameter::Pan,
                    target: pan,
                });
            }
            if let Some(balance) = stored.balance {
                targets.push(FadeTarget {
                    group: scoped.group,
                    channel: scoped.channel,
                    parameter: FadeParameter::Balance,
                    target: balance,
                });
            }
            if let Some(width) = stored.width {
                targets.push(FadeTarget {
                    group: scoped.group,
                    channel: scoped.channel,
                    parameter: FadeParameter::Width,
                    target: width,
                });
            }
        }
        None => {}
    }
}
```

Replace the final empty-target block with:

```rust
if targets.is_empty() {
    return skipped("no applicable targets");
}
```

- [ ] **Step 6: Run targeted recall policy tests**

Run:

```bash
cargo nextest run -p advanced-show-control scene_recall::policy::tests
```

Expected: all scene recall policy tests pass after updating any tests that still assert PAN missing-value blocks.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/scene_recall/policy.rs
git commit -m "fix: no-op unavailable pan targets"
```

---

### Task 3: Label Link/DCAs In The Frontend

**Files:**
- Modify: `ui/src/format.ts`

- [ ] **Step 1: Add or update format helper behavior manually**

No frontend unit test suite currently covers `ui/src/format.ts`. Make the smallest UI helper change directly and verify with typecheck/build in Task 4.

Update `channelDisplayGroup` in `ui/src/format.ts`:

```ts
export function channelDisplayGroup(group: number) {
  if (group === 0) return "Inputs";
  if (group === 1) return "Groups";
  if (group === 2) return "Aux";
  if (group === 6) return "Matrix";
  if (group === 12) return "Link/DCAs";
  if ([3, 4, 5, 7, 8].includes(group)) return "Masters";
  return "Unknown";
}
```

Update `channelDisplayGroupOrder` in `ui/src/format.ts`:

```ts
export function channelDisplayGroupOrder(groupName: string) {
  return ["Inputs", "Groups", "Aux", "Matrix", "Masters", "Link/DCAs", "Unknown"].indexOf(groupName);
}
```

- [ ] **Step 2: Run frontend typecheck**

Run:

```bash
npm run typecheck
```

Expected: TypeScript check passes.

- [ ] **Step 3: Commit**

Run:

```bash
git add ui/src/format.ts
git commit -m "fix: label link dca channels"
```

---

### Task 4: Final Verification

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run Rust formatting check**

Run:

```bash
cargo fmt --all -- --check
```

Expected: formatting check passes.

- [ ] **Step 2: Run Rust linting**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clippy passes with no warnings.

- [ ] **Step 3: Run Rust tests**

Run:

```bash
cargo nextest run --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 4: Run frontend typecheck**

Run:

```bash
npm run typecheck
```

Expected: typecheck passes.

- [ ] **Step 5: Run frontend build**

Run:

```bash
npm run build
```

Expected: frontend production build passes.

- [ ] **Step 6: Inspect final status**

Run:

```bash
git status --short
```

Expected: no uncommitted changes except intentional files if the user requested no commits during execution.

---

## Self-Review

- Spec coverage: explicit no-pan mode is covered by Task 1; PAN no-op recall and available-target behavior are covered by Task 2; Link/DCAs UI label is covered by Task 3; full verification is covered by Task 4.
- Placeholder scan: no TBD/TODO placeholders remain.
- Type consistency: the plan consistently uses `PanMode::None`, `FadeParameter::{Pan,Balance,Width}`, `scope_toggles.pan`, and the existing `RecallPolicyDecision::{Start,Skip,Blocked}` names.
