# Frontend Provider Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace shared frontend app-state and command prop drilling with typed React context providers while preserving Storybook mockability.

**Architecture:** `App.tsx` remains the runtime owner and provides current `AppViewState`, command error, and typed command handlers through two small contexts. Connected app-level components consume hooks; small reusable leaf components remain prop-driven.

**Tech Stack:** React 19, TypeScript, Vite, Storybook React-Vite, Tauri API.

## Global Constraints

- The refactor is behavior-preserving.
- Do not change Tauri command semantics, shell state projection, safety behavior, or the current visual design.
- `App.tsx` keeps Tauri startup, event subscriptions, discovery polling, reconnect polling, and snapshot version guarding.
- `activeTab` and `showConnection` stay shell-local and explicit.
- Storybook stories for connected components wrap with mock providers.
- Leaf components such as `StatusBadge`, `ShowFileControls`, and `DurationInput` stay prop-driven where their inputs are narrow.
- No new frontend test framework.
- Verification commands: `npm run typecheck`, `npm run build`, and `npm run build-storybook` from `ui/`.

---

## File Structure

- Create `ui/src/appContext.tsx`: owns `AppStateProvider`, `AppCommandsProvider`, `useAppState`, `useAppCommands`, and the `AppCommands` type.
- Create `ui/src/storybook/MockAppProviders.tsx`: wraps stories with state and command providers, defaulting omitted commands to no-op handlers.
- Modify `ui/src/App.tsx`: builds the command object and wraps `AppShell` in providers.
- Modify `ui/src/components/AppShell.tsx`: accepts only shell-local props and reads shared app state from hooks.
- Modify `ui/src/components/Header.tsx`: reads app state and commands from hooks.
- Modify `ui/src/components/ConnectionScreen.tsx`: reads app state and shared connection commands from hooks, while keeping `onResume` as a shell-local prop.
- Modify `ui/src/components/SceneTab.tsx`: reads app state and scene commands from hooks.
- Modify `ui/src/components/LogsTab.tsx`: reads app state from hooks.
- Modify connected Storybook stories under `ui/src/components/*.stories.tsx`: wrap with `MockAppProviders` instead of passing app state and command props.

---

### Task 1: Add Typed Providers And Storybook Mock Wrapper

**Files:**
- Create: `ui/src/appContext.tsx`
- Create: `ui/src/storybook/MockAppProviders.tsx`

**Interfaces:**
- Consumes: `AppViewState`, `Lv1SystemIdentity`, and `disconnectedAppViewState` from `ui/src/types.ts`.
- Produces: `AppStateProvider`, `AppCommandsProvider`, `useAppState()`, `useAppCommands()`, `AppCommands`, and `MockAppProviders`.

- [ ] **Step 1: Create `ui/src/appContext.tsx` with failing-safe hooks**

```tsx
import { createContext, useContext, type ReactNode } from "react";
import type { AppViewState, Lv1SystemIdentity } from "./types";

export type AppCommands = {
  abortAll: () => void;
  disconnect: () => void | Promise<void>;
  newShowFile: () => void;
  openShowFile: () => void;
  saveShowFile: () => void;
  saveShowFileAs: () => void;
  selectScene: (sceneId: string) => void;
  selectSystem: (identity: Lv1SystemIdentity) => void | Promise<void>;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => void;
  setSceneScopePanEnabled: (sceneId: string, enabled: boolean) => void;
  storeSceneConfig: (sceneId: string) => Promise<boolean>;
  toggleLockout: () => void;
};

type AppStateContextValue = {
  appState: AppViewState;
  commandError: string | null;
};

const AppStateContext = createContext<AppStateContextValue | null>(null);
const AppCommandsContext = createContext<AppCommands | null>(null);

export function AppStateProvider(props: AppStateContextValue & { children: ReactNode }) {
  return (
    <AppStateContext.Provider value={{ appState: props.appState, commandError: props.commandError }}>
      {props.children}
    </AppStateContext.Provider>
  );
}

export function AppCommandsProvider(props: { commands: AppCommands; children: ReactNode }) {
  return <AppCommandsContext.Provider value={props.commands}>{props.children}</AppCommandsContext.Provider>;
}

export function useAppState() {
  const value = useContext(AppStateContext);
  if (!value) {
    throw new Error("useAppState must be used within AppStateProvider");
  }
  return value;
}

export function useAppCommands() {
  const value = useContext(AppCommandsContext);
  if (!value) {
    throw new Error("useAppCommands must be used within AppCommandsProvider");
  }
  return value;
}
```

- [ ] **Step 2: Create `ui/src/storybook/MockAppProviders.tsx`**

```tsx
import type { ReactNode } from "react";
import { AppCommandsProvider, AppStateProvider, type AppCommands } from "../appContext";
import { disconnectedAppViewState, type AppViewState } from "../types";

const noop = () => {};
const promiseTrue = async () => true;

export const mockAppCommands: AppCommands = {
  abortAll: noop,
  disconnect: noop,
  newShowFile: noop,
  openShowFile: noop,
  saveShowFile: noop,
  saveShowFileAs: noop,
  selectScene: noop,
  selectSystem: noop,
  setAllChannelsScoped: noop,
  setChannelScoped: noop,
  setSceneDurationMs: promiseTrue,
  setSceneScopeFadersEnabled: noop,
  setSceneScopePanEnabled: noop,
  storeSceneConfig: promiseTrue,
  toggleLockout: noop,
};

export function MockAppProviders(props: {
  appState?: AppViewState;
  commandError?: string | null;
  commands?: Partial<AppCommands>;
  children: ReactNode;
}) {
  return (
    <AppStateProvider appState={props.appState ?? disconnectedAppViewState} commandError={props.commandError ?? null}>
      <AppCommandsProvider commands={{ ...mockAppCommands, ...props.commands }}>
        {props.children}
      </AppCommandsProvider>
    </AppStateProvider>
  );
}
```

- [ ] **Step 3: Run typecheck to verify provider types compile**

Run: `npm run typecheck`

Working directory: `ui/`

Expected: FAIL only if the new provider type signatures are invalid. Fix any type errors in `ui/src/appContext.tsx` or `ui/src/storybook/MockAppProviders.tsx` before continuing. It may PASS if no connected component has been migrated yet.

- [ ] **Step 4: Commit Task 1**

```bash
git add ui/src/appContext.tsx ui/src/storybook/MockAppProviders.tsx
git commit -m "feat: add frontend app providers"
```

---

### Task 2: Wire Production Runtime Through Providers

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/AppShell.tsx`

**Interfaces:**
- Consumes: `AppStateProvider`, `AppCommandsProvider`, and `AppCommands` from `ui/src/appContext.tsx`.
- Produces: `AppShell` props reduced to `activeTab`, `onSelectTab`, `showConnection`, `onOpenConnection`, and `onResume`.

- [ ] **Step 1: Update imports in `ui/src/App.tsx`**

Replace the existing `AppShell` import block with:

```tsx
import { AppCommandsProvider, AppStateProvider, type AppCommands } from "./appContext";
import { AppShell, type MainTab } from "./components/AppShell";
```

- [ ] **Step 2: Build a typed command object in `App.tsx` before the return**

Insert this after the reconnect effect and before `return (`:

```tsx
  const commands: AppCommands = {
    abortAll: () => runVoidCommand("abort_all_fades", applySnapshot, setCommandError),
    disconnect: async () => {
      await runSnapshotCommand("disconnect_lv1", undefined, applySnapshot, setCommandError);
      setShowConnection(true);
    },
    newShowFile: () => runSnapshotCommand("new_show_file", undefined, applySnapshot, setCommandError),
    openShowFile: () => runSnapshotCommand("open_show_file_dialog", undefined, applySnapshot, setCommandError),
    saveShowFile: () => runSnapshotCommand("save_show_file", undefined, applySnapshot, setCommandError),
    saveShowFileAs: () => runSnapshotCommand("save_show_file_as_dialog", undefined, applySnapshot, setCommandError),
    selectScene: (sceneId: string) => runSnapshotCommand("select_scene_config", { sceneId }, applySnapshot, setCommandError),
    selectSystem: async (identity) => {
      setCommandError(null);
      try {
        const snapshot = await connectLv1System(identity);
        applySnapshot(snapshot);
        if (snapshot.connection === "connected") {
          setShowConnection(false);
        }
      } catch (error) {
        setCommandError(String(error));
      }
    },
    setAllChannelsScoped: (sceneId: string, scoped: boolean) =>
      runSnapshotCommand("set_all_channels_scoped", { sceneId, scoped }, applySnapshot, setCommandError),
    setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) =>
      runSnapshotCommand("set_channel_scoped", { sceneId, group, channel, scoped }, applySnapshot, setCommandError),
    setSceneDurationMs: (sceneId: string, durationMs: number) =>
      runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs }, applySnapshot, setCommandError),
    setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) =>
      runSnapshotCommand("set_scene_scope_faders_enabled", { sceneId, enabled }, applySnapshot, setCommandError),
    setSceneScopePanEnabled: (sceneId: string, enabled: boolean) =>
      setSceneScopePanEnabled(sceneId, enabled, applySnapshot, setCommandError),
    storeSceneConfig: (sceneId: string) =>
      runSnapshotCommand("store_scene_config", { sceneId }, applySnapshot, setCommandError),
    toggleLockout: () => runSnapshotCommand("set_lockout", { enabled: !appState.lockout }, applySnapshot, setCommandError),
  };
```

- [ ] **Step 3: Replace the `App.tsx` return JSX**

Replace the current `<AppShell ... />` return with:

```tsx
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
```

- [ ] **Step 4: Reduce `AppShell` props and consume app state**

In `ui/src/components/AppShell.tsx`, replace the type imports and props with:

```tsx
import type { ReactNode } from "react";
import { useAppState } from "../appContext";
import { ConnectionScreen } from "./ConnectionScreen";
import { Header } from "./Header";
import { LogsTab } from "./LogsTab";
import { SceneTab } from "./SceneTab";

export type MainTab = "scene" | "logs";

export function AppShell(props: {
  activeTab: MainTab;
  onOpenConnection: () => void;
  onResume: () => void;
  onSelectTab: (tab: MainTab) => void;
  showConnection: boolean;
}) {
  const { appState } = useAppState();
```

Then replace the conditional children with connected components:

```tsx
      {props.showConnection ? (
        <ConnectionScreen onResume={props.onResume} />
      ) : (
        <main className="min-h-screen bg-slate-950 text-slate-100">
          <Header onOpenConnection={props.onOpenConnection} />
```

Replace the scene/log section with:

```tsx
          <section className="p-6">
            {props.activeTab === "scene" && <SceneTab />}
            {props.activeTab === "logs" && <LogsTab />}
          </section>
```

Replace the reconnect overlay line with:

```tsx
      <ReconnectOverlay active={appState.reconnect.active} />
```

- [ ] **Step 5: Run typecheck and observe expected connected-component errors**

Run: `npm run typecheck`

Working directory: `ui/`

Expected: FAIL because `Header`, `ConnectionScreen`, `SceneTab`, and `LogsTab` still require old props. This verifies Task 2 exposed the intended migration boundary.

- [ ] **Step 6: Commit Task 2 after recording the expected failure**

Do not commit if there are unexpected syntax errors in `App.tsx` or `AppShell.tsx`. If the only failures are old prop requirements from unmigrated connected components, commit:

```bash
git add ui/src/App.tsx ui/src/components/AppShell.tsx
git commit -m "refactor: provide app runtime through context"
```

---

### Task 3: Migrate Connected Components And Stories

**Files:**
- Modify: `ui/src/components/Header.tsx`
- Modify: `ui/src/components/ConnectionScreen.tsx`
- Modify: `ui/src/components/SceneTab.tsx`
- Modify: `ui/src/components/LogsTab.tsx`
- Modify: `ui/src/components/AppShell.stories.tsx`
- Modify: `ui/src/components/Header.stories.tsx`
- Modify: `ui/src/components/ConnectionScreen.stories.tsx`
- Modify: `ui/src/components/SceneTab.stories.tsx`
- Modify: `ui/src/components/LogsTab.stories.tsx`

**Interfaces:**
- Consumes: `useAppState()`, `useAppCommands()`, and `MockAppProviders`.
- Produces: connected components that no longer accept shared `appState`, `commandError`, or command props.

- [ ] **Step 1: Migrate `Header.tsx`**

Replace the import and function signature with:

```tsx
import { useAppCommands, useAppState } from "../appContext";
import { formatSceneNumber } from "../format";
import { ShowFileControls } from "./ShowFileControls";
import { StatusBadge } from "./StatusBadge";

export function Header(props: { onOpenConnection: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();
```

Inside the JSX, replace `props.appState` with `appState`, `props.commandError` with `commandError`, and command props with `commands.*`:

```tsx
onNew={commands.newShowFile}
onOpen={commands.openShowFile}
onSave={commands.saveShowFile}
onSaveAs={commands.saveShowFileAs}
onClick={props.onOpenConnection}
onClick={commands.toggleLockout}
onClick={commands.abortAll}
```

- [ ] **Step 2: Migrate `ConnectionScreen.tsx`**

Replace the import and function signature with:

```tsx
import { useAppCommands, useAppState } from "../appContext";
import type { DiscoveredLv1System, Lv1SystemIdentity } from "../types";

export function ConnectionScreen(props: { onResume: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();
  const isConnected = appState.connection === "connected";
```

Replace state/error usages with `appState` and `commandError`. Replace disconnect and select callbacks with:

```tsx
onClick={commands.disconnect}
onSelectSystem={commands.selectSystem}
```

Keep `onResume={props.onResume}`.

- [ ] **Step 3: Migrate `LogsTab.tsx`**

Replace the file with:

```tsx
import { useAppState } from "../appContext";

export function LogsTab() {
  const { appState } = useAppState();

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
        {appState.logs.length === 0 ? (
          <p className="p-3 text-sm text-slate-400">No events yet.</p>
        ) : (
          appState.logs.map((entry) => (
            <div
              className="grid grid-cols-[9rem_6rem_1fr] gap-3 border-b border-slate-800 px-3 py-2 text-sm last:border-b-0"
              key={entry.id}
            >
              <span className="text-slate-500">{entry.timestamp}</span>
              <span className="uppercase text-slate-400">{entry.severity}</span>
              <span>{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
```

- [ ] **Step 4: Migrate `SceneTab.tsx`**

Replace the first import and function signature with:

```tsx
import { useAppCommands, useAppState } from "../appContext";
import type { AppViewState, ChannelConfig, SceneConfig } from "../types";
```

```tsx
export function SceneTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const selected = appState.sceneConfigs.find((scene) => scene.sceneId === appState.selectedSceneId);
  const duplicateNames = duplicateSceneNames(appState.sceneConfigs);
```

Replace `props.appState` with `appState` and command calls with `commands.*`:

```tsx
onClick={() => commands.selectScene(scene.sceneId)}
onClick={() => commands.storeSceneConfig(selected.sceneId)}
setSceneDurationMs={commands.setSceneDurationMs}
onClick={() => commands.setSceneScopeFadersEnabled(selected.sceneId, !selected.scopeToggles.faders)}
onClick={() => commands.setSceneScopePanEnabled(selected.sceneId, !selected.scopeToggles.pan)}
channels={appState.channels}
setAllChannelsScoped={commands.setAllChannelsScoped}
setChannelScoped={commands.setChannelScoped}
```

Keep `ScopeGrid` prop-driven.

- [ ] **Step 5: Migrate `AppShell.stories.tsx` to mock providers**

Remove the old noop args. Add:

```tsx
import { MockAppProviders } from "../storybook/MockAppProviders";
```

Use a render function that wraps every story:

```tsx
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <AppShell
        activeTab={args.activeTab}
        onOpenConnection={args.onOpenConnection}
        onResume={args.onResume}
        onSelectTab={args.onSelectTab}
        showConnection={args.showConnection}
      />
    </MockAppProviders>
  ),
```

Keep args only for `activeTab`, `appState`, `commandError`, `onOpenConnection`, `onResume`, `onSelectTab`, and `showConnection`.

- [ ] **Step 6: Migrate individual connected component stories**

For `SceneTab.stories.tsx`, add `MockAppProviders` and replace args with render wrapping:

```tsx
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <SceneTab />
    </MockAppProviders>
  ),
```

For `Header.stories.tsx`, wrap with:

```tsx
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <Header onOpenConnection={args.onOpenConnection} />
    </MockAppProviders>
  ),
```

For `ConnectionScreen.stories.tsx`, wrap with:

```tsx
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <ConnectionScreen onResume={args.onResume} />
    </MockAppProviders>
  ),
```

For `LogsTab.stories.tsx`, wrap with:

```tsx
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <LogsTab />
    </MockAppProviders>
  ),
```

- [ ] **Step 7: Run frontend verification**

Run: `npm run typecheck`

Working directory: `ui/`

Expected: PASS.

Run: `npm run build`

Working directory: `ui/`

Expected: PASS.

Run: `npm run build-storybook`

Working directory: `ui/`

Expected: PASS.

- [ ] **Step 8: Commit Task 3**

```bash
git add ui/src/components/Header.tsx ui/src/components/ConnectionScreen.tsx ui/src/components/SceneTab.tsx ui/src/components/LogsTab.tsx ui/src/components/AppShell.stories.tsx ui/src/components/Header.stories.tsx ui/src/components/ConnectionScreen.stories.tsx ui/src/components/SceneTab.stories.tsx ui/src/components/LogsTab.stories.tsx
git commit -m "refactor: consume app providers in frontend components"
```

---

## Final Verification

- [ ] Run `npm run typecheck` from `ui/`; expected PASS.
- [ ] Run `npm run build` from `ui/`; expected PASS.
- [ ] Run `npm run build-storybook` from `ui/`; expected PASS.
- [ ] Run `git status --short`; expected only intentional files or a clean tree.

## Self-Review

- Spec coverage: Tasks cover two contexts, typed hooks, `App.tsx` as runtime owner, connected component hook consumption, Storybook mock-provider wrapping, centralized error behavior, and frontend verification.
- Placeholder scan: No placeholder requirements are left for implementation workers.
- Type consistency: `AppCommands` method names match all planned component and story usages.
