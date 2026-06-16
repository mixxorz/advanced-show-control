# Pan Family Smoke Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a live LV1 CLI smoke test that drives Channel 3 pan, balance, and width through the fade engine and stops on manual override detection.

**Architecture:** Add one `pan-family-smoke-test` subcommand to the existing `lv1-probe` CLI in `src/main.rs`. The command uses `Lv1Actor`, `AppCommandBus`, and `FadeEngine` exactly like `fade-test`, then writes a small JSONL smoke log for command steps and fade events.

**Tech Stack:** Rust, Tokio, Clap, existing `advanced_show_control` core crate.

---

### Task 1: CLI Parser Coverage

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing parser test**

Add a test named `parses_pan_family_smoke_test_command` in the existing `#[cfg(test)] mod tests`.

```rust
#[test]
fn parses_pan_family_smoke_test_command() {
    let cli = Cli::try_parse_from([
        "lv1-probe",
        "pan-family-smoke-test",
        "--host",
        "192.168.1.10",
        "--port",
        "50001",
        "--timeout-ms",
        "3000",
        "--log-dir",
        "logs-smoke",
        "--group",
        "0",
        "--channel",
        "2",
        "--duration-ms",
        "750",
    ])
    .unwrap();

    match cli.command {
        Command::PanFamilySmokeTest {
            host,
            port,
            timeout_ms,
            log_dir,
            group,
            channel,
            duration_ms,
        } => {
            assert_eq!(host.as_deref(), Some("192.168.1.10"));
            assert_eq!(port, Some(50001));
            assert_eq!(timeout_ms, 3000);
            assert_eq!(log_dir, std::path::PathBuf::from("logs-smoke"));
            assert_eq!(group, 0);
            assert_eq!(channel, 2);
            assert_eq!(duration_ms, 750);
        }
        other => panic!("expected PanFamilySmokeTest, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run the targeted test and verify RED**

Run: `cargo nextest run -p advanced-show-control --bin advanced-show-control parses_pan_family_smoke_test_command`

Expected: FAIL because `Command::PanFamilySmokeTest` does not exist yet.

- [ ] **Step 3: Add the Clap variant and dispatch arm**

Add `PanFamilySmokeTest` to `Command` with defaults `group=0`, `channel=2`, `duration_ms=1000`, `timeout_ms=6000`, `log_dir=logs/pan-family-smoke-test`, then dispatch to `run_pan_family_smoke_test`.

- [ ] **Step 4: Run the targeted test and verify GREEN**

Run: `cargo nextest run -p advanced-show-control --bin advanced-show-control parses_pan_family_smoke_test_command`

Expected: PASS.

### Task 2: Smoke Runner

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement JSONL smoke logging and fade sequence**

Add `PanFamilySmokeOptions`, `run_pan_family_smoke_test`, `run_pan_family_smoke_step`, and `write_smoke_log_entry` in `src/main.rs` near the existing `run_fade_test` helper.

- [ ] **Step 2: Run formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS, or run `cargo fmt --all` and re-check if formatting is needed.

- [ ] **Step 3: Run targeted binary tests**

Run: `cargo nextest run -p advanced-show-control --bin advanced-show-control`

Expected: PASS.

- [ ] **Step 4: Run clippy for the touched package**

Run: `cargo clippy -p advanced-show-control --bin advanced-show-control -- -D warnings`

Expected: PASS.

### Task 3: Live Use Notes

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Ensure runtime output prints the smoke log path**

The command must print a line like `[smoke-log] logs/pan-family-smoke-test/lv1-pan-family-smoke-<timestamp>.jsonl` before fades start.

- [ ] **Step 2: Ensure override exits non-zero**

On `FadeEvent::ChannelOverride`, return an error that includes `manual override detected` and the smoke log path.

- [ ] **Step 3: Final verification**

Run: `cargo nextest run -p advanced-show-control --bin advanced-show-control && cargo clippy -p advanced-show-control --bin advanced-show-control -- -D warnings`

Expected: PASS.
