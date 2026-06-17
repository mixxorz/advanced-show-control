# Real Frontend Shell And Scenes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the test-bed connected UI with a real console-style app shell and production-ready Scenes tab.

**Architecture:** Keep the existing Tauri state and command providers. Build bottom-up from Tailwind/CSS tokens to one-file React components, then compose the Scenes workflow and full app shell. Non-Scenes tabs are navigable placeholders except Logs, which continues to render existing frontend log data.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4, Vite, Storybook, Playwright visual snapshots, Tauri command bridge.

## Global Constraints

- Preserve the existing Tauri `AppViewState` and command contracts unless a frontend-only type is needed.
- Keep global safety and connection state visible at all times.
- The bottom status bar must show a live clock, not `lastEventAt`.
- Use IBM Plex Sans for UI text and IBM Plex Mono for numeric/status text.
- Define reusable Tailwind/CSS theme variables for fonts, console colors, orange accent states, surfaces, borders, status colors, sizing, and interaction states.
- Avoid hard-coded Tailwind values when a reusable token is appropriate.
- Build component by component from smallest to largest.
- Put each React component in its own file.
- Use a near-black neutral shell, flat neutral charcoal panels, slightly lighter neutral sections, and minimal depth.
- Do not implement Playlists, Events, Sessions, or Settings workflows.
- Do not change backend command contracts or safety behavior.

---

## File Structure

- Modify `ui/src/index.css`: define IBM Plex font faces/imports for development, Tailwind `@theme` tokens, root font defaults, and base body styles.
- Create `ui/src/components/ConsoleButton.tsx`: shared console button with active, disabled, and variant states.
- Create `ui/src/components/TopTab.tsx`: single top navigation tab.
- Create `ui/src/components/TopTabBar.tsx`: tab bar for Scenes, Playlists, Events, Sessions, Logs, Settings.
- Create `ui/src/components/Panel.tsx`: reusable bordered flat panel.
- Create `ui/src/components/StatusCell.tsx`: bottom-bar label/value cell.
- Create `ui/src/components/PlaceholderTab.tsx`: placeholder view for future tabs.
- Create `ui/src/components/SceneListRow.tsx`: one row in the scene list.
- Create `ui/src/components/ScopeButton.tsx`: one channel-scope button.
- Create `ui/src/components/SceneList.tsx`: left scene list and duplicate-name warning.
- Create `ui/src/components/ScopeToggleGroup.tsx`: FADER/PAN toggles.
- Create `ui/src/components/SelectedSceneHeader.tsx`: selected scene title, toggles, duration, and actions.
- Create `ui/src/components/ChannelScopeGrid.tsx`: grouped scope editor.
- Create `ui/src/components/BottomStatusBar.tsx`: persistent status bar with live clock.
- Create `ui/src/components/ConsoleLogsTab.tsx`: shell-styled logs view using existing log state.
- Modify `ui/src/components/SceneTab.tsx`: replace inline layout with composed scene components.
- Modify `ui/src/components/AppShell.tsx`: replace current connected shell with top tabs, content region, bottom status bar, and placeholders.
- Modify `ui/src/components/AppShell.stories.tsx`: update tab args and assertions for the new shell.
- Modify `ui/src/components/SceneTab.stories.tsx`: update story expectations if labels or layout change.
- Modify `ui/tests/visual/storybook.visual.spec.ts`: update story list if component story IDs change.
- Modify `docs/superpowers/specs/2026-06-17-real-frontend-shell-scenes-design.md`: only if implementation uncovers a necessary scope clarification.

---

### Task 1: Define Design Tokens And Fonts

**Files:**
- Modify: `ui/src/index.css`

**Interfaces:**
- Produces: Tailwind utilities such as `bg-console-bg`, `bg-console-panel`, `bg-console-section`, `text-console-primary`, `text-accent-orange`, `text-status-current`, `font-ui`, `font-mono`, `border-console-line`, `rounded-console-panel`, and `rounded-console-control`.

- [ ] **Step 1: Replace CSS with theme tokens**

Use this content for `ui/src/index.css`:

```css
@import url("https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600;700&family=IBM+Plex+Sans:wght@400;500;600;700&display=swap");
@import "tailwindcss";

@theme {
  --font-ui: "IBM Plex Sans", ui-sans-serif, system-ui, sans-serif;
  --font-mono: "IBM Plex Mono", ui-monospace, SFMono-Regular, monospace;

  --color-console-bg: #050506;
  --color-console-chrome: #080809;
  --color-console-panel: #0d0d0f;
  --color-console-section: #141416;
  --color-console-control: #18181b;
  --color-console-control-hover: #202026;
  --color-console-line: #2a2a2e;
  --color-console-line-soft: #242428;
  --color-console-line-strong: #3a3a40;

  --color-console-primary: #dedbd6;
  --color-console-secondary: #b8b3ab;
  --color-console-muted: #8f8981;
  --color-console-disabled: #56524d;

  --color-accent-orange: #ff8a00;
  --color-accent-orange-hover: #ff9f2a;
  --color-accent-orange-active: #c96400;
  --color-accent-orange-soft: #261404;

  --color-status-current: #52d62d;
  --color-status-cued: #2c9dff;
  --color-status-warning: #f0b429;
  --color-status-danger: #ff5c5c;

  --radius-console-panel: 0.3125rem;
  --radius-console-control: 0.1875rem;
}

:root {
  color-scheme: dark;
  font-family: var(--font-ui);
  background: var(--color-console-bg);
  color: var(--color-console-primary);
}

body {
  margin: 0;
  min-width: 320px;
  background: var(--color-console-bg);
}

button,
input {
  font: inherit;
}
```

- [ ] **Step 2: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS with no TypeScript errors.

- [ ] **Step 3: Run build**

Run: `npm run build` in `ui/`

Expected: PASS and Vite reports a successful production build.

- [ ] **Step 4: Commit**

```bash
git add ui/src/index.css
git commit -m "feat: define console theme tokens"
```

---

### Task 2: Build Small Shared Components

**Files:**
- Create: `ui/src/components/ConsoleButton.tsx`
- Create: `ui/src/components/TopTab.tsx`
- Create: `ui/src/components/TopTabBar.tsx`
- Create: `ui/src/components/Panel.tsx`
- Create: `ui/src/components/StatusCell.tsx`
- Create: `ui/src/components/PlaceholderTab.tsx`

**Interfaces:**
- Consumes: Tailwind tokens from Task 1.
- Produces: `ConsoleButton`, `TopTab`, `TopTabBar`, `Panel`, `StatusCell`, `PlaceholderTab`.
- Produces type: `MainTab = "scenes" | "playlists" | "events" | "sessions" | "logs" | "settings"` exported from `TopTabBar.tsx`.

- [ ] **Step 1: Create `ConsoleButton.tsx`**

```tsx
import type { ReactNode } from "react";

type ConsoleButtonVariant = "primary" | "secondary";

export function ConsoleButton(props: {
  active?: boolean;
  children: ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  variant?: ConsoleButtonVariant;
}) {
  const variant = props.variant ?? "secondary";
  const className = props.active || variant === "primary"
    ? "rounded-console-control border border-accent-orange bg-accent-orange-active px-4 py-2 font-bold text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control disabled:text-console-disabled"
    : "rounded-console-control border border-console-line bg-console-control px-4 py-2 font-bold text-console-primary hover:border-console-line-strong hover:bg-console-control-hover disabled:text-console-disabled";

  return (
    <button className={className} disabled={props.disabled} onClick={props.onClick}>
      {props.children}
    </button>
  );
}
```

- [ ] **Step 2: Create `TopTab.tsx`**

```tsx
import type { ReactNode } from "react";

export function TopTab(props: {
  active: boolean;
  children: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      className={
        props.active
          ? "border-x border-t border-console-line border-b-4 border-b-accent-orange bg-console-panel px-8 py-4 text-sm font-semibold uppercase tracking-[0.12em] text-accent-orange"
          : "border-x border-t border-console-line border-b border-b-console-line bg-console-chrome px-8 py-4 text-sm font-semibold uppercase tracking-[0.12em] text-console-secondary hover:text-console-primary"
      }
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}
```

- [ ] **Step 3: Create `TopTabBar.tsx`**

```tsx
import { TopTab } from "./TopTab";

export type MainTab = "scenes" | "playlists" | "events" | "sessions" | "logs" | "settings";

const tabs: { id: MainTab; label: string }[] = [
  { id: "scenes", label: "Scenes" },
  { id: "playlists", label: "Playlists" },
  { id: "events", label: "Events" },
  { id: "sessions", label: "Sessions" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];

export function TopTabBar(props: {
  activeTab: MainTab;
  onSelectTab: (tab: MainTab) => void;
}) {
  return (
    <nav className="flex border-b border-console-line bg-console-chrome">
      {tabs.map((tab) => (
        <TopTab
          active={props.activeTab === tab.id}
          key={tab.id}
          onClick={() => props.onSelectTab(tab.id)}
        >
          {tab.label}
        </TopTab>
      ))}
    </nav>
  );
}
```

- [ ] **Step 4: Create `Panel.tsx`**

```tsx
import type { ReactNode } from "react";

export function Panel(props: { children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-console-panel border border-console-line bg-console-panel ${props.className ?? ""}`}>
      {props.children}
    </section>
  );
}
```

- [ ] **Step 5: Create `StatusCell.tsx`**

```tsx
import type { ReactNode } from "react";

export function StatusCell(props: {
  label: string;
  tone?: "default" | "current" | "cued" | "warning" | "danger";
  value: ReactNode;
}) {
  const tone = props.tone ?? "default";
  const valueClass = {
    default: "text-console-primary",
    current: "text-status-current",
    cued: "text-status-cued",
    warning: "text-status-warning",
    danger: "text-status-danger",
  }[tone];

  return (
    <div className="min-w-0 border-r border-console-line px-6 py-3 last:border-r-0">
      <div className="text-xs uppercase tracking-[0.08em] text-console-secondary">{props.label}</div>
      <div className={`mt-1 truncate font-mono text-lg font-medium ${valueClass}`}>{props.value}</div>
    </div>
  );
}
```

- [ ] **Step 6: Create `PlaceholderTab.tsx`**

```tsx
import { Panel } from "./Panel";

export function PlaceholderTab(props: { name: string }) {
  return (
    <Panel className="grid min-h-[32rem] place-items-center p-8">
      <div className="max-w-lg text-center">
        <h2 className="text-xl font-semibold uppercase tracking-[0.08em] text-console-primary">{props.name}</h2>
        <p className="mt-3 text-console-secondary">
          This workflow is part of the product shell but is not built yet.
        </p>
      </div>
    </Panel>
  );
}
```

- [ ] **Step 7: Run checks**

Run: `npm run typecheck` in `ui/`

Expected: PASS with no TypeScript errors.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/ConsoleButton.tsx ui/src/components/TopTab.tsx ui/src/components/TopTabBar.tsx ui/src/components/Panel.tsx ui/src/components/StatusCell.tsx ui/src/components/PlaceholderTab.tsx
git commit -m "feat: add console shell primitives"
```

---

### Task 3: Build Scenes Components

**Files:**
- Create: `ui/src/components/SceneListRow.tsx`
- Create: `ui/src/components/SceneList.tsx`
- Create: `ui/src/components/ScopeButton.tsx`
- Create: `ui/src/components/ScopeToggleGroup.tsx`
- Create: `ui/src/components/SelectedSceneHeader.tsx`
- Create: `ui/src/components/ChannelScopeGrid.tsx`

**Interfaces:**
- Consumes: `ConsoleButton`, `Panel`, existing `format.ts` helpers, existing `SceneConfig`, `ChannelConfig`, and `AppViewState` types.
- Produces: components consumed by `SceneTab` in Task 4.

- [ ] **Step 1: Create `SceneListRow.tsx`**

```tsx
import type { SceneConfig, SceneSummary } from "../types";
import { formatSceneDurationSummary, formatSceneNumber } from "../format";

export function SceneListRow(props: {
  currentScene: SceneSummary | null;
  scene: SceneConfig;
  selected: boolean;
  onSelect: () => void;
}) {
  const current = props.currentScene?.index === props.scene.sceneIndex && props.currentScene.name === props.scene.sceneName;

  return (
    <button
      className={
        props.selected
          ? "grid w-full grid-cols-[4rem_1fr_4rem] items-center border border-accent-orange-active bg-accent-orange-soft px-3 py-2 text-left text-console-primary"
          : "grid w-full grid-cols-[4rem_1fr_4rem] items-center border-b border-console-line-soft px-3 py-2 text-left text-console-secondary hover:bg-console-section hover:text-console-primary"
      }
      onClick={props.onSelect}
    >
      <span className={current ? "font-mono text-status-current" : "font-mono"}>{formatSceneNumber(props.scene.sceneIndex)}</span>
      <span className={current ? "truncate text-status-current" : "truncate"}>{props.scene.sceneName}</span>
      <span className={props.selected ? "text-right font-mono text-console-primary" : "text-right font-mono"}>{formatSceneDurationSummary(props.scene.durationMs)}</span>
    </button>
  );
}
```

- [ ] **Step 2: Create `SceneList.tsx`**

```tsx
import type { SceneConfig, SceneSummary } from "../types";
import { Panel } from "./Panel";
import { SceneListRow } from "./SceneListRow";

function duplicateSceneNames(scenes: SceneConfig[]): string[] {
  const counts = new Map<string, number>();
  for (const scene of scenes) counts.set(scene.sceneName, (counts.get(scene.sceneName) ?? 0) + 1);
  return [...counts.entries()].filter(([, count]) => count > 1).map(([name]) => name).sort((a, b) => a.localeCompare(b));
}

export function SceneList(props: {
  currentScene: SceneSummary | null;
  scenes: SceneConfig[];
  selectedSceneId: string | null;
  onSelectScene: (sceneId: string) => void;
}) {
  const duplicateNames = duplicateSceneNames(props.scenes);

  return (
    <Panel className="flex min-h-0 flex-col overflow-hidden">
      <div className="border-b border-console-line px-4 py-3">
        <h2 className="text-base font-semibold uppercase tracking-[0.08em] text-console-primary">Scene List</h2>
      </div>
      <div className="grid grid-cols-[4rem_1fr_4rem] border-b border-console-line-soft px-3 py-2 text-xs uppercase tracking-[0.08em] text-console-secondary">
        <span>#</span><span>Scene Name</span><span className="text-right">X-Fade</span>
      </div>
      {duplicateNames.length > 0 ? (
        <div className="border-b border-status-warning bg-console-section px-3 py-2 text-sm text-status-warning">
          Duplicate scene names: {duplicateNames.join(", ")}
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-auto">
        {props.scenes.length === 0 ? (
          <p className="p-4 text-sm text-console-muted">No scenes loaded.</p>
        ) : props.scenes.map((scene) => (
          <SceneListRow
            currentScene={props.currentScene}
            key={scene.sceneId}
            onSelect={() => props.onSelectScene(scene.sceneId)}
            scene={scene}
            selected={scene.sceneId === props.selectedSceneId}
          />
        ))}
      </div>
    </Panel>
  );
}
```

- [ ] **Step 3: Create `ScopeButton.tsx`**

```tsx
export function ScopeButton(props: {
  active: boolean;
  label: string;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      className={
        props.active
          ? "min-w-10 rounded-console-control border border-accent-orange bg-accent-orange-active px-3 py-2 font-mono font-bold text-white"
          : "min-w-10 rounded-console-control border border-console-line bg-console-control px-3 py-2 font-mono font-bold text-console-primary hover:bg-console-control-hover"
      }
      onClick={props.onClick}
      title={props.title}
    >
      {props.label}
    </button>
  );
}
```

- [ ] **Step 4: Create `ScopeToggleGroup.tsx`**

```tsx
import { ConsoleButton } from "./ConsoleButton";

export function ScopeToggleGroup(props: {
  fadersEnabled: boolean;
  panEnabled: boolean;
  onToggleFaders: () => void;
  onTogglePan: () => void;
}) {
  return (
    <div className="flex gap-2">
      <ConsoleButton active={props.fadersEnabled} onClick={props.onToggleFaders}>FADER</ConsoleButton>
      <ConsoleButton active={props.panEnabled} onClick={props.onTogglePan}>PAN</ConsoleButton>
    </div>
  );
}
```

- [ ] **Step 5: Create `SelectedSceneHeader.tsx`**

```tsx
import type { SceneConfig } from "../types";
import { formatSceneNumber } from "../format";
import { ConsoleButton } from "./ConsoleButton";
import { DurationInput } from "./DurationInput";
import { Panel } from "./Panel";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

export function SelectedSceneHeader(props: {
  scene: SceneConfig;
  onStore: () => void;
  onToggleFaders: () => void;
  onTogglePan: () => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => void;
}) {
  return (
    <Panel className="p-4">
      <div className="grid gap-6 xl:grid-cols-[1fr_auto_auto] xl:items-center">
        <div>
          <div className="text-sm font-semibold uppercase tracking-[0.08em] text-accent-orange">Selected Scene</div>
          <h2 className="mt-2 font-mono text-3xl font-semibold text-console-primary">
            {formatSceneNumber(props.scene.sceneIndex)} <span className="font-ui">{props.scene.sceneName}</span>
          </h2>
        </div>
        <div>
          <div className="mb-2 text-sm uppercase tracking-[0.08em] text-console-secondary">Scene Scope</div>
          <ScopeToggleGroup
            fadersEnabled={props.scene.scopeToggles.faders}
            onToggleFaders={props.onToggleFaders}
            onTogglePan={props.onTogglePan}
            panEnabled={props.scene.scopeToggles.pan}
          />
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <DurationInput durationMs={props.scene.durationMs} sceneId={props.scene.sceneId} setSceneDurationMs={props.setSceneDurationMs} />
          <ConsoleButton onClick={props.onStore} variant="primary">Store</ConsoleButton>
          <ConsoleButton disabled>Cue</ConsoleButton>
          <ConsoleButton disabled>Recall</ConsoleButton>
        </div>
      </div>
    </Panel>
  );
}
```

- [ ] **Step 6: Create `ChannelScopeGrid.tsx`**

```tsx
import type { AppViewState, ChannelConfig, SceneConfig } from "../types";
import { channelButtonLabel, channelDisplayGroup, channelDisplayGroupOrder, channelName, formatDb, formatPanFamilySummary } from "../format";
import { ConsoleButton } from "./ConsoleButton";
import { Panel } from "./Panel";
import { ScopeButton } from "./ScopeButton";

function channelKey(group: number, channel: number) {
  return `${group}:${channel}`;
}

export function ChannelScopeGrid(props: {
  channels: AppViewState["channels"];
  scene: SceneConfig;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const scoped = new Set(props.scene.scopedChannels.map((entry) => channelKey(entry.group, entry.channel)));
  const groups = new Map<string, ChannelConfig[]>();

  for (const config of props.scene.channelConfigs) {
    const groupName = channelDisplayGroup(config.group);
    groups.set(groupName, [...(groups.get(groupName) ?? []), config]);
  }

  const grouped = [...groups.entries()].sort(([a], [b]) => channelDisplayGroupOrder(a) - channelDisplayGroupOrder(b));

  if (props.scene.channelConfigs.length === 0) {
    return <Panel className="p-4 text-sm text-console-muted">Store the current mixer state to choose scoped channels.</Panel>;
  }

  return (
    <Panel className="p-4">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-console-line pb-3">
        <h3 className="text-base font-semibold uppercase tracking-[0.08em] text-console-primary">Channel Scope</h3>
        <div className="flex gap-2">
          <ConsoleButton onClick={() => props.setAllChannelsScoped(props.scene.sceneId, true)}>All</ConsoleButton>
          <ConsoleButton onClick={() => props.setAllChannelsScoped(props.scene.sceneId, false)}>None</ConsoleButton>
        </div>
      </div>
      <div className="mt-4 space-y-4">
        {grouped.map(([groupName, configs]) => (
          <section className="rounded-console-panel border border-console-line-soft bg-console-section p-3" key={groupName}>
            <h4 className="text-xs font-semibold uppercase tracking-[0.08em] text-console-secondary">{groupName}</h4>
            <div className="mt-3 flex flex-wrap gap-2">
              {[...configs].sort((a, b) => a.channel - b.channel).map((config) => {
                const key = channelKey(config.group, config.channel);
                const isScoped = scoped.has(key);
                return (
                  <ScopeButton
                    active={isScoped}
                    key={key}
                    label={channelButtonLabel(config.group, config.channel)}
                    onClick={() => props.setChannelScoped(props.scene.sceneId, config.group, config.channel, !isScoped)}
                    title={`${channelName(props.channels, config.group, config.channel)} · ${formatDb(config.faderDb ?? 0)} · ${formatPanFamilySummary(config)}`}
                  />
                );
              })}
            </div>
          </section>
        ))}
      </div>
    </Panel>
  );
}
```

- [ ] **Step 7: Run checks**

Run: `npm run typecheck` in `ui/`

Expected: PASS with no TypeScript errors.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/SceneListRow.tsx ui/src/components/SceneList.tsx ui/src/components/ScopeButton.tsx ui/src/components/ScopeToggleGroup.tsx ui/src/components/SelectedSceneHeader.tsx ui/src/components/ChannelScopeGrid.tsx
git commit -m "feat: add console scene components"
```

---

### Task 4: Compose Scenes Tab

**Files:**
- Modify: `ui/src/components/SceneTab.tsx`

**Interfaces:**
- Consumes: `SceneList`, `SelectedSceneHeader`, `ChannelScopeGrid` from Task 3.
- Produces: Existing `SceneTab` export with new console layout.

- [ ] **Step 1: Replace `SceneTab.tsx`**

```tsx
import { useAppCommands, useAppState } from "../appHooks";
import { ChannelScopeGrid } from "./ChannelScopeGrid";
import { SceneList } from "./SceneList";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

export function SceneTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const selected = appState.sceneConfigs.find((scene) => scene.sceneId === appState.selectedSceneId);

  return (
    <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[23rem_1fr]">
      <SceneList
        currentScene={appState.currentScene}
        onSelectScene={commands.selectScene}
        scenes={appState.sceneConfigs}
        selectedSceneId={appState.selectedSceneId}
      />
      <div className="min-h-0 space-y-3 overflow-auto">
        {selected ? (
          <>
            <SelectedSceneHeader
              onStore={() => commands.storeSceneConfig(selected.sceneId)}
              onToggleFaders={() => commands.setSceneScopeFadersEnabled(selected.sceneId, !selected.scopeToggles.faders)}
              onTogglePan={() => commands.setSceneScopePanEnabled(selected.sceneId, !selected.scopeToggles.pan)}
              scene={selected}
              setSceneDurationMs={commands.setSceneDurationMs}
            />
            <ChannelScopeGrid
              channels={appState.channels}
              scene={selected}
              setAllChannelsScoped={commands.setAllChannelsScoped}
              setChannelScoped={commands.setChannelScoped}
            />
          </>
        ) : (
          <div className="rounded-console-panel border border-console-line bg-console-panel p-4 text-console-muted">
            Select a scene to edit its scoped channels.
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Run storybook tests**

Run: `npm run test:storybook -- SceneTab` in `ui/`

Expected: PASS, or update story assertions if they refer to removed copy.

- [ ] **Step 3: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/SceneTab.tsx ui/src/components/SceneTab.stories.tsx
git commit -m "feat: compose console scenes tab"
```

---

### Task 5: Build Bottom Status Bar And Logs Tab

**Files:**
- Create: `ui/src/components/BottomStatusBar.tsx`
- Create: `ui/src/components/ConsoleLogsTab.tsx`

**Interfaces:**
- Consumes: `StatusCell`, `Panel`, existing `AppViewState`.
- Produces: `BottomStatusBar` and `ConsoleLogsTab` for `AppShell`.

- [ ] **Step 1: Create `BottomStatusBar.tsx`**

```tsx
import { useEffect, useState } from "react";
import type { AppViewState } from "../types";
import { formatSceneNumber } from "../format";
import { StatusCell } from "./StatusCell";

function formatClock(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function selectedSceneLabel(appState: AppViewState) {
  const selected = appState.sceneConfigs.find((scene) => scene.sceneId === appState.selectedSceneId);
  return selected ? `${formatSceneNumber(selected.sceneIndex)} ${selected.sceneName}` : "None";
}

export function BottomStatusBar(props: { appState: AppViewState }) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const currentScene = props.appState.currentScene
    ? `${formatSceneNumber(props.appState.currentScene.index)} ${props.appState.currentScene.name}`
    : "None";
  const connection = props.appState.connectedLv1Identity?.host
    ? `Connected to ${props.appState.connectedLv1Identity.host}`
    : props.appState.connection;
  const modeTone = props.appState.lockout ? "warning" : props.appState.fadeState === "blocked" ? "danger" : "default";
  const syncValue = props.appState.reconnect.active ? "Reconnecting" : props.appState.connection === "connected" ? "In Sync" : "Offline";
  const syncTone = props.appState.reconnect.active ? "warning" : props.appState.connection === "connected" ? "current" : "danger";

  return (
    <footer className="grid grid-cols-1 border-t border-console-line bg-console-chrome md:grid-cols-[0.8fr_1.2fr_1.4fr_1.8fr_1fr_1fr]">
      <StatusCell label="Mode" tone={modeTone} value={props.appState.lockout ? "LOCKOUT" : props.appState.fadeState.toUpperCase()} />
      <StatusCell label="Current" tone="current" value={currentScene} />
      <StatusCell label="Selected" tone="cued" value={selectedSceneLabel(props.appState)} />
      <StatusCell label="Connection" tone={props.appState.connection === "connected" ? "current" : "danger"} value={connection} />
      <StatusCell label="Sync" tone={syncTone} value={syncValue} />
      <StatusCell label="Time" value={formatClock(now)} />
    </footer>
  );
}
```

- [ ] **Step 2: Create `ConsoleLogsTab.tsx`**

```tsx
import { useAppState } from "../appHooks";
import { Panel } from "./Panel";

const severityClass = {
  info: "text-console-primary",
  warning: "text-status-warning",
  error: "text-status-danger",
};

export function ConsoleLogsTab() {
  const { appState } = useAppState();

  return (
    <Panel className="h-full min-h-[32rem] overflow-hidden">
      <div className="border-b border-console-line px-4 py-3">
        <h2 className="text-base font-semibold uppercase tracking-[0.08em] text-console-primary">Logs</h2>
      </div>
      <div className="max-h-[calc(100vh-14rem)] overflow-auto p-4">
        {appState.logs.length === 0 ? (
          <p className="text-sm text-console-muted">No frontend logs yet.</p>
        ) : (
          <div className="space-y-2">
            {appState.logs.map((entry) => (
              <div className="grid grid-cols-[6rem_6rem_1fr] gap-3 border-b border-console-line-soft pb-2 font-mono text-sm" key={entry.id}>
                <span className="text-console-muted">{entry.timestamp}</span>
                <span className={severityClass[entry.severity]}>{entry.severity.toUpperCase()}</span>
                <span className="font-ui text-console-primary">{entry.message}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </Panel>
  );
}
```

- [ ] **Step 3: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/BottomStatusBar.tsx ui/src/components/ConsoleLogsTab.tsx
git commit -m "feat: add console status and logs"
```

---

### Task 6: Compose Full App Shell

**Files:**
- Modify: `ui/src/components/AppShell.tsx`
- Modify: `ui/src/App.tsx` if current tab literals still use old names.

**Interfaces:**
- Consumes: `MainTab`, `TopTabBar`, `BottomStatusBar`, `ConsoleLogsTab`, `PlaceholderTab`, `SceneTab`.
- Produces: Existing `AppShell` with `activeTab: MainTab` where `MainTab` is imported from `TopTabBar`.

- [ ] **Step 1: Replace `AppShell.tsx`**

```tsx
import { useAppState } from "../appHooks";
import { BottomStatusBar } from "./BottomStatusBar";
import { ConnectionScreen } from "./ConnectionScreen";
import { ConsoleLogsTab } from "./ConsoleLogsTab";
import { PlaceholderTab } from "./PlaceholderTab";
import { SceneTab } from "./SceneTab";
import { type MainTab, TopTabBar } from "./TopTabBar";

export type { MainTab } from "./TopTabBar";

export function AppShell(props: {
  activeTab: MainTab;
  onOpenConnection: () => void;
  onResume: () => void;
  onSelectTab: (tab: MainTab) => void;
  showConnection: boolean;
}) {
  const { appState } = useAppState();

  return (
    <>
      {props.showConnection ? (
        <ConnectionScreen onResume={props.onResume} />
      ) : (
        <main className="grid min-h-screen grid-rows-[auto_1fr_auto] bg-console-bg font-ui text-console-primary">
          <TopTabBar activeTab={props.activeTab} onSelectTab={props.onSelectTab} />
          <section className="min-h-0 p-3">
            {props.activeTab === "scenes" && <SceneTab />}
            {props.activeTab === "playlists" && <PlaceholderTab name="Playlists" />}
            {props.activeTab === "events" && <PlaceholderTab name="Events" />}
            {props.activeTab === "sessions" && <PlaceholderTab name="Sessions" />}
            {props.activeTab === "logs" && <ConsoleLogsTab />}
            {props.activeTab === "settings" && <PlaceholderTab name="Settings" />}
          </section>
          <BottomStatusBar appState={appState} />
        </main>
      )}
      <ReconnectOverlay active={appState.reconnect.active} />
    </>
  );
}

function ReconnectOverlay(props: { active: boolean }) {
  if (!props.active) return null;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-console-bg/70">
      <div className="rounded-console-panel border border-console-line bg-console-panel px-8 py-6 text-xl font-semibold text-console-primary">
        Reconnecting...
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Update `App.tsx` tab defaults**

If `App.tsx` initializes old tab names, update it to use `"scenes"` and `"logs"`. The relevant state should look like:

```tsx
const [activeTab, setActiveTab] = useState<MainTab>("scenes");
```

- [ ] **Step 3: Run typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/AppShell.tsx ui/src/App.tsx
git commit -m "feat: compose console app shell"
```

---

### Task 7: Update Stories And Visual Coverage

**Files:**
- Modify: `ui/src/components/AppShell.stories.tsx`
- Modify: `ui/src/components/SceneTab.stories.tsx`
- Modify: `ui/tests/visual/storybook.visual.spec.ts`

**Interfaces:**
- Consumes: new `MainTab` values from Task 6.
- Produces: Storybook stories that exercise connected shell, logs tab, placeholder tabs, duplicate scene warning, and reconnect overlay.

- [ ] **Step 1: Update AppShell story args**

In `ui/src/components/AppShell.stories.tsx`, update default `activeTab` to `"scenes"`, update `LogsTab` to `"logs"`, and add one placeholder story:

```tsx
export const SettingsPlaceholder: Story = {
  args: {
    activeTab: "settings",
  },
};
```

Update the play assertion to check for the new tab names:

```tsx
await expect(canvas.getByRole("button", { name: "Scenes" })).toBeInTheDocument();
await expect(canvas.getByRole("button", { name: "Settings" })).toBeInTheDocument();
```

- [ ] **Step 2: Update SceneTab stories if needed**

If a story assertion expects old copy such as `Scoped Channels`, update it to `Channel Scope`. Keep the stored selected scene story and duplicate scene story.

- [ ] **Step 3: Update visual story list if needed**

Keep `app-appshell--scene-tab` and `components-scenetab--stored-scene-selected` in `ui/tests/visual/storybook.visual.spec.ts`. Remove visual entries only if their stories were intentionally deleted. Do not remove visual coverage for AppShell or SceneTab.

- [ ] **Step 4: Run Storybook tests**

Run: `npm run test:storybook` in `ui/`

Expected: PASS.

- [ ] **Step 5: Build Storybook**

Run: `npm run build-storybook` in `ui/`

Expected: PASS and `storybook-static/` is regenerated.

- [ ] **Step 6: Update snapshots if the visual changes are intentional**

Run: `npm run test:visual:update` in `ui/`

Expected: PASS and snapshots update for changed AppShell/SceneTab/Logs visuals.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/AppShell.stories.tsx ui/src/components/SceneTab.stories.tsx ui/tests/visual/storybook.visual.spec.ts ui/tests/visual/storybook.visual.spec.ts-snapshots
git commit -m "test: update console shell stories"
```

---

### Task 8: Final Verification And Documentation Check

**Files:**
- Modify: `docs/superpowers/specs/2026-06-17-real-frontend-shell-scenes-design.md` only if implementation differs from the approved spec.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: verified frontend build ready for review.

- [ ] **Step 1: Run frontend typecheck**

Run: `npm run typecheck` in `ui/`

Expected: PASS.

- [ ] **Step 2: Run production build**

Run: `npm run build` in `ui/`

Expected: PASS.

- [ ] **Step 3: Run Storybook tests**

Run: `npm run test:storybook` in `ui/`

Expected: PASS.

- [ ] **Step 4: Run visual tests**

Run: `npm run test:visual` in `ui/`

Expected: PASS after intentional snapshot updates.

- [ ] **Step 5: Inspect git status**

Run: `git status --short`

Expected: only intended frontend, docs, design reference, and snapshot files are changed.

- [ ] **Step 6: Commit final doc drift if needed**

If the spec changed during implementation:

```bash
git add docs/superpowers/specs/2026-06-17-real-frontend-shell-scenes-design.md
git commit -m "docs: update frontend shell design"
```

If no doc drift exists, do not create an empty commit.

---

## Self-Review

- Spec coverage: Tasks cover tokens/fonts, flat neutral visual direction, one-file components, bottom-up build order, Scenes tab, placeholder tabs, Logs integration, bottom clock, Storybook, visual tests, and final verification.
- Placeholder scan: The plan uses `PlaceholderTab` as an intentional component name for future-tab panels; no plan placeholders such as TBD or incomplete steps remain.
- Type consistency: `MainTab` uses plural tab names throughout. Scene components consume existing `AppViewState`, `SceneConfig`, and command signatures.
