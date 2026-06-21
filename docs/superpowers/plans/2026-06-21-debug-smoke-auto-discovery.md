# Debug Smoke Auto-Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-fill the debug smoke-test app's LV1 identity fields from the first discovered LV1 system without auto-connecting or running smoke tests.

**Architecture:** Use the existing production discovery path: debug frontend calls `refresh_lv1_discovery`, backend updates show-owned discovery state, projector emits `app-status-changed`, and debug frontend reads `discoveredLv1Systems[0]`. Keep all behavior frontend-only except for invoking the existing command wrapper.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Tauri command wrappers.

## Global Constraints

- This applies only to the development-only Tauri debug smoke-test frontend.
- Production app behavior must remain unchanged.
- Auto-discovery must not call `connect_lv1_system`.
- Auto-discovery must not run any smoke-test command.
- Auto-discovery must not emit `app-status-changed` directly.
- Auto-discovery must not bypass the existing `ShowCommand::RefreshLv1Discovery` path.
- Manual LV1 identity field edits must prevent later discovery updates from overwriting user-entered values.
- Do not add a discovered-system picker or dropdown.
- Do not add a debug-only backend discovery command.

---

## File Structure

- Modify `ui/src/debug/commands.ts`: export a debug wrapper for existing `refresh_lv1_discovery` as `refreshLv1Discovery(): Promise<void>`.
- Modify `ui/src/debug/commands.test.ts`: add a wrapper test proving `refreshLv1Discovery` invokes `refresh_lv1_discovery`.
- Modify `ui/src/debug/SmokeDebugApp.tsx`: import and pass `refreshLv1Discovery` into `SmokeTestPanel`.
- Modify `ui/src/debug/SmokeDebugApp.test.tsx`: test discovery startup behavior using the real panel and mocked command wrapper.
- Modify `ui/src/debug/SmokeTestPanel.tsx`: own auto-fill state, trigger discovery, consume `appState.discoveredLv1Systems`, render discovery status, and preserve manual edits.
- Modify `ui/src/debug/SmokeTestPanel.test.tsx`: add focused tests for auto-fill, manual-edit protection, and no implicit command execution.

### Task 1: Discovery Command Wrapper

**Files:**
- Modify: `ui/src/debug/commands.ts`
- Modify: `ui/src/debug/commands.test.ts`

**Interfaces:**
- Consumes: Tauri `invoke` from `@tauri-apps/api/core`.
- Produces: `refreshLv1Discovery(): Promise<void>` for `SmokeDebugApp`.

- [ ] **Step 1: Add failing wrapper test**

In `ui/src/debug/commands.test.ts`, add this test next to the existing command wrapper tests:

```ts
it("invokes LV1 discovery refresh", async () => {
  invokeMock.mockResolvedValueOnce(undefined);

  await refreshLv1Discovery();

  expect(invokeMock).toHaveBeenCalledWith("refresh_lv1_discovery");
});
```

Also add `refreshLv1Discovery` to the import from `./commands` in that test file.

- [ ] **Step 2: Run the failing test**

Run from `ui/`:

```bash
npm run test -- commands
```

Expected: FAIL because `refreshLv1Discovery` is not exported.

- [ ] **Step 3: Add command wrapper**

In `ui/src/debug/commands.ts`, add:

```ts
export async function refreshLv1Discovery(): Promise<void> {
  await invoke("refresh_lv1_discovery");
}
```

- [ ] **Step 4: Verify wrapper test passes**

Run from `ui/`:

```bash
npm run test -- commands
```

Expected: PASS.

### Task 2: Debug Panel Auto-Fill Behavior

**Files:**
- Modify: `ui/src/debug/SmokeTestPanel.tsx`
- Modify: `ui/src/debug/SmokeTestPanel.test.tsx`

**Interfaces:**
- Consumes: `AppViewState.discoveredLv1Systems` and `refreshLv1Discovery(): Promise<void>` passed as `onRefreshLv1Discovery`.
- Produces: `SmokeTestPanel` prop `onRefreshLv1Discovery: () => Promise<void>` and visible discovery status text.

- [ ] **Step 1: Add failing auto-fill test**

In `ui/src/debug/SmokeTestPanel.test.tsx`, add a discovered app state helper:

```ts
import { disconnectedAppViewState } from "../types";

const discoveredAppState = {
  ...disconnectedAppViewState,
  discoveredLv1Systems: [
    {
      identity: {
        uuid: "lv1-uuid",
        host: "lv1-host",
        address: "192.168.10.50",
        port: 12345,
      },
      latencyMs: 2,
      status: "available" as const,
    },
  ],
  stateVersion: disconnectedAppViewState.stateVersion + 1,
};
```

Add this test:

```ts
it("auto-fills LV1 identity fields from the first discovered system", async () => {
  renderWithAppProviders(
    <SmokeTestPanel
      appState={discoveredAppState}
      onRefreshLv1Discovery={vi.fn(async () => undefined)}
      onRunConnectionTest={vi.fn()}
      onRunSceneRecallTest={vi.fn()}
      onRunFadeStartsTest={vi.fn()}
      onRunFadeCompletesTest={vi.fn()}
      onRunDecreasingXFadeTest={vi.fn()}
      onRunLockoutBlocksRecallTest={vi.fn()}
    />,
  );

  expect(screen.getByLabelText("LV1 address")).toHaveValue("192.168.10.50");
  expect(screen.getByLabelText("LV1 port")).toHaveValue("12345");
  expect(
    screen.getByText(/Auto-filled from discovered LV1: lv1-host 192\.168\.10\.50:12345/),
  ).toBeInTheDocument();
});
```

- [ ] **Step 2: Add failing manual-edit protection test**

Add this test:

```ts
it("does not overwrite manually edited LV1 identity fields", async () => {
  const user = userEvent.setup();
  const onRefreshLv1Discovery = vi.fn(async () => undefined);
  const { rerender } = renderWithAppProviders(
    <SmokeTestPanel
      appState={disconnectedAppViewState}
      onRefreshLv1Discovery={onRefreshLv1Discovery}
      onRunConnectionTest={vi.fn()}
      onRunSceneRecallTest={vi.fn()}
      onRunFadeStartsTest={vi.fn()}
      onRunFadeCompletesTest={vi.fn()}
      onRunDecreasingXFadeTest={vi.fn()}
      onRunLockoutBlocksRecallTest={vi.fn()}
    />,
  );

  await user.type(screen.getByLabelText("LV1 address"), "10.0.0.9");

  rerender(
    <SmokeTestPanel
      appState={discoveredAppState}
      onRefreshLv1Discovery={onRefreshLv1Discovery}
      onRunConnectionTest={vi.fn()}
      onRunSceneRecallTest={vi.fn()}
      onRunFadeStartsTest={vi.fn()}
      onRunFadeCompletesTest={vi.fn()}
      onRunDecreasingXFadeTest={vi.fn()}
      onRunLockoutBlocksRecallTest={vi.fn()}
    />,
  );

  expect(screen.getByLabelText("LV1 address")).toHaveValue("10.0.0.9");
  expect(screen.getByText("Manual LV1 identity entry active.")).toBeInTheDocument();
});
```

- [ ] **Step 3: Add failing no implicit connect/test assertion**

Add this test:

```ts
it("does not connect or run smoke tests when auto-fill succeeds", () => {
  const onRunConnectionTest = vi.fn();
  const onRunSceneRecallTest = vi.fn();
  const onRunFadeStartsTest = vi.fn();
  const onRunFadeCompletesTest = vi.fn();
  const onRunDecreasingXFadeTest = vi.fn();
  const onRunLockoutBlocksRecallTest = vi.fn();

  renderWithAppProviders(
    <SmokeTestPanel
      appState={discoveredAppState}
      onRefreshLv1Discovery={vi.fn(async () => undefined)}
      onRunConnectionTest={onRunConnectionTest}
      onRunSceneRecallTest={onRunSceneRecallTest}
      onRunFadeStartsTest={onRunFadeStartsTest}
      onRunFadeCompletesTest={onRunFadeCompletesTest}
      onRunDecreasingXFadeTest={onRunDecreasingXFadeTest}
      onRunLockoutBlocksRecallTest={onRunLockoutBlocksRecallTest}
    />,
  );

  expect(onRunConnectionTest).not.toHaveBeenCalled();
  expect(onRunSceneRecallTest).not.toHaveBeenCalled();
  expect(onRunFadeStartsTest).not.toHaveBeenCalled();
  expect(onRunFadeCompletesTest).not.toHaveBeenCalled();
  expect(onRunDecreasingXFadeTest).not.toHaveBeenCalled();
  expect(onRunLockoutBlocksRecallTest).not.toHaveBeenCalled();
});
```

- [ ] **Step 4: Run tests to verify failure**

Run from `ui/`:

```bash
npm run test -- SmokeTestPanel
```

Expected: FAIL because `onRefreshLv1Discovery` prop and auto-fill behavior do not exist yet.

- [ ] **Step 5: Implement panel auto-fill**

In `ui/src/debug/SmokeTestPanel.tsx`, update imports:

```ts
import { useEffect, useState } from "react";
```

Add prop:

```ts
onRefreshLv1Discovery: () => Promise<void>;
```

Add state near the existing input state:

```ts
const [uuid, setUuid] = useState("");
const [host, setHost] = useState("");
const [identityManuallyEdited, setIdentityManuallyEdited] = useState(false);
const [identityAutoFilled, setIdentityAutoFilled] = useState(false);
const [discoveryError, setDiscoveryError] = useState<string | null>(null);
```

Add these helper functions inside `SmokeTestPanel`:

```ts
function markIdentityManualEdit(setValue: (value: string) => void, value: string) {
  setIdentityManuallyEdited(true);
  setValue(value);
}

function discoveryStatus() {
  if (identityManuallyEdited) return "Manual LV1 identity entry active.";
  if (discoveryError) return "LV1 discovery failed; enter identity manually.";
  if (identityAutoFilled) {
    const displayHost = host.trim().length > 0 ? host.trim() : "discovered host";
    return `Auto-filled from discovered LV1: ${displayHost} ${address.trim()}:${port.trim()}`;
  }
  return "Searching for LV1 systems...";
}
```

Add discovery startup effect:

```ts
useEffect(() => {
  if (identityManuallyEdited || identityAutoFilled) return;

  let cancelled = false;

  async function refresh() {
    try {
      await props.onRefreshLv1Discovery();
      if (!cancelled) setDiscoveryError(null);
    } catch {
      if (!cancelled) setDiscoveryError("failed");
    }
  }

  void refresh();
  const timer = window.setInterval(() => void refresh(), 3000);

  return () => {
    cancelled = true;
    window.clearInterval(timer);
  };
}, [identityAutoFilled, identityManuallyEdited, props.onRefreshLv1Discovery]);
```

Add auto-fill effect:

```ts
useEffect(() => {
  if (identityManuallyEdited || identityAutoFilled) return;
  const discovered = props.appState.discoveredLv1Systems[0]?.identity;
  if (!discovered) return;
  if (!discovered.address || !discovered.port) return;

  setUuid(discovered.uuid ?? "");
  setHost(discovered.host ?? "");
  setAddress(discovered.address);
  setPort(String(discovered.port));
  setIdentityAutoFilled(true);
}, [identityAutoFilled, identityManuallyEdited, props.appState.discoveredLv1Systems]);
```

Update identity construction:

```ts
const identity: Lv1SystemIdentity = {
  uuid: uuid.trim().length > 0 ? uuid.trim() : null,
  host: host.trim().length > 0 ? host.trim() : null,
  address: address.trim(),
  port: parsedPort,
};
```

Add fields before LV1 address:

```tsx
<label className="block text-sm">
  LV1 UUID
  <input
    className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
    value={uuid}
    onChange={(event) => markIdentityManualEdit(setUuid, event.target.value)}
  />
</label>
<label className="block text-sm">
  LV1 host
  <input
    className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
    value={host}
    onChange={(event) => markIdentityManualEdit(setHost, event.target.value)}
  />
</label>
```

Update LV1 address and port change handlers:

```tsx
onChange={(event) => markIdentityManualEdit(setAddress, event.target.value)}
```

```tsx
onChange={(event) => markIdentityManualEdit(setPort, event.target.value)}
```

Add status after the input grid:

```tsx
<p className="mt-3 text-sm text-console-muted">{discoveryStatus()}</p>
```

- [ ] **Step 6: Verify panel tests pass**

Run from `ui/`:

```bash
npm run test -- SmokeTestPanel
```

Expected: PASS.

### Task 3: Wire Debug App Discovery Services

**Files:**
- Modify: `ui/src/debug/SmokeDebugApp.tsx`
- Modify: `ui/src/debug/SmokeDebugApp.test.tsx`

**Interfaces:**
- Consumes: `refreshLv1Discovery(): Promise<void>` from `ui/src/debug/commands.ts`.
- Produces: `SmokeTestPanel` receives `onRefreshLv1Discovery={refreshLv1Discovery}`.

- [ ] **Step 1: Add failing app-level startup test**

In `ui/src/debug/SmokeDebugApp.test.tsx`, extend the hoisted command mock:

```ts
refreshLv1Discovery: vi.fn(),
```

Add this test:

```ts
it("requests LV1 discovery when the debug app starts", async () => {
  commands.refreshLv1Discovery.mockResolvedValueOnce(undefined);

  renderWithAppProviders(
    <SmokeDebugApp
      services={{
        frontendReady: vi.fn(async () => undefined),
        listenForAppStatus: vi.fn(async () => () => {}),
      }}
    />,
  );

  expect(await screen.findByText("Searching for LV1 systems...")).toBeInTheDocument();
  expect(commands.refreshLv1Discovery).toHaveBeenCalled();
  expect(commands.runConnectionTest).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run test to verify failure**

Run from `ui/`:

```bash
npm run test -- SmokeDebugApp
```

Expected: FAIL because `SmokeDebugApp` does not pass `onRefreshLv1Discovery` yet.

- [ ] **Step 3: Wire command into app**

In `ui/src/debug/SmokeDebugApp.tsx`, add `refreshLv1Discovery` to the command imports:

```ts
refreshLv1Discovery,
```

Pass the prop to `SmokeTestPanel`:

```tsx
onRefreshLv1Discovery={refreshLv1Discovery}
```

- [ ] **Step 4: Verify app test passes**

Run from `ui/`:

```bash
npm run test -- SmokeDebugApp
```

Expected: PASS.

### Task 4: Final Verification

**Files:**
- No new files unless tests reveal a required source fix.

**Interfaces:**
- Consumes: Tasks 1-3 complete.
- Produces: Verified debug auto-discovery implementation.

- [ ] **Step 1: Run targeted UI tests**

Run from `ui/`:

```bash
npm run test -- SmokeDebugApp SmokeTestPanel commands
```

Expected: PASS.

- [ ] **Step 2: Run frontend typecheck**

Run from `ui/`:

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 3: Run formatting check**

Run from `ui/`:

```bash
npm run format:check
```

Expected: PASS.

- [ ] **Step 4: Run lint**

Run from `ui/`:

```bash
npm run lint
```

Expected: PASS.

- [ ] **Step 5: Inspect diff**

Run from repo root:

```bash
git diff -- ui/src/debug/commands.ts ui/src/debug/commands.test.ts ui/src/debug/SmokeDebugApp.tsx ui/src/debug/SmokeDebugApp.test.tsx ui/src/debug/SmokeTestPanel.tsx ui/src/debug/SmokeTestPanel.test.tsx
```

Expected: Diff is limited to debug frontend auto-discovery and tests.
