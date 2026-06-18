# Connection Behavior Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement safe startup auto-connect matching, reliable connection modal behavior, top-bar connection controls, and frontend behavior tests.

**Architecture:** Keep Tauri as the production command/event wiring layer and keep React components driven by app state and command context. Backend startup matching stays in `src-tauri/src/commands.rs`; frontend behavior is tested through Vitest/React Testing Library with mock app state/commands and an injectable app runtime boundary for lifecycle tests.

**Tech Stack:** Rust/Tauri commands, React 19, TypeScript, Vitest, React Testing Library, Storybook, Playwright visual snapshots.

## Global Constraints

- On app open, the connection modal automatically opens.
- Startup auto-connect matches the last connected system by UUID first, then by exact trimmed hostname only when exactly one available discovered system matches.
- Do not fall back to IP address and port for startup auto-connect.
- If auto-connect cannot safely choose or connect to a system, leave the modal open and allow offline use.
- Manual connection attempts close the modal only after a connected snapshot confirms the selected system is connected.
- Manual connection failures keep the modal open and display the error.
- Clicking the modal close button is allowed and leaves the app offline.
- Unavailable systems do not start connection attempts.
- The top-bar connection control opens the connection modal while connected, connecting, or offline.
- The top-bar indicator must not show connected while connecting or offline.
- The connection modal shows system name, IP address, port, latency, availability, and connected status.
- The currently connected system is highlighted blue using existing console design language.
- Frontend behavior tests use Vitest and React Testing Library; Storybook stories remain visual documentation and Playwright snapshots remain visual regression coverage.
- Preserve safety-critical LV1 behavior: do not send fader commands while disconnected, connecting, stale, unavailable, or unsafe.

---

## File Structure

- Modify `src-tauri/src/commands.rs`: update `remembered_auto_connect_target`, add focused unit tests, and leave reconnect matching unchanged.
- Modify `ui/package.json` and `ui/package-lock.json`: add React Testing Library packages if missing.
- Create `ui/src/test/render.tsx`: shared test render helper using `MockAppProviders`.
- Create `ui/src/test/deferred.ts`: helper for pending promise lifecycle tests.
- Modify `ui/src/components/ConnectionModal.tsx`: disable unavailable rows, add accessible labels/status text as needed, and centralize connected identity matching.
- Test `ui/src/components/ConnectionModal.test.tsx`: modal row rendering, connected highlight, unavailable click suppression, row click behavior, close behavior.
- Modify `ui/src/components/TopTabBar.tsx`: accept `onOpenConnection`, show connected/connecting/offline correctly, and wire the button.
- Modify `ui/src/components/AppShell.tsx`: pass `onOpenConnection` into `TopTabBar`.
- Test `ui/src/components/TopTabBar.test.tsx`: indicator states and open-modal button behavior.
- Create `ui/src/AppRuntime.tsx`: extracted stateful runtime component with injectable command/event functions.
- Modify `ui/src/App.tsx`: thin production wrapper that passes real Tauri command functions and event subscription into `AppRuntime`.
- Test `ui/src/AppRuntime.test.tsx`: startup modal lifecycle, auto-connect success/failure, manual pending/success/failure, offline close.
- Modify Storybook stories only if required to keep existing visual states accurate after component API changes.

---

### Task 1: Backend Startup Match Rule

**Files:**
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `crate::connection_preferences::ConnectionPreferences`, `crate::connection_state::DiscoveredLv1System`, `crate::connection_state::DiscoveredLv1Status`.
- Produces: `remembered_auto_connect_target(preferences: &ConnectionPreferences, systems: &[DiscoveredLv1System]) -> Option<Lv1SystemIdentity>` with UUID-first and hostname fallback behavior.

- [ ] **Step 1: Write failing backend tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/commands.rs`, near the current `remembered_auto_connect_target` tests:

```rust
    fn remembered_preferences(
        uuid: Option<&str>,
        host: Option<&str>,
    ) -> crate::connection_preferences::ConnectionPreferences {
        crate::connection_preferences::ConnectionPreferences {
            last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
                uuid: uuid.map(str::to_string),
                host: host.map(str::to_string),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        }
    }

    fn discovered_system(
        uuid: Option<&str>,
        host: Option<&str>,
        address: &str,
        status: crate::connection_state::DiscoveredLv1Status,
    ) -> crate::connection_state::DiscoveredLv1System {
        crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: uuid.map(str::to_string),
                host: host.map(str::to_string),
                address: address.to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status,
        }
    }

    #[test]
    fn remembered_hostname_fallback_matches_single_available_system() {
        let preferences = remembered_preferences(Some("uuid-1"), Some(" LV1-FOH "));
        let systems = vec![discovered_system(
            Some("uuid-2"),
            Some("LV1-FOH"),
            "10.0.0.20",
            crate::connection_state::DiscoveredLv1Status::Available,
        )];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.20");
    }

    #[test]
    fn remembered_uuid_match_takes_precedence_over_hostname_match() {
        let preferences = remembered_preferences(Some("uuid-1"), Some("LV1-FOH"));
        let systems = vec![
            discovered_system(
                Some("uuid-2"),
                Some("LV1-FOH"),
                "10.0.0.20",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
            discovered_system(
                Some("uuid-1"),
                Some("Renamed LV1"),
                "10.0.0.21",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
        ];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.21");
    }

    #[test]
    fn remembered_hostname_fallback_rejects_duplicate_available_hosts() {
        let preferences = remembered_preferences(None, Some("LV1-FOH"));
        let systems = vec![
            discovered_system(
                None,
                Some("LV1-FOH"),
                "10.0.0.20",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
            discovered_system(
                None,
                Some("LV1-FOH"),
                "10.0.0.21",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
        ];

        assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
    }

    #[test]
    fn remembered_hostname_fallback_ignores_unavailable_systems() {
        let preferences = remembered_preferences(None, Some("LV1-FOH"));
        let systems = vec![discovered_system(
            None,
            Some("LV1-FOH"),
            "10.0.0.20",
            crate::connection_state::DiscoveredLv1Status::Unavailable,
        )];

        assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
    }
```

- [ ] **Step 2: Run backend tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control-tauri commands::tests::remembered_hostname_fallback_matches_single_available_system commands::tests::remembered_uuid_match_takes_precedence_over_hostname_match commands::tests::remembered_hostname_fallback_rejects_duplicate_available_hosts commands::tests::remembered_hostname_fallback_ignores_unavailable_systems`

Expected: the hostname fallback test fails because `remembered_auto_connect_target` currently only matches UUID.

- [ ] **Step 3: Implement minimal backend matching change**

Replace `remembered_auto_connect_target` in `src-tauri/src/commands.rs` with:

```rust
fn remembered_auto_connect_target(
    preferences: &crate::connection_preferences::ConnectionPreferences,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Option<crate::connection_state::Lv1SystemIdentity> {
    let remembered = preferences.last_connected_lv1.as_ref()?;
    let available_systems = systems
        .iter()
        .filter(|system| system.status == crate::connection_state::DiscoveredLv1Status::Available);

    if let Some(remembered_uuid) = remembered.uuid.as_ref() {
        if let Some(system) = available_systems
            .clone()
            .find(|system| system.identity.uuid.as_ref() == Some(remembered_uuid))
        {
            return Some(system.identity.clone());
        }
    }

    let remembered_host = remembered.host.as_deref()?.trim();
    if remembered_host.is_empty() {
        return None;
    }

    let mut host_matches = available_systems
        .filter(|system| system.identity.host.as_deref().map(str::trim) == Some(remembered_host));
    let first = host_matches.next()?;
    if host_matches.next().is_some() {
        return None;
    }

    Some(first.identity.clone())
}
```

- [ ] **Step 4: Run backend tests to verify they pass**

Run: `cargo nextest run -p advanced-show-control-tauri remembered_`

Expected: all remembered auto-connect tests pass.

- [ ] **Step 5: Run formatter**

Run: `cargo fmt --all -- --check`

Expected: PASS. If it fails, run `cargo fmt --all`, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 6: Commit backend rule**

Run:

```bash
git status --short
git diff -- src-tauri/src/commands.rs
git add src-tauri/src/commands.rs
git commit -m "fix: match startup connection by hostname fallback"
```

Expected: commit includes only `src-tauri/src/commands.rs`.

---

### Task 2: Frontend Test Harness

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/package-lock.json`
- Create: `ui/src/test/render.tsx`
- Create: `ui/src/test/deferred.ts`

**Interfaces:**
- Produces: `renderWithAppProviders(ui, options)` for component tests.
- Produces: `createDeferred<T>()` for controlled promise tests.

- [ ] **Step 1: Add frontend testing dependencies**

Run: `npm --prefix ui install --save-dev @testing-library/react @testing-library/user-event`

Expected: `ui/package.json` and `ui/package-lock.json` update with the new dev dependencies.

- [ ] **Step 2: Create shared render helper**

Create `ui/src/test/render.tsx`:

```tsx
import { render, type RenderOptions } from "@testing-library/react";
import type { ReactElement } from "react";
import type { AppCommands } from "../appContext";
import { MockAppProviders } from "../storybook/MockAppProviders";
import type { AppViewState } from "../types";

export function renderWithAppProviders(
  ui: ReactElement,
  options: RenderOptions & {
    appState?: AppViewState;
    commandError?: string | null;
    commands?: Partial<AppCommands>;
  } = {},
) {
  const { appState, commandError, commands, ...renderOptions } = options;

  return render(ui, {
    wrapper: ({ children }) => (
      <MockAppProviders
        appState={appState}
        commandError={commandError}
        commands={commands}
      >
        {children}
      </MockAppProviders>
    ),
    ...renderOptions,
  });
}
```

- [ ] **Step 3: Create deferred promise helper**

Create `ui/src/test/deferred.ts`:

```ts
export type Deferred<T> = {
  promise: Promise<T>;
  reject: (reason?: unknown) => void;
  resolve: (value: T | PromiseLike<T>) => void;
};

export function createDeferred<T>(): Deferred<T> {
  let resolve!: Deferred<T>["resolve"];
  let reject!: Deferred<T>["reject"];
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });

  return { promise, reject, resolve };
}
```

- [ ] **Step 4: Verify empty unit test project still passes**

Run: `npm --prefix ui run test`

Expected: PASS. The unit project may report no tests or only helper compilation.

- [ ] **Step 5: Commit test harness**

Run:

```bash
git status --short
git diff -- ui/package.json ui/package-lock.json ui/src/test/render.tsx ui/src/test/deferred.ts
git add ui/package.json ui/package-lock.json ui/src/test/render.tsx ui/src/test/deferred.ts
git commit -m "test: add frontend render helpers"
```

Expected: commit includes only frontend test harness files and package metadata.

---

### Task 3: Connection Modal Behavior

**Files:**
- Modify: `ui/src/components/ConnectionModal.tsx`
- Create: `ui/src/components/ConnectionModal.test.tsx`

**Interfaces:**
- Consumes: `renderWithAppProviders` from `ui/src/test/render.tsx`.
- Produces: connected identity helper internal to `ConnectionModal.tsx`; unavailable rows do not call `selectSystem`.

- [ ] **Step 1: Write failing modal tests**

Create `ui/src/components/ConnectionModal.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState, discoveredSystemsAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import type { AppViewState, DiscoveredLv1System } from "../types";
import { ConnectionModal } from "./ConnectionModal";

function renderModal(options: {
  appState?: AppViewState;
  commandError?: string | null;
  onResume?: () => void;
  selectSystem?: (identity: DiscoveredLv1System["identity"]) => void;
} = {}) {
  return renderWithAppProviders(
    <ConnectionModal onResume={options.onResume ?? vi.fn()} />,
    {
      appState: options.appState ?? discoveredSystemsAppState,
      commandError: options.commandError,
      commands: options.selectSystem
        ? { selectSystem: options.selectSystem }
        : undefined,
    },
  );
}

describe("ConnectionModal", () => {
  it("renders discovered system details", () => {
    renderModal();

    expect(screen.getByText("FOH LV1")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.42:22000")).toBeInTheDocument();
    expect(screen.getByText("3 ms")).toBeInTheDocument();
    expect(screen.getByText("Available")).toBeInTheDocument();
    expect(screen.getByText("LV1 Console")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.43:22000")).toBeInTheDocument();
    expect(screen.getByText("-- ms")).toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });

  it("shows command errors", () => {
    renderModal({ commandError: "LV1 did not connect" });

    expect(screen.getByText("LV1 did not connect")).toBeInTheDocument();
  });

  it("calls onResume from the close button", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    renderModal({ onResume });

    await user.click(screen.getByLabelText("Close connection modal"));

    expect(onResume).toHaveBeenCalledTimes(1);
  });

  it("selects available systems", async () => {
    const user = userEvent.setup();
    const selectSystem = vi.fn();
    renderModal({ selectSystem });

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(selectSystem).toHaveBeenCalledWith({
      uuid: "lv1-demo",
      host: "FOH LV1",
      address: "192.168.1.42",
      port: 22000,
    });
  });

  it("does not select unavailable systems", async () => {
    const user = userEvent.setup();
    const selectSystem = vi.fn();
    renderModal({ selectSystem });

    await user.click(screen.getByRole("button", { name: /LV1 Console/i }));

    expect(selectSystem).not.toHaveBeenCalled();
  });

  it("highlights the currently connected system", () => {
    const appState: AppViewState = {
      ...connectedAppState,
      discoveredLv1Systems: [
        {
          identity: connectedAppState.connectedLv1Identity!,
          latencyMs: 4,
          status: "connected",
        },
      ],
    };
    renderModal({ appState });

    expect(screen.getByRole("button", { name: /FOH LV1/i })).toHaveClass(
      "border-status-current",
    );
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run modal tests to verify failure**

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: FAIL because unavailable rows still call `selectSystem` and/or accessible names/classes need adjustment.

- [ ] **Step 3: Implement modal behavior**

Update `ui/src/components/ConnectionModal.tsx` with these targeted changes:

```tsx
import { useAppCommands, useAppState } from "../appHooks";
import type { DiscoveredLv1System, Lv1SystemIdentity } from "../types";

export function ConnectionModal(props: { onResume: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/75 p-6 font-ui text-console-primary">
      <section className="grid h-[min(52vh,22rem)] max-h-full w-full max-w-xl grid-rows-[auto_1fr] gap-5 overflow-hidden rounded-console-panel border border-console-line bg-console-panel/95 px-6 py-6 shadow-2xl">
        <div className="flex items-start justify-between gap-6 border-b border-console-line pb-4">
          <div className="min-w-0">
            <h1 className="text-lg font-normal uppercase text-console-primary">
              Connect to LV1
            </h1>
          </div>

          <button
            aria-label="Close connection modal"
            className="relative h-7 w-7 text-console-secondary hover:text-console-primary"
            onClick={props.onResume}
          >
            <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 rotate-45 rounded-full bg-current" />
            <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 -rotate-45 rounded-full bg-current" />
          </button>
        </div>

        <div className="grid min-h-0 grid-rows-[auto_1fr] gap-3">
          {commandError && (
            <p className="rounded-console-control border border-status-danger bg-console-section px-3 py-2 text-sm text-status-danger">
              {commandError}
            </p>
          )}

          <div className="grid min-h-0 content-start gap-3 overflow-auto">
            {appState.discoveredLv1Systems.length === 0 ? (
              <div className="rounded-console-panel border border-console-line bg-console-section p-5 text-base text-console-secondary">
                Searching for consoles...
              </div>
            ) : (
              appState.discoveredLv1Systems.map((system) => (
                <SystemRow
                  connectedIdentity={appState.connectedLv1Identity}
                  key={systemKey(system)}
                  system={system}
                  onSelectSystem={commands.selectSystem}
                  onResume={props.onResume}
                />
              ))
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

function SystemRow(props: {
  connectedIdentity: Lv1SystemIdentity | null;
  system: DiscoveredLv1System;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  const { system } = props;
  const isConnected = identitiesMatch(system.identity, props.connectedIdentity);
  const isUnavailable = system.status === "unavailable";
  const rowClass = isConnected
    ? "border-status-current bg-console-section/70 hover:border-status-current hover:bg-console-control/70"
    : isUnavailable
      ? "cursor-not-allowed border-console-line bg-console-section/40 opacity-70"
      : "border-console-line bg-console-section/70 hover:border-console-line-strong hover:bg-console-control/70";

  return (
    <button
      className={`grid gap-3 rounded-console-control border px-4 py-2.5 text-left md:grid-cols-[1fr_auto_auto] md:items-center ${rowClass}`}
      onClick={() => {
        if (isConnected) {
          props.onResume();
          return;
        }
        if (!isUnavailable) {
          props.onSelectSystem(system.identity);
        }
      }}
      type="button"
    >
      <div className="grid min-w-0 grid-cols-[auto_1fr] items-center gap-x-3 gap-y-0.5">
        <span
          className={
            isUnavailable
              ? "row-span-2 h-2 w-2 rounded-full bg-status-danger"
              : isConnected
                ? "row-span-2 h-2 w-2 rounded-full bg-status-current"
                : "row-span-2 h-2 w-2 rounded-full bg-status-cued"
          }
        />
        <div className="truncate text-base font-normal text-console-primary">
          {system.identity.host ?? "LV1 Console"}
        </div>
        <div className="font-mono text-xs text-console-secondary">
          {system.identity.address}:{system.identity.port}
        </div>
      </div>
      {isUnavailable ? (
        <div className="font-mono text-sm text-status-danger md:justify-self-end">
          Unavailable
        </div>
      ) : isConnected ? (
        <div className="flex items-center gap-3 font-mono text-sm text-status-current md:justify-self-end">
          <span>Connected</span>
          <span className="h-4 border-l border-console-line" />
          <span>
            {system.latencyMs === null ? "-- ms" : `${system.latencyMs} ms`}
          </span>
        </div>
      ) : (
        <div className="flex items-center gap-3 font-mono text-sm text-status-cued md:justify-self-end">
          <span>Available</span>
          <span className="h-4 border-l border-console-line" />
          <span>
            {system.latencyMs === null ? "-- ms" : `${system.latencyMs} ms`}
          </span>
        </div>
      )}
      <span className="h-2.5 w-2.5 rotate-45 border-t-2 border-r-2 border-console-secondary md:justify-self-end" />
    </button>
  );
}

function identitiesMatch(
  system: Lv1SystemIdentity,
  connected: Lv1SystemIdentity | null,
) {
  if (!connected) {
    return false;
  }
  if (system.uuid && connected.uuid) {
    return system.uuid === connected.uuid;
  }
  return (
    system.host === connected.host &&
    system.address === connected.address &&
    system.port === connected.port
  );
}

function systemKey(system: DiscoveredLv1System) {
  return (
    system.identity.uuid ?? `${system.identity.address}:${system.identity.port}`
  );
}
```

- [ ] **Step 4: Run modal tests to verify pass**

Run: `npm --prefix ui run test -- ConnectionModal.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit modal behavior**

Run:

```bash
git status --short
git diff -- ui/src/components/ConnectionModal.tsx ui/src/components/ConnectionModal.test.tsx
git add ui/src/components/ConnectionModal.tsx ui/src/components/ConnectionModal.test.tsx
git commit -m "fix: keep unavailable systems non-actionable"
```

Expected: commit includes only modal component and test.

---

### Task 4: Top-Bar Connection Control

**Files:**
- Modify: `ui/src/components/TopTabBar.tsx`
- Modify: `ui/src/components/AppShell.tsx`
- Create: `ui/src/components/TopTabBar.test.tsx`
- Update stories if TypeScript requires `onOpenConnection` prop: `ui/src/components/TopTabBar.stories.tsx`, `ui/src/components/AppShell.stories.tsx`

**Interfaces:**
- Consumes: `AppViewState.connection` and `connectedLv1Identity`.
- Produces: `TopTabBar(props: { activeTab; onSelectTab; onOpenConnection })`.

- [ ] **Step 1: Write failing top-bar tests**

Create `ui/src/components/TopTabBar.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { TopTabBar } from "./TopTabBar";

function renderTopBar(appState: AppViewState, onOpenConnection = vi.fn()) {
  renderWithAppProviders(
    <TopTabBar
      activeTab="scenes"
      onOpenConnection={onOpenConnection}
      onSelectTab={vi.fn()}
    />,
    { appState },
  );
  return { onOpenConnection };
}

describe("TopTabBar", () => {
  it("shows connected status and console name", () => {
    renderTopBar(connectedAppState);

    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /FOH LV1/i })).toBeInTheDocument();
  });

  it("shows offline status when disconnected", () => {
    renderTopBar(disconnectedAppViewState);

    expect(screen.getByText("Offline")).toBeInTheDocument();
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();
  });

  it("shows connecting status without reporting connected", () => {
    renderTopBar({ ...disconnectedAppViewState, connection: "connecting" });

    expect(screen.getByText("Connecting")).toBeInTheDocument();
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();
  });

  it("opens the connection modal from the console button", async () => {
    const user = userEvent.setup();
    const { onOpenConnection } = renderTopBar(connectedAppState);

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(onOpenConnection).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run top-bar tests to verify failure**

Run: `npm --prefix ui run test -- TopTabBar.test.tsx`

Expected: FAIL because `TopTabBar` does not accept `onOpenConnection` and does not show connecting state.

- [ ] **Step 3: Implement top-bar behavior**

Replace `TopTabBar` in `ui/src/components/TopTabBar.tsx` with:

```tsx
import { useAppState } from "../appHooks";
import { TopTab } from "./TopTab";

export type MainTab =
  | "scenes"
  | "playlists"
  | "events"
  | "sessions"
  | "logs"
  | "settings";

const tabs: { id: MainTab; label: string }[] = [
  { id: "scenes", label: "Scenes" },
  { id: "playlists", label: "Cue Lists" },
  { id: "events", label: "Events" },
  { id: "sessions", label: "Sessions" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];

export function TopTabBar(props: {
  activeTab: MainTab;
  onOpenConnection: () => void;
  onSelectTab: (tab: MainTab) => void;
}) {
  const { appState } = useAppState();
  const connected = appState.connection === "connected";
  const connecting = appState.connection === "connecting";
  const consoleName = appState.connectedLv1Identity?.host ?? "Console A";
  const statusLabel = connected
    ? "Connected"
    : connecting
      ? "Connecting"
      : "Offline";
  const statusClass = connected
    ? "text-status-cued"
    : connecting
      ? "text-console-secondary"
      : "text-status-danger";
  const dotClass = connected
    ? "bg-status-cued"
    : connecting
      ? "bg-console-secondary"
      : "bg-status-danger";

  return (
    <nav className="mx-3 mt-3 flex overflow-hidden rounded-console-panel border border-console-line bg-console-chrome">
      <div className="flex min-w-0 flex-1">
        {tabs.map((tab) => (
          <TopTab
            active={props.activeTab === tab.id}
            key={tab.id}
            onClick={() => props.onSelectTab(tab.id)}
          >
            {tab.label}
          </TopTab>
        ))}
      </div>
      <div className="flex items-center gap-5 px-4">
        <div
          className={`flex items-center gap-2 font-mono text-sm font-normal uppercase ${statusClass}`}
        >
          <span className={`h-2.5 w-2.5 rounded-full ${dotClass}`} />
          {statusLabel}
        </div>
        <button
          className="flex min-w-36 items-center justify-between gap-4 rounded-console-control border border-console-line bg-black/20 px-3 py-2 text-base font-normal uppercase text-console-primary shadow-inner hover:border-console-line-strong"
          onClick={props.onOpenConnection}
          type="button"
        >
          <span className="truncate">{consoleName}</span>
          <span className="h-0 w-0 border-x-[5px] border-t-[6px] border-x-transparent border-t-console-secondary" />
        </button>
      </div>
    </nav>
  );
}
```

Update `ui/src/components/AppShell.tsx` so the `TopTabBar` call passes `onOpenConnection`:

```tsx
        <TopTabBar
          activeTab={props.activeTab}
          onOpenConnection={props.onOpenConnection}
          onSelectTab={props.onSelectTab}
        />
```

- [ ] **Step 4: Update stories if typecheck fails**

If `TopTabBar.stories.tsx` directly renders `TopTabBar`, add `onOpenConnection={() => {}}` to that story render. If `AppShell.stories.tsx` already passes `onOpenConnection`, do not change it.

- [ ] **Step 5: Run top-bar tests**

Run: `npm --prefix ui run test -- TopTabBar.test.tsx`

Expected: PASS.

- [ ] **Step 6: Run frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 7: Commit top-bar control**

Run:

```bash
git status --short
git diff -- ui/src/components/TopTabBar.tsx ui/src/components/AppShell.tsx ui/src/components/TopTabBar.test.tsx ui/src/components/TopTabBar.stories.tsx
git add ui/src/components/TopTabBar.tsx ui/src/components/AppShell.tsx ui/src/components/TopTabBar.test.tsx
git add ui/src/components/TopTabBar.stories.tsx || true
git commit -m "fix: open connection modal from top bar"
```

Expected: commit includes top-bar component, shell wiring, tests, and story updates only if needed.

---

### Task 5: Injectable App Runtime Lifecycle

**Files:**
- Create: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/App.tsx`
- Create: `ui/src/AppRuntime.test.tsx`

**Interfaces:**
- Produces: `AppRuntime` component with injectable lifecycle functions.
- Produces: default `App` wrapper preserving real Tauri wiring.

- [ ] **Step 1: Write failing lifecycle tests**

Create `ui/src/AppRuntime.test.tsx`:

```tsx
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import { connectedAppState, discoveredSystemsAppState } from "./storybook/mockAppState";
import { createDeferred } from "./test/deferred";
import { disconnectedAppViewState, type AppViewState } from "./types";
import { render } from "@testing-library/react";

function makeServices(overrides: Partial<AppRuntimeServices> = {}): AppRuntimeServices {
  return {
    abortAll: vi.fn(async () => undefined),
    attemptReconnectLv1: vi.fn(async () => disconnectedAppViewState),
    connectLv1System: vi.fn(async () => connectedAppState),
    disconnectLv1: vi.fn(async () => disconnectedAppViewState),
    listenForAppStatus: vi.fn(async () => () => {}),
    newShowFile: vi.fn(async () => disconnectedAppViewState),
    openShowFile: vi.fn(async () => disconnectedAppViewState),
    reconnectTimedOut: vi.fn(async () => disconnectedAppViewState),
    refreshAppState: vi.fn(async () => disconnectedAppViewState),
    refreshLv1Discovery: vi.fn(async () => discoveredSystemsAppState),
    saveShowFile: vi.fn(async () => disconnectedAppViewState),
    saveShowFileAs: vi.fn(async () => disconnectedAppViewState),
    selectSceneConfig: vi.fn(async () => disconnectedAppViewState),
    setAllChannelsScoped: vi.fn(async () => disconnectedAppViewState),
    setChannelScoped: vi.fn(async () => disconnectedAppViewState),
    setLockout: vi.fn(async () => disconnectedAppViewState),
    setSceneDurationMs: vi.fn(async () => disconnectedAppViewState),
    setSceneScopeFadersEnabled: vi.fn(async () => disconnectedAppViewState),
    setSceneScopePanEnabled: vi.fn(async () => disconnectedAppViewState),
    storeSceneConfig: vi.fn(async () => disconnectedAppViewState),
    startupAutoConnectLv1: vi.fn(async () => disconnectedAppViewState),
    ...overrides,
  };
}

describe("AppRuntime connection lifecycle", () => {
  it("opens the connection modal on startup", () => {
    render(<AppRuntime services={makeServices()} />);

    expect(screen.getByRole("heading", { name: "Connect to LV1" })).toBeInTheDocument();
  });

  it("closes the modal after successful startup auto-connect", async () => {
    render(
      <AppRuntime
        services={makeServices({ startupAutoConnectLv1: vi.fn(async () => connectedAppState) })}
      />,
    );

    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
    });
  });

  it("keeps the modal open and displays startup auto-connect errors", async () => {
    render(
      <AppRuntime
        services={makeServices({
          startupAutoConnectLv1: vi.fn(async () => {
            throw new Error("startup failed");
          }),
        })}
      />,
    );

    expect(await screen.findByText("Error: startup failed")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Connect to LV1" })).toBeInTheDocument();
  });

  it("keeps the modal open while manual connect is pending and closes after selected system connects", async () => {
    const user = userEvent.setup();
    const pending = createDeferred<AppViewState>();
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => discoveredSystemsAppState),
      connectLv1System: vi.fn(() => pending.promise),
    });
    render(<AppRuntime services={services} />);
    await screen.findByText("FOH LV1");

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(screen.getByRole("heading", { name: "Connect to LV1" })).toBeInTheDocument();

    await act(async () => {
      pending.resolve(connectedAppState);
      await pending.promise;
    });

    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
    });
  });

  it("keeps the modal open and displays manual connection errors", async () => {
    const user = userEvent.setup();
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => discoveredSystemsAppState),
      connectLv1System: vi.fn(async () => {
        throw new Error("manual failed");
      }),
    });
    render(<AppRuntime services={services} />);
    await screen.findByText("FOH LV1");

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(await screen.findByText("Error: manual failed")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Connect to LV1" })).toBeInTheDocument();
  });

  it("allows the engineer to close the modal and stay offline", async () => {
    const user = userEvent.setup();
    render(<AppRuntime services={makeServices()} />);

    await user.click(screen.getByLabelText("Close connection modal"));

    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
    expect(screen.getByText("Offline")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run lifecycle tests to verify failure**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx`

Expected: FAIL because `AppRuntime` does not exist.

- [ ] **Step 3: Extract app runtime**

Create `ui/src/AppRuntime.tsx` by moving the current stateful logic from `App.tsx` into an injectable component. Use this exact interface and implementation skeleton, preserving the existing commands object behavior:

```tsx
import { useCallback, useEffect, useState } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "./appContext";
import { AppShell, type MainTab } from "./components/AppShell";
import { disconnectedAppViewState, type AppViewState, type Lv1SystemIdentity } from "./types";

export type AppStatusListener = (appState: AppViewState) => void;

export type AppRuntimeServices = {
  abortAll: () => Promise<void> | void;
  attemptReconnectLv1: () => Promise<AppViewState>;
  connectLv1System: (identity: Lv1SystemIdentity) => Promise<AppViewState>;
  disconnectLv1: () => Promise<AppViewState>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
  newShowFile: () => Promise<AppViewState>;
  openShowFile: () => Promise<AppViewState>;
  reconnectTimedOut: (attempt: number) => Promise<AppViewState>;
  refreshAppState: () => Promise<AppViewState>;
  refreshLv1Discovery: () => Promise<AppViewState>;
  saveShowFile: () => Promise<AppViewState>;
  saveShowFileAs: () => Promise<AppViewState>;
  selectSceneConfig: (sceneId: string) => Promise<AppViewState>;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => Promise<AppViewState>;
  setChannelScoped: (
    sceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => Promise<AppViewState>;
  setLockout: (enabled: boolean) => Promise<AppViewState>;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<AppViewState>;
  setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => Promise<AppViewState>;
  setSceneScopePanEnabled: (sceneId: string, enabled: boolean) => Promise<AppViewState>;
  startupAutoConnectLv1: () => Promise<AppViewState>;
  storeSceneConfig: (sceneId: string) => Promise<AppViewState>;
};

export function AppRuntime(props: { services: AppRuntimeServices }) {
  const { services } = props;
  const [activeTab, setActiveTab] = useState<MainTab>("scenes");
  const [showConnection, setShowConnection] = useState(true);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);

  const applySnapshot = useCallback((next: AppViewState) => {
    setAppState((prev) =>
      !prev || next.stateVersion > prev.stateVersion ? next : prev,
    );
  }, []);

  const runSnapshot = useCallback(
    async (command: () => Promise<AppViewState>) => {
      setCommandError(null);
      try {
        const snapshot = await command();
        applySnapshot(snapshot);
        return true;
      } catch (error) {
        setCommandError(String(error));
        try {
          applySnapshot(await services.refreshAppState());
        } catch (refreshError) {
          setCommandError(String(refreshError));
        }
        return false;
      }
    },
    [applySnapshot, services],
  );

  useEffect(() => {
    let cancelled = false;
    void services.startupAutoConnectLv1()
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
        setShowConnection(snapshot.connection !== "connected");
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setCommandError(String(error));
        setShowConnection(true);
      });

    const unlistenPromise = services.listenForAppStatus((snapshot) => {
      if (!cancelled) {
        applySnapshot(snapshot);
      }
    });

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => {
        void unlisten();
      });
    };
  }, [applySnapshot, services]);

  useEffect(() => {
    if (!showConnection) {
      return;
    }
    let cancelled = false;
    async function refreshDiscovery() {
      try {
        const snapshot = await services.refreshLv1Discovery();
        if (cancelled) {
          return;
        }
        setCommandError(null);
        applySnapshot(snapshot);
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
        }
      }
    }
    void refreshDiscovery();
    const interval = window.setInterval(() => {
      void refreshDiscovery();
    }, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [showConnection, applySnapshot, services]);

  useEffect(() => {
    if (!appState.reconnect.active) {
      return;
    }
    const attempt = appState.reconnect.attempt;
    let cancelled = false;
    let reconnectInFlight = false;
    async function attemptReconnect() {
      if (reconnectInFlight) {
        return;
      }
      reconnectInFlight = true;
      try {
        const snapshot = await services.attemptReconnectLv1();
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
        if (snapshot.connection === "connected") {
          setCommandError(null);
          setShowConnection(false);
        }
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
        }
      } finally {
        reconnectInFlight = false;
      }
    }
    void attemptReconnect();
    const interval = window.setInterval(() => {
      void attemptReconnect();
    }, 2000);
    const timer = window.setTimeout(async () => {
      try {
        const snapshot = await services.reconnectTimedOut(attempt);
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
        if (!snapshot.reconnect.active && snapshot.connection !== "connected") {
          setShowConnection(true);
        }
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
          setShowConnection(true);
        }
      }
    }, 15000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
      window.clearTimeout(timer);
    };
  }, [appState.reconnect.active, appState.reconnect.attempt, applySnapshot, services]);

  const commands: AppCommands = {
    abortAll: () => {
      setCommandError(null);
      void Promise.resolve(services.abortAll()).catch((error) => {
        setCommandError(String(error));
      });
    },
    disconnect: async () => {
      await runSnapshot(() => services.disconnectLv1());
      setShowConnection(true);
    },
    newShowFile: () => runSnapshot(() => services.newShowFile()),
    openShowFile: () => runSnapshot(() => services.openShowFile()),
    saveShowFile: () => runSnapshot(() => services.saveShowFile()),
    saveShowFileAs: () => runSnapshot(() => services.saveShowFileAs()),
    selectScene: (sceneId: string) =>
      runSnapshot(() => services.selectSceneConfig(sceneId)),
    selectSystem: async (identity) => {
      setCommandError(null);
      try {
        const snapshot = await services.connectLv1System(identity);
        applySnapshot(snapshot);
        if (
          snapshot.connection === "connected" &&
          identityMatches(snapshot.connectedLv1Identity, identity)
        ) {
          setShowConnection(false);
        }
      } catch (error) {
        setCommandError(String(error));
      }
    },
    setAllChannelsScoped: (sceneId: string, scoped: boolean) =>
      runSnapshot(() => services.setAllChannelsScoped(sceneId, scoped)),
    setChannelScoped: (sceneId, group, channel, scoped) =>
      runSnapshot(() => services.setChannelScoped(sceneId, group, channel, scoped)),
    setSceneDurationMs: (sceneId, durationMs) =>
      runSnapshot(() => services.setSceneDurationMs(sceneId, durationMs)),
    setSceneScopeFadersEnabled: (sceneId, enabled) =>
      runSnapshot(() => services.setSceneScopeFadersEnabled(sceneId, enabled)),
    setSceneScopePanEnabled: (sceneId, enabled) =>
      runSnapshot(() => services.setSceneScopePanEnabled(sceneId, enabled)),
    storeSceneConfig: (sceneId) =>
      runSnapshot(() => services.storeSceneConfig(sceneId)),
    toggleLockout: () => runSnapshot(() => services.setLockout(!appState.lockout)),
  };

  return (
    <AppStateProvider appState={appState} commandError={commandError}>
      <AppCommandsProvider commands={commands}>
        <AppShell
          activeTab={activeTab}
          onOpenConnection={() => setShowConnection(true)}
          onResume={() => setShowConnection(false)}
          onSelectTab={setActiveTab}
          showConnection={showConnection}
        />
      </AppCommandsProvider>
    </AppStateProvider>
  );
}

function identityMatches(
  connected: Lv1SystemIdentity | null,
  selected: Lv1SystemIdentity,
) {
  if (!connected) {
    return false;
  }
  if (connected.uuid && selected.uuid) {
    return connected.uuid === selected.uuid;
  }
  return (
    connected.host === selected.host &&
    connected.address === selected.address &&
    connected.port === selected.port
  );
}
```

- [ ] **Step 4: Replace `App.tsx` with production wiring wrapper**

Replace `ui/src/App.tsx` with:

```tsx
import { listen } from "@tauri-apps/api/event";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import {
  attemptReconnectLv1,
  connectLv1System,
  reconnectTimedOut,
  refreshLv1Discovery,
  startupAutoConnectLv1,
} from "./commands";
import { invoke } from "@tauri-apps/api/core";
import type { AppViewState } from "./types";

const services: AppRuntimeServices = {
  abortAll: () => invoke("abort_all_fades"),
  attemptReconnectLv1,
  connectLv1System,
  disconnectLv1: () => invoke<AppViewState>("disconnect_lv1"),
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) => listener(event.payload)),
  newShowFile: () => invoke<AppViewState>("new_show_file"),
  openShowFile: () => invoke<AppViewState>("open_show_file_dialog"),
  reconnectTimedOut,
  refreshAppState: () => invoke<AppViewState>("get_app_status"),
  refreshLv1Discovery,
  saveShowFile: () => invoke<AppViewState>("save_show_file"),
  saveShowFileAs: () => invoke<AppViewState>("save_show_file_as_dialog"),
  selectSceneConfig: (sceneId) =>
    invoke<AppViewState>("select_scene_config", { sceneId }),
  setAllChannelsScoped: (sceneId, scoped) =>
    invoke<AppViewState>("set_all_channels_scoped", { sceneId, scoped }),
  setChannelScoped: (sceneId, group, channel, scoped) =>
    invoke<AppViewState>("set_channel_scoped", { sceneId, group, channel, scoped }),
  setLockout: (enabled) => invoke<AppViewState>("set_lockout", { enabled }),
  setSceneDurationMs: (sceneId, durationMs) =>
    invoke<AppViewState>("set_scene_duration_ms", { sceneId, durationMs }),
  setSceneScopeFadersEnabled: (sceneId, enabled) =>
    invoke<AppViewState>("set_scene_scope_faders_enabled", { sceneId, enabled }),
  setSceneScopePanEnabled: (sceneId, enabled) =>
    invoke<AppViewState>("set_scene_scope_pan_enabled", { sceneId, enabled }),
  startupAutoConnectLv1,
  storeSceneConfig: (sceneId) =>
    invoke<AppViewState>("store_scene_config", { sceneId }),
};

export default function App() {
  return <AppRuntime services={services} />;
}
```

After this replacement, remove unused `runSnapshotCommand`, `runVoidCommand`, `refreshAppState`, and `setSceneScopePanEnabled` exports from `ui/src/commands.ts` if TypeScript reports them unused and no other file imports them.

- [ ] **Step 5: Run lifecycle tests**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx`

Expected: PASS.

- [ ] **Step 6: Run all unit tests**

Run: `npm --prefix ui run test`

Expected: PASS.

- [ ] **Step 7: Run typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 8: Commit app runtime extraction**

Run:

```bash
git status --short
git diff -- ui/src/App.tsx ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx ui/src/commands.ts
git add ui/src/App.tsx ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx ui/src/commands.ts
git commit -m "test: cover connection startup lifecycle"
```

Expected: commit includes app runtime extraction, lifecycle tests, and any command cleanup only.

---

### Task 6: Storybook State Coverage And Final Verification

**Files:**
- Modify if needed: `ui/src/components/ConnectionModal.stories.tsx`
- Modify if needed: `ui/src/components/TopTabBar.stories.tsx`
- Modify if needed: `ui/src/components/AppShell.stories.tsx`

**Interfaces:**
- Consumes: final component props from Tasks 3-5.
- Produces: Storybook stories that still document searching, found systems, connected highlight, unavailable system, command error, and top-bar connected/offline/connecting states.

- [ ] **Step 1: Run Storybook test/type checks to find story breakage**

Run: `npm run typecheck`

Expected: PASS. If it fails due to missing `onOpenConnection`, update affected stories to pass `onOpenConnection={() => {}}`.

- [ ] **Step 2: Inspect existing connection stories**

Read `ui/src/components/ConnectionModal.stories.tsx`, `ui/src/components/TopTabBar.stories.tsx`, and `ui/src/components/AppShell.stories.tsx`. Confirm stories cover:

```text
Connection modal searching/no systems
Connection modal available systems
Connection modal command error
Connection modal connected system highlighted
Connection modal unavailable system visible
Top bar connected
Top bar offline
Top bar connecting
```

- [ ] **Step 3: Add missing stories only**

If any listed state is missing, add the minimal story variant using existing `MockAppProviders` and existing mock states. Do not add Storybook `play` tests unless the story itself becomes clearer by demonstrating an interaction.

- [ ] **Step 4: Run frontend unit tests**

Run: `npm --prefix ui run test`

Expected: PASS.

- [ ] **Step 5: Run frontend typecheck and build**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Run targeted backend tests**

Run: `cargo nextest run -p advanced-show-control-tauri remembered_`

Expected: PASS.

- [ ] **Step 7: Run workspace formatting and clippy**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 8: Commit story or cleanup changes**

If Step 3 changed stories, run:

```bash
git status --short
git diff -- ui/src/components/ConnectionModal.stories.tsx ui/src/components/TopTabBar.stories.tsx ui/src/components/AppShell.stories.tsx
git add ui/src/components/ConnectionModal.stories.tsx ui/src/components/TopTabBar.stories.tsx ui/src/components/AppShell.stories.tsx
git commit -m "docs: cover connection story states"
```

If no files changed, do not create an empty commit.

---

## Self-Review

Spec coverage:

- Startup modal, auto-connect success/failure, manual success/failure, offline close: Task 5.
- UUID-first and hostname fallback: Task 1.
- No IP fallback and duplicate hostname safety: Task 1.
- Unavailable systems non-actionable: Task 3.
- Top-bar status and modal entry point: Task 4.
- Modal details and connected highlight: Task 3.
- Frontend testing strategy: Tasks 2-5 use Vitest/RTL; Task 6 preserves Storybook/Playwright roles.

Placeholder scan: no placeholders, TODOs, or unspecified validation steps remain.

Type consistency: `AppRuntimeServices`, `renderWithAppProviders`, and `createDeferred` are defined before use. `TopTabBar` prop changes are wired through `AppShell`. Backend helper signature remains unchanged for call sites.
