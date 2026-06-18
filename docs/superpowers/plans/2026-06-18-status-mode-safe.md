# Status Mode Safe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the app shell so the bottom status bar reports `Ready`, `Fading`, `Safe`, or `Offline`, scene fallbacks display `---`, and the top bar exposes a fixed-label `SAFE` toggle.

**Architecture:** Keep this as a frontend-only presentation change. `TopTabBar` consumes existing `appState.lockout` and `toggleLockout`; `BottomStatusBar` derives display labels from existing `connection`, `lockout`, `fadeState`, `cuedSceneId`, and `currentScene` fields.

**Tech Stack:** React, TypeScript, Tailwind CSS theme tokens, Vitest, React Testing Library, Storybook.

## Global Constraints

- Preserve the existing app shell visual language.
- Use existing status color tokens from `ui/src/index.css` rather than adding hard-coded colors.
- Do not change backend lockout or fade safety behavior.
- Manual override or blocked fade events must not be represented as `Mode: Blocked`.
- The `SAFE` button label stays `SAFE` in active and inactive states.
- `SAFE` remains available regardless of LV1 connection.

---

## File Structure

- Modify `ui/src/components/BottomStatusBar.tsx`: derive cued/current fallbacks and mode label/tone/pulse.
- Create `ui/src/components/BottomStatusBar.test.tsx`: focused tests for `---`, `Offline`, `Safe`, `Fading`, and `Ready`.
- Modify `ui/src/components/StatusCell.tsx`: add `className?: string` so `BottomStatusBar` can apply a pulse class only to the mode value.
- Modify `ui/src/components/TopTabBar.tsx`: add the fixed-label `SAFE` button using existing app commands/state.
- Modify `ui/src/components/TopTabBar.test.tsx`: test `SAFE` active/inactive state and toggle behavior.
- Modify `ui/src/components/BottomStatusBar.stories.tsx`: add ready, fading, and safe stories.
- Modify `ui/src/components/TopTabBar.stories.tsx`: add safe-active story if no equivalent exists.

---

### Task 1: Bottom Status Bar Labels And Mode

**Files:**
- Modify: `ui/src/components/StatusCell.tsx`
- Modify: `ui/src/components/BottomStatusBar.tsx`
- Create: `ui/src/components/BottomStatusBar.test.tsx`
- Modify: `ui/src/components/BottomStatusBar.stories.tsx`

**Interfaces:**
- Consumes: `AppViewState.connection`, `AppViewState.lockout`, `AppViewState.fadeState`, `AppViewState.currentScene`, `AppViewState.cuedSceneId`, `AppViewState.sceneConfigs`.
- Produces: `StatusCell` accepts an optional `className?: string` prop for the value element.
- Produces: `BottomStatusBar` renders operator labels `---`, `Offline`, `Safe`, `Fading`, and `Ready`.

- [ ] **Step 1: Write the failing bottom status tests**

Create `ui/src/components/BottomStatusBar.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { BottomStatusBar } from "./BottomStatusBar";

function renderBottomStatusBar(appState: AppViewState) {
  renderWithAppProviders(<BottomStatusBar appState={appState} />, { appState });
}

describe("BottomStatusBar", () => {
  it("shows dashes when no current or cued scene is available", () => {
    renderBottomStatusBar(disconnectedAppViewState);

    expect(screen.getAllByText("---")).toHaveLength(2);
  });

  it("shows offline mode while disconnected", () => {
    renderBottomStatusBar(disconnectedAppViewState);

    expect(screen.getByText("Offline")).toBeInTheDocument();
  });

  it("shows ready mode when connected and idle", () => {
    renderBottomStatusBar(connectedAppState);

    expect(screen.getByText("Ready")).toBeInTheDocument();
  });

  it("shows safe mode before fading when lockout is enabled", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      fadeState: "running",
      lockout: true,
    });

    expect(screen.getByText("Safe")).toBeInTheDocument();
    expect(screen.queryByText("Fading")).not.toBeInTheDocument();
  });

  it("shows fading mode with a pulse while a fade is running", () => {
    renderBottomStatusBar({ ...connectedAppState, fadeState: "running" });

    expect(screen.getByText("Fading")).toHaveClass("animate-pulse");
  });
});
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run from `ui/`:

```bash
npm run test -- BottomStatusBar.test.tsx
```

Expected: FAIL because the new test file expects `---`, `Ready`, `Safe`, and `Fading`, while the current component renders `None`, uppercase raw fade states, and no pulse class.

- [ ] **Step 3: Add optional value classes to `StatusCell`**

Update `ui/src/components/StatusCell.tsx`:

```tsx
import type { ReactNode } from "react";

export function StatusCell(props: {
  className?: string;
  font?: "ui" | "mono";
  label: string;
  tone?: "default" | "current" | "cued" | "warning" | "danger";
  value: ReactNode;
}) {
  const tone = props.tone ?? "default";
  const fontClass = props.font === "mono" ? "font-mono" : "font-ui";
  const valueClass = {
    default: "text-console-primary",
    current: "text-status-current",
    cued: "text-status-cued",
    warning: "text-status-warning",
    danger: "text-status-danger",
  }[tone];

  return (
    <div className="grid min-w-0 content-center border-r border-console-line px-6 py-3 last:border-r-0">
      <div className="text-xs uppercase tracking-[0.08em] text-console-secondary">
        {props.label}
      </div>
      <div
        className={`mt-1 truncate ${fontClass} text-lg font-normal ${valueClass} ${props.className ?? ""}`}
      >
        {props.value}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Implement bottom status mode derivation**

Update `ui/src/components/BottomStatusBar.tsx`:

```tsx
import { useEffect, useState } from "react";
import type { AppViewState } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { StatusCell } from "./StatusCell";

function formatClock(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function cuedSceneLabel(appState: AppViewState) {
  const cued = appState.sceneConfigs.find(
    (scene) => scene.sceneId === appState.cuedSceneId,
  );
  return cued ? cued.sceneName : "---";
}

function modeDisplay(appState: AppViewState): {
  className?: string;
  tone: "default" | "cued" | "warning";
  value: string;
} {
  if (appState.connection !== "connected") {
    return { tone: "default", value: "Offline" };
  }

  if (appState.lockout) {
    return { tone: "warning", value: "Safe" };
  }

  if (appState.fadeState === "running") {
    return { className: "animate-pulse", tone: "warning", value: "Fading" };
  }

  return { tone: "cued", value: "Ready" };
}

export function BottomStatusBar(props: { appState: AppViewState }) {
  const commands = useAppCommands();
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const currentScene = props.appState.currentScene
    ? props.appState.currentScene.name
    : "---";
  const mode = modeDisplay(props.appState);
  const canGo = Boolean(props.appState.cuedSceneId && commands.recallScene);

  return (
    <footer className="mx-3 mb-3 grid grid-cols-1 overflow-hidden rounded-console-panel border border-console-line bg-console-chrome md:grid-cols-[0.7fr_1.4fr_1.4fr_0.9fr_0.8fr]">
      <div className="grid min-w-0 place-items-center border-r border-console-line p-3 last:border-r-0">
        <ConsoleButton
          disabled={!canGo}
          fullWidth
          onClick={() => {
            if (props.appState.cuedSceneId) {
              commands.recallScene?.(props.appState.cuedSceneId);
            }
          }}
          size="big"
          variant="primary"
        >
          GO
        </ConsoleButton>
      </div>
      <StatusCell
        label="Cued"
        tone={props.appState.cuedSceneId ? "cued" : "default"}
        value={cuedSceneLabel(props.appState)}
      />
      <StatusCell
        label="Current"
        tone={props.appState.currentScene ? "current" : "default"}
        value={currentScene}
      />
      <StatusCell
        className={mode.className}
        label="Mode"
        tone={mode.tone}
        value={mode.value}
      />
      <StatusCell font="mono" label="Time" value={formatClock(now)} />
    </footer>
  );
}
```

- [ ] **Step 5: Update bottom status stories**

Update `ui/src/components/BottomStatusBar.stories.tsx` to include explicit safe and fading states:

```tsx
import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { BottomStatusBar } from "./BottomStatusBar";

const cuedConnectedAppState = {
  ...connectedAppState,
  cuedSceneId: connectedAppState.sceneConfigs[1]?.sceneId ?? null,
};

const safeAppState = {
  ...cuedConnectedAppState,
  lockout: true,
};

const fadingAppState = {
  ...cuedConnectedAppState,
  fadeState: "running" as const,
};

const meta: Meta<typeof BottomStatusBar> = {
  title: "Shell/BottomStatusBar",
  component: BottomStatusBar,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    appState: cuedConnectedAppState,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <BottomStatusBar {...args} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<typeof BottomStatusBar>;

export const Ready: Story = {};

export const Fading: Story = {
  args: {
    appState: fadingAppState,
  },
};

export const Safe: Story = {
  args: {
    appState: safeAppState,
  },
};

export const Offline: Story = {
  args: {
    appState: discoveringAppState,
  },
};
```

- [ ] **Step 6: Run targeted bottom status tests**

Run from `ui/`:

```bash
npm run test -- BottomStatusBar.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit Task 1**

```bash
git add ui/src/components/StatusCell.tsx ui/src/components/BottomStatusBar.tsx ui/src/components/BottomStatusBar.test.tsx ui/src/components/BottomStatusBar.stories.tsx
git commit -m "feat: clarify bottom status mode"
```

---

### Task 2: Top Bar SAFE Toggle

**Files:**
- Modify: `ui/src/components/TopTabBar.tsx`
- Modify: `ui/src/components/TopTabBar.test.tsx`
- Modify: `ui/src/components/TopTabBar.stories.tsx`

**Interfaces:**
- Consumes: `useAppState().appState.lockout`.
- Consumes: `useAppCommands().toggleLockout`.
- Produces: fixed-label `button` with accessible name `SAFE` that toggles lockout.

- [ ] **Step 1: Write failing top bar tests**

Update `ui/src/components/TopTabBar.test.tsx` to import only existing utilities and add these tests inside `describe("TopTabBar", () => { ... })`:

```tsx
  it("renders a fixed-label SAFE button", () => {
    renderTopBar(connectedAppState);

    expect(screen.getByRole("button", { name: "SAFE" })).toBeInTheDocument();
  });

  it("toggles lockout from the SAFE button", async () => {
    const user = userEvent.setup();
    const toggleLockout = vi.fn();

    renderWithAppProviders(
      <TopTabBar
        activeTab="scenes"
        onOpenConnection={vi.fn()}
        onSelectTab={vi.fn()}
      />,
      { appState: connectedAppState, commands: { toggleLockout } },
    );

    await user.click(screen.getByRole("button", { name: "SAFE" }));

    expect(toggleLockout).toHaveBeenCalledTimes(1);
  });

  it("marks the SAFE button pressed when lockout is active", () => {
    renderTopBar({ ...connectedAppState, lockout: true });

    expect(screen.getByRole("button", { name: "SAFE" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run from `ui/`:

```bash
npm run test -- TopTabBar.test.tsx
```

Expected: FAIL because `SAFE` does not exist yet.

- [ ] **Step 3: Implement the `SAFE` button**

Update `ui/src/components/TopTabBar.tsx`:

```tsx
import { useAppCommands, useAppState } from "../appHooks";
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
  const commands = useAppCommands();
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
  const safeClass = appState.lockout
    ? "border-status-warning bg-status-warning/15 text-status-warning shadow-inner shadow-status-warning/20"
    : "border-console-line bg-black/20 text-console-primary hover:border-console-line-strong";

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
      <div className="flex items-center gap-3 px-4">
        <button
          aria-pressed={appState.lockout}
          className={`rounded-console-control border px-3 py-2 font-mono text-sm font-normal uppercase ${safeClass}`}
          onClick={commands.toggleLockout}
          type="button"
        >
          SAFE
        </button>
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

- [ ] **Step 4: Update top bar stories**

Inspect `ui/src/components/TopTabBar.stories.tsx`. Add a safe-active story using `{ ...connectedAppState, lockout: true }` and `MockAppProviders`, preserving the existing story format. The story must render the same fixed label `SAFE` with active styling.

- [ ] **Step 5: Run targeted top bar tests**

Run from `ui/`:

```bash
npm run test -- TopTabBar.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit Task 2**

```bash
git add ui/src/components/TopTabBar.tsx ui/src/components/TopTabBar.test.tsx ui/src/components/TopTabBar.stories.tsx
git commit -m "feat: add safe toggle to top bar"
```

---

### Task 3: Frontend Verification

**Files:**
- Verify: `ui/src/components/BottomStatusBar.tsx`
- Verify: `ui/src/components/TopTabBar.tsx`
- Verify: `ui/src/components/StatusCell.tsx`
- Verify: related tests and stories.

**Interfaces:**
- Consumes: completed Task 1 and Task 2 behavior.
- Produces: verified frontend state ready for review.

- [ ] **Step 1: Run formatting check**

Run from `ui/`:

```bash
npm run format:check
```

Expected: PASS.

- [ ] **Step 2: Run lint**

Run from `ui/`:

```bash
npm run lint
```

Expected: PASS.

- [ ] **Step 3: Run typecheck**

Run from `ui/`:

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 4: Run unit tests**

Run from `ui/`:

```bash
npm run test
```

Expected: PASS.

- [ ] **Step 5: Run build**

Run from `ui/`:

```bash
npm run build
```

Expected: PASS.

- [ ] **Step 6: Commit verification fixes if needed**

If any verification command required code changes, commit only those changes:

```bash
git add ui/src/components/BottomStatusBar.tsx ui/src/components/BottomStatusBar.test.tsx ui/src/components/BottomStatusBar.stories.tsx ui/src/components/StatusCell.tsx ui/src/components/TopTabBar.tsx ui/src/components/TopTabBar.test.tsx ui/src/components/TopTabBar.stories.tsx
git commit -m "fix: satisfy status mode checks"
```

Expected: Skip this step if no fixes were needed.

---

## Self-Review

- Spec coverage: Task 1 covers `Cued`, `Current`, and `Mode`; Task 2 covers fixed-label `SAFE`; Task 3 covers frontend verification.
- Placeholder scan: No placeholder steps remain; each code-changing step names exact files and code or exact inspection target.
- Type consistency: `StatusCell.className?: string`, `AppViewState.fadeState`, `AppViewState.lockout`, and `toggleLockout` names match existing project types.
