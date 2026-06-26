# Explicit LV1 Latency Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove misleading discovery-derived latency from app state and add an explicit one-shot TCP connect latency probe from the connection modal.

**Architecture:** Discovery state remains show-owned projection data and contains only LV1 identity/status. TCP probe behavior lives under `src-tauri/src/lv1/probe.rs`, and the `ui/` Tauri command is a thin adapter that delegates to `lv1`. The React modal owns probe result UI state locally by discovered-system key.

**Tech Stack:** Rust/Tauri with Tokio for TCP probing, React/TypeScript with Vitest and Testing Library for modal behavior.

## Global Constraints

- Remove `latencyMs` / `latency_ms` from discovered LV1 system state.
- The probe command name is `probe_lv1_tcp_connect_latency`.
- The command accepts `Lv1SystemIdentity` and optional `timeoutMs`.
- Default probe timeout is 500 ms.
- Clamp probe timeout to 100-2000 ms.
- Return `{ tcpConnectMs }` on successful TCP connect.
- Probe results are local modal UI state, not projected backend state.
- Do not send LV1 protocol messages from the probe.
- Do not reuse or interfere with the persistent `Lv1Actor` connection.
- Rust behavior tests must be pure unit tests or side-effect tests using public async helpers such as a local `TcpListener`; do not inspect actor internals.

---

## File Structure

- Modify `src-tauri/src/lv1/probe.rs`: add `TcpConnectProbeResult`, timeout clamping, and async TCP connect probe helper.
- Modify `src-tauri/src/lv1/mod.rs`: publicly export probe result/helper if needed by `ui` command adapter.
- Modify `src-tauri/src/connection_state.rs`: remove `latency_ms` from `DiscoveredLv1System` and update mapping tests.
- Modify `src-tauri/src/lv1/discovery.rs`: remove `DiscoveryEntry.latency_ms` and elapsed discovery timing.
- Modify `src-tauri/src/ui/commands/lifecycle.rs`: add thin `probe_lv1_tcp_connect_latency` command adapter.
- Modify `src-tauri/src/ui/commands.rs` and `src-tauri/src/ui/mod.rs`: export/register new command.
- Modify `ui/src/types.ts`: remove `latencyMs` from `DiscoveredLv1System`, add `TcpConnectLatencyResult`.
- Modify `ui/src/commands.ts`: add `probeLv1TcpConnectLatency` service function.
- Modify `ui/src/AppRuntime.tsx`: add service and app-command wiring for the probe.
- Modify `ui/src/appContext.tsx` and `ui/src/storybook/mockAppCommands.ts`: add probe command type/default.
- Modify `ui/src/components/ConnectionModal.tsx`: remove projected latency display and add row-local `Test` action/result display.
- Modify `ui/src/components/ConnectionModal.test.tsx` and `ui/src/storybook/mockAppState.ts`: update fixtures and add probe UI coverage.

---

### Task 1: Remove Discovery Latency From Shared State

**Files:**
- Modify: `src-tauri/src/connection_state.rs`
- Modify: `src-tauri/src/lv1/discovery.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/storybook/mockAppState.ts`
- Modify: `ui/src/components/ConnectionModal.test.tsx`

**Interfaces:**
- Consumes: Existing `DiscoveryEntry`, `DiscoveredLv1System`, and `ConnectionModal` row rendering.
- Produces: `DiscoveredLv1System { identity, status }` in Rust and TypeScript with no latency field.

- [ ] **Step 1: Write the failing Rust state test**

In `src-tauri/src/connection_state.rs`, replace the previous latency preservation test with this test:

```rust
#[test]
fn system_from_discovery_maps_identity_and_status() {
    let entry = DiscoveryEntry {
        service: "_waveslv113._tcp".to_string(),
        uuid: Some("lv1-demo".to_string()),
        host: Some("FOH LV1".to_string()),
        port: Some(22000),
        addresses: vec!["192.168.1.42".to_string()],
        ipv6: Vec::new(),
        source: "192.168.1.42".to_string(),
    };

    let system = system_from_discovery(&entry).expect("entry should map to modal system");

    assert_eq!(system.identity.address, "192.168.1.42");
    assert_eq!(system.identity.port, 22000);
    assert_eq!(system.status, DiscoveredLv1Status::Available);
}
```

- [ ] **Step 2: Run Rust test to verify it fails**

Run: `cargo nextest run -p advanced-show-control connection_state::tests::system_from_discovery_maps_identity_and_status`

Expected: FAIL to compile while `DiscoveryEntry` still requires `latency_ms` or while the old test/function shape remains.

- [ ] **Step 3: Remove Rust latency fields and discovery timing**

In `src-tauri/src/connection_state.rs`, make `DiscoveredLv1System` look like this:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLv1System {
    pub identity: Lv1SystemIdentity,
    pub status: DiscoveredLv1Status,
}
```

Keep `system_from_discovery` but remove `latency_ms`:

```rust
pub fn system_from_discovery(entry: &DiscoveryEntry) -> Option<DiscoveredLv1System> {
    Some(DiscoveredLv1System {
        identity: identity_from_discovery(entry)?,
        status: DiscoveredLv1Status::Available,
    })
}
```

In `src-tauri/src/lv1/discovery.rs`, remove `pub latency_ms: Option<u64>` from `DiscoveryEntry`, remove `let started_at = Instant::now();`, set `deadline` from `Instant::now() + options.timeout`, remove `mut` from the parsed `entry`, remove `entry.latency_ms = ...`, and remove `latency_ms: None` from all `DiscoveryEntry` constructors.

- [ ] **Step 4: Remove TypeScript latency field and fixtures**

In `ui/src/types.ts`, change the type to:

```ts
export type DiscoveredLv1System = {
  identity: Lv1SystemIdentity;
  status: DiscoveredLv1Status;
};
```

In `ui/src/storybook/mockAppState.ts`, remove every `latencyMs: ...` property from discovered-system fixtures.

In `ui/src/components/ConnectionModal.test.tsx`, remove expectations for `"3 ms"` and `"-- ms"` from `renders discovered system details`.

- [ ] **Step 5: Run targeted checks**

Run: `cargo nextest run -p advanced-show-control discovery connection_state`

Expected: PASS, all discovery/connection-state tests pass.

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: PASS after modal test expectations no longer reference discovery latency.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/connection_state.rs src-tauri/src/lv1/discovery.rs src-tauri/src/show/actor.rs ui/src/types.ts ui/src/storybook/mockAppState.ts ui/src/components/ConnectionModal.test.tsx
git commit -m "fix: remove discovery latency from state"
```

---

### Task 2: Add LV1 TCP Connect Probe Helper

**Files:**
- Modify: `src-tauri/src/lv1/probe.rs`
- Modify: `src-tauri/src/lv1/mod.rs`

**Interfaces:**
- Consumes: Host address string, port, and optional timeout from the future Tauri command.
- Produces: `pub struct TcpConnectProbeResult { pub tcp_connect_ms: u64 }`, `pub fn clamp_tcp_probe_timeout(timeout_ms: Option<u64>) -> Duration`, and `pub async fn probe_tcp_connect_latency(address: &str, port: u16, timeout_ms: Option<u64>) -> Result<TcpConnectProbeResult, String>`.

- [ ] **Step 1: Write failing timeout clamp unit test**

Append to `src-tauri/src/lv1/probe.rs` tests:

```rust
#[test]
fn clamps_tcp_probe_timeout() {
    assert_eq!(clamp_tcp_probe_timeout(None), std::time::Duration::from_millis(500));
    assert_eq!(clamp_tcp_probe_timeout(Some(25)), std::time::Duration::from_millis(100));
    assert_eq!(clamp_tcp_probe_timeout(Some(750)), std::time::Duration::from_millis(750));
    assert_eq!(clamp_tcp_probe_timeout(Some(3000)), std::time::Duration::from_millis(2000));
}
```

- [ ] **Step 2: Run clamp test to verify it fails**

Run: `cargo nextest run -p advanced-show-control lv1::probe::tests::clamps_tcp_probe_timeout`

Expected: FAIL to compile because `clamp_tcp_probe_timeout` does not exist.

- [ ] **Step 3: Implement minimal timeout clamp and result type**

Add near the top of `src-tauri/src/lv1/probe.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpConnectProbeResult {
    pub tcp_connect_ms: u64,
}

pub fn clamp_tcp_probe_timeout(timeout_ms: Option<u64>) -> std::time::Duration {
    std::time::Duration::from_millis(timeout_ms.unwrap_or(500).clamp(100, 2000))
}
```

- [ ] **Step 4: Verify clamp test passes**

Run: `cargo nextest run -p advanced-show-control lv1::probe::tests::clamps_tcp_probe_timeout`

Expected: PASS.

- [ ] **Step 5: Write failing async TCP connect test**

Append to `src-tauri/src/lv1/probe.rs` tests:

```rust
#[tokio::test]
async fn probes_successful_tcp_connect_latency() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept_task = tokio::spawn(async move {
        let _ = listener.accept().await.unwrap();
    });

    let result = probe_tcp_connect_latency("127.0.0.1", port, Some(500))
        .await
        .unwrap();

    assert!(result.tcp_connect_ms <= 500);
    accept_task.await.unwrap();
}
```

- [ ] **Step 6: Run async probe test to verify it fails**

Run: `cargo nextest run -p advanced-show-control lv1::probe::tests::probes_successful_tcp_connect_latency`

Expected: FAIL to compile because `probe_tcp_connect_latency` does not exist.

- [ ] **Step 7: Implement TCP connect probe helper**

Add to `src-tauri/src/lv1/probe.rs`:

```rust
pub async fn probe_tcp_connect_latency(
    address: &str,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<TcpConnectProbeResult, String> {
    let timeout = clamp_tcp_probe_timeout(timeout_ms);
    let target = format!("{address}:{port}");
    let started_at = std::time::Instant::now();
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&target))
        .await
        .map_err(|_| "TCP probe timed out".to_string())?
        .map_err(|err| format!("TCP probe failed: {err}"))?;
    drop(stream);

    Ok(TcpConnectProbeResult {
        tcp_connect_ms: started_at.elapsed().as_millis() as u64,
    })
}
```

In `src-tauri/src/lv1/mod.rs`, export it:

```rust
pub use probe::{TcpConnectProbeResult, probe_tcp_connect_latency};
```

- [ ] **Step 8: Verify probe tests pass**

Run: `cargo nextest run -p advanced-show-control lv1::probe`

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lv1/probe.rs src-tauri/src/lv1/mod.rs
git commit -m "feat: add LV1 TCP latency probe"
```

---

### Task 3: Expose Thin Tauri Command and Frontend Service

**Files:**
- Modify: `src-tauri/src/ui/commands/lifecycle.rs`
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/commands.ts`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/appContext.tsx`
- Modify: `ui/src/storybook/mockAppCommands.ts`

**Interfaces:**
- Consumes: `crate::lv1::probe_tcp_connect_latency(address, port, timeout_ms)` from Task 2.
- Produces: Tauri command `probe_lv1_tcp_connect_latency` and frontend command `probeLv1TcpConnectLatency(identity): Promise<TcpConnectLatencyResult>`.

- [ ] **Step 1: Add Tauri command adapter**

In `src-tauri/src/ui/commands/lifecycle.rs`, add:

```rust
#[tauri::command]
pub async fn probe_lv1_tcp_connect_latency(
    identity: Lv1SystemIdentity,
    timeout_ms: Option<u64>,
) -> Result<crate::lv1::TcpConnectProbeResult, String> {
    crate::lv1::probe_tcp_connect_latency(&identity.address, identity.port, timeout_ms).await
}
```

In `src-tauri/src/ui/commands.rs`, add it to the lifecycle export list:

```rust
pub use lifecycle::{
    attempt_reconnect_lv1, connect_lv1_system, disconnect_lv1, frontend_ready,
    probe_lv1_tcp_connect_latency, reconnect_timed_out, startup_auto_connect_lv1,
};
```

In `src-tauri/src/ui/mod.rs`, add `commands::lifecycle::probe_lv1_tcp_connect_latency` to the `tauri::generate_handler!` list next to the other lifecycle commands.

- [ ] **Step 2: Run Rust compile check**

Run: `cargo check --workspace`

Expected: PASS.

- [ ] **Step 3: Add frontend service types and command wrapper**

In `ui/src/types.ts`, add:

```ts
export type TcpConnectLatencyResult = {
  tcpConnectMs: number;
};
```

In `ui/src/commands.ts`, update imports and add:

```ts
import type {
  AppSettings,
  Lv1SystemIdentity,
  TcpConnectLatencyResult,
} from "./types";

export async function probeLv1TcpConnectLatency(
  identity: Lv1SystemIdentity,
  timeoutMs = 500,
) {
  return invoke<TcpConnectLatencyResult>("probe_lv1_tcp_connect_latency", {
    identity,
    timeoutMs,
  });
}
```

- [ ] **Step 4: Wire command through runtime context**

In `ui/src/AppRuntime.tsx`, import `TcpConnectLatencyResult` and add service type:

```ts
probeLv1TcpConnectLatency: (
  identity: Lv1SystemIdentity,
) => Promise<TcpConnectLatencyResult>;
```

Add to `commands`:

```ts
probeLv1TcpConnectLatency: (identity) =>
  services.probeLv1TcpConnectLatency(identity),
```

In `ui/src/appContext.tsx`, import `TcpConnectLatencyResult` and add:

```ts
probeLv1TcpConnectLatency: (
  identity: Lv1SystemIdentity,
) => Promise<TcpConnectLatencyResult>;
```

In `ui/src/storybook/mockAppCommands.ts`, add:

```ts
probeLv1TcpConnectLatency: async () => ({ tcpConnectMs: 3 }),
```

- [ ] **Step 5: Run typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS or failures only from `ConnectionModal` not yet using the new command; fix import/type mismatches before proceeding.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ui/commands/lifecycle.rs src-tauri/src/ui/commands.rs src-tauri/src/ui/mod.rs ui/src/types.ts ui/src/commands.ts ui/src/AppRuntime.tsx ui/src/appContext.tsx ui/src/storybook/mockAppCommands.ts
git commit -m "feat: expose LV1 latency probe command"
```

---

### Task 4: Add Connection Modal Probe UI

**Files:**
- Modify: `ui/src/components/ConnectionModal.tsx`
- Modify: `ui/src/components/ConnectionModal.test.tsx`
- Modify: `ui/src/components/ConnectionModal.stories.tsx` if stories need command overrides for deterministic examples.

**Interfaces:**
- Consumes: `commands.probeLv1TcpConnectLatency(identity)` from Task 3.
- Produces: Row-local UI states: `Test`, `Testing...`, `TCP <n> ms`, or error text.

- [ ] **Step 1: Write failing modal probe test**

In `ui/src/components/ConnectionModal.test.tsx`, add:

```tsx
it("shows a TCP probe result without selecting the system", async () => {
  const user = userEvent.setup();
  const selectSystem = vi.fn();
  const probeLv1TcpConnectLatency = vi.fn().mockResolvedValue({ tcpConnectMs: 5 });
  renderModal({
    selectSystem,
    commands: { probeLv1TcpConnectLatency },
  });

  await user.click(screen.getAllByRole("button", { name: "Test TCP latency" })[0]);

  expect(probeLv1TcpConnectLatency).toHaveBeenCalledWith({
    uuid: "lv1-demo",
    host: "FOH LV1",
    address: "192.168.1.42",
    port: 22000,
  });
  expect(await screen.findByText("TCP 5 ms")).toBeInTheDocument();
  expect(selectSystem).not.toHaveBeenCalled();
});
```

Update the `renderModal` helper signature to accept extra command overrides:

```tsx
commands?: Parameters<typeof renderWithAppProviders>[1]["commands"];
```

and merge them into the provider options.

- [ ] **Step 2: Run modal test to verify it fails**

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: FAIL because no `Test TCP latency` button exists yet.

- [ ] **Step 3: Implement local probe state in modal**

In `ui/src/components/ConnectionModal.tsx`, import `useState` and add local state:

```tsx
type ProbeResult =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "success"; tcpConnectMs: number }
  | { status: "error"; message: string };
```

Inside `ConnectionModal`, add:

```tsx
const [probeResults, setProbeResults] = useState<Record<string, ProbeResult>>({});

async function testSystem(system: DiscoveredLv1System) {
  const key = systemKey(system);
  setProbeResults((current) => ({ ...current, [key]: { status: "testing" } }));
  try {
    const result = await commands.probeLv1TcpConnectLatency(system.identity);
    setProbeResults((current) => ({
      ...current,
      [key]: { status: "success", tcpConnectMs: result.tcpConnectMs },
    }));
  } catch (error) {
    setProbeResults((current) => ({
      ...current,
      [key]: { status: "error", message: String(error) },
    }));
  }
}
```

Pass `probeResult={probeResults[systemKey(system)] ?? { status: "idle" }}` and `onTestSystem={testSystem}` to `SystemRow`.

- [ ] **Step 4: Add row Test button without changing connect click behavior**

Update `SystemRow` props:

```tsx
probeResult: ProbeResult;
onTestSystem: (system: DiscoveredLv1System) => void | Promise<void>;
```

Replace the latency/status trailing area with status plus test action:

```tsx
<div className="flex items-center gap-3 font-mono text-sm md:justify-self-end">
  <span className={isUnavailable ? "text-status-danger" : isConnected ? "text-status-current" : "text-status-cued"}>
    {isUnavailable ? "Unavailable" : isConnected ? "Connected" : "Available"}
  </span>
  <span className="h-4 border-l border-console-line" />
  <span className="text-console-secondary">{probeLabel(props.probeResult)}</span>
  <ConsoleButton
    onClick={(event) => {
      event.stopPropagation();
      void props.onTestSystem(system);
    }}
    size="small"
    variant="secondary"
  >
    Test
  </ConsoleButton>
</div>
```

Add helper:

```tsx
function probeLabel(result: ProbeResult) {
  switch (result.status) {
    case "testing":
      return "Testing...";
    case "success":
      return `TCP ${result.tcpConnectMs} ms`;
    case "error":
      return result.message;
    case "idle":
      return "Not tested";
  }
}
```

Use `aria-label="Test TCP latency"` on the `ConsoleButton` if `ConsoleButton` supports passing native button props; otherwise wrap with a normal `button` styled consistently.

- [ ] **Step 5: Run modal tests**

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/ConnectionModal.tsx ui/src/components/ConnectionModal.test.tsx ui/src/components/ConnectionModal.stories.tsx
git commit -m "feat: add connection latency test action"
```

---

### Task 5: Final Verification

**Files:**
- No code edits unless verification exposes a defect.

**Interfaces:**
- Consumes: All prior tasks.
- Produces: Verified branch state ready for review.

- [ ] **Step 1: Run Rust formatting and lint**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 2: Run Rust targeted tests**

Run: `cargo nextest run -p advanced-show-control discovery connection_state lv1::probe`

Expected: PASS.

- [ ] **Step 3: Run frontend checks**

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: PASS.

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 4: Inspect diff and status**

Run: `git status --short`

Expected: no unstaged changes except intentional verification fixes.

Run: `git diff --stat HEAD~4..HEAD`

Expected: only latency/probe-related Rust, frontend, and docs changes.

- [ ] **Step 5: Commit verification fixes if needed**

If any verification fix was required:

```bash
git add <fixed-files>
git commit -m "fix: complete LV1 latency probe verification"
```

If no fixes were required, do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: discovery latency removal is Task 1, `lv1/` probe helper is Task 2, Tauri command/service wiring is Task 3, local modal UI state is Task 4, verification is Task 5.
- No continuous polling is planned; the probe runs only when the row `Test` action is clicked.
- No LV1 protocol messages are sent by the probe; it only opens and drops a TCP connection.
- The plan keeps the Tauri command thin and places networking behavior under `lv1/probe.rs`.
