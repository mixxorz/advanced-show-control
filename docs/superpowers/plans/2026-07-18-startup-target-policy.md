# Startup LV1 Target Policy Relocation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #57 by moving unchanged startup LV1 identity matching policy beside its connection-state types.

**Architecture:** Move the direct pure function and its tests from lifecycle into `connection_state.rs`. Lifecycle continues to own settings/discovery requests, generation changes, abort decisions, logging, and connection dispatch.

**Tech Stack:** Rust 2024, Tokio actor tests, cargo-nextest

## Global Constraints

- Execute after issue #38 and before issue #53.
- Match only `Available` systems; exact UUID takes precedence over one exact trimmed hostname match.
- Reject blank or ambiguous hostnames and never fall back to address/port matching.
- Do not introduce a service, trait, actor, or dependency-injection layer.
- Before advancing to issue #53, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Relocate The Pure Matching Policy

**Files:**
- Modify: `src-tauri/src/connection_state.rs:1-76`
- Modify: `src-tauri/src/lifecycle/mod.rs:121-147,713-738,1105-1189`
- Test: `src-tauri/src/connection_state.rs`
- Test: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Consumes: `Lv1SystemIdentity`, `DiscoveredLv1System`, and `DiscoveredLv1Status`.
- Produces: `connection_state::startup_auto_connect_target(&Lv1SystemIdentity, &[DiscoveredLv1System]) -> Option<Lv1SystemIdentity>`.

- [ ] **Step 1: Add relocated tests before the function**

Move the existing lifecycle pure-policy test cases into `connection_state::tests`, preserving their assertions. Add this explicit blank-host/address-rejection case:

```rust
#[test]
fn startup_target_rejects_blank_hostname_without_using_address_or_port() {
    let remembered = Lv1SystemIdentity {
        uuid: None,
        host: Some("   ".to_string()),
        address: "10.0.0.20".to_string(),
        port: 50_000,
    };
    let systems = vec![DiscoveredLv1System {
        identity: Lv1SystemIdentity {
            uuid: None,
            host: Some("Different LV1".to_string()),
            address: "10.0.0.20".to_string(),
            port: 50_000,
        },
        status: DiscoveredLv1Status::Available,
    }];

    assert_eq!(startup_auto_connect_target(&remembered, &systems), None);
}
```

The moved tests must cover UUID precedence, unique trimmed hostname fallback, ambiguity rejection, unavailable candidates, and no address-only fallback.

- [ ] **Step 2: Run the relocated tests red**

Run:

```bash
cargo nextest run -p advanced-show-control connection_state
```

Expected: compilation fails because `connection_state::startup_auto_connect_target` is not defined.

- [ ] **Step 3: Add the direct pure function**

Add after `system_from_discovery` in `src-tauri/src/connection_state.rs`:

```rust
pub fn startup_auto_connect_target(
    remembered: &Lv1SystemIdentity,
    systems: &[DiscoveredLv1System],
) -> Option<Lv1SystemIdentity> {
    let available: Vec<_> = systems
        .iter()
        .filter(|system| system.status == DiscoveredLv1Status::Available)
        .collect();

    if let Some(uuid) = remembered.uuid.as_deref()
        && let Some(system) = available
            .iter()
            .find(|system| system.identity.uuid.as_deref() == Some(uuid))
    {
        return Some(system.identity.clone());
    }

    let host = remembered.host.as_deref()?.trim();
    if host.is_empty() {
        return None;
    }
    let mut matches = available
        .into_iter()
        .filter(|system| system.identity.host.as_deref().map(str::trim) == Some(host));
    let target = matches.next()?.identity.clone();
    matches.next().is_none().then_some(target)
}
```

- [ ] **Step 4: Route lifecycle through the relocated function**

Delete lifecycle's local `startup_auto_connect_target` definition. Replace its call in `startup_auto_connect_with_discovered` with:

```rust
let Some(identity) =
    crate::connection_state::startup_auto_connect_target(&remembered, systems)
else {
    tracing::debug!(
        event = "startup_auto_connect_no_match",
        "No safe discovered LV1 match for the remembered startup target"
    );
    return Ok(ConnectCommandResult { changed: false });
};
```

Delete only the pure tests moved to `connection_state.rs`. Retain lifecycle actor tests proving no remembered identity and ambiguous discovery do not advance generation or dispatch a connection.

- [ ] **Step 5: Run focused verification**

Run:

```bash
cargo nextest run -p advanced-show-control connection_state lifecycle
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: pure policy and lifecycle orchestration tests pass.

- [ ] **Step 6: Run the issue smoke checkpoint**

Run `make smoke`, then read `logs/debug-smoke-report.txt`. Expected: authoritative suite success. Fix and rerun only when needed to prove a failure correction.

- [ ] **Step 7: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/connection_state.rs src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/connection_state.rs src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: relocate startup LV1 target policy"
```
