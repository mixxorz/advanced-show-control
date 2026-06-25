# Keyboard Shortcut Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace manual keyboard shortcut editing with a global keyboard handler layer and intuitive shortcut capture controls in Settings.

**Architecture:** Add a focused React keyboard provider that owns the single app-wide `keydown` listener and dispatches normalized events through priority-ordered handlers. Build shortcut capture and shortcut formatting on top of that provider, then update `SettingsTab` to use one capture button per shortcut while preserving the existing settings persistence model.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, Tauri frontend command bindings.

## Global Constraints

- Preserve the existing persisted `KeyboardShortcut` shape: `{ key: string; modifiers: { shift; control; alt; meta } }`.
- Preserve the existing full-object `replaceAppSettings(settings)` replacement flow.
- Do not wire GO or Cue shortcut execution in this plan.
- Do not change Rust settings types or settings persistence.
- Do not mark show/session data dirty and do not send LV1 or fade commands.
- `Tab` must be recordable while capture mode is active.
- Capture must preempt future shortcut execution by using higher-priority keyboard handling.
- Use frontend tests for behavior changes; no Rust tests are required because this is UI-only.

---

## File Structure

- Create `ui/src/keyboard.tsx`: `KeyboardProvider`, `useKeyboardHandler`, `useShortcutCapture`, event normalization, handler priority dispatch.
- Create `ui/src/keyboard.test.tsx`: provider and hook tests for dispatch priority, capture, modifier-only handling, Escape cancel, and Tab capture.
- Create `ui/src/shortcutFormat.ts`: pure helpers for shortcut key normalization and OS-specific display formatting.
- Create `ui/src/shortcutFormat.test.ts`: pure formatter tests for macOS and non-macOS output.
- Modify `ui/src/AppRuntime.tsx`: wrap the normal app providers with `KeyboardProvider`.
- Modify `ui/src/storybook/MockAppProviders.tsx`: wrap stories and test render helpers with `KeyboardProvider`.
- Modify `ui/src/components/SettingsTab.tsx`: replace manual shortcut text inputs and modifier checkboxes with capture buttons.
- Modify `ui/src/components/SettingsTab.test.tsx`: update shortcut tests to use global keydown capture.

---

### Task 1: Global Keyboard Provider

**Files:**
- Create: `ui/src/keyboard.tsx`
- Create: `ui/src/keyboard.test.tsx`
- Modify: `ui/src/AppRuntime.tsx:298-309`
- Modify: `ui/src/storybook/MockAppProviders.tsx:16-24`

**Interfaces:**
- Produces: `KeyboardProvider(props: { children: ReactNode })`
- Produces: `useKeyboardHandler(handler: KeyboardHandler): void`
- Produces: `useShortcutCapture(): ShortcutCaptureApi`
- Produces: `type KeyboardHandler = { id: string; priority: number; enabled?: boolean; handleKeyDown: (event: AppKeyboardEvent) => "handled" | "ignored" }`
- Produces: `type AppKeyboardEvent = { key: string; modifiers: KeyboardShortcutModifiers; originalEvent: KeyboardEvent }`
- Produces: `type ShortcutCaptureApi = { activeCaptureId: string | null; startCapture: (request: ShortcutCaptureRequest) => void; cancelCapture: (id?: string) => void; isCapturing: (id: string) => boolean }`
- Consumes: `KeyboardShortcut` and `KeyboardShortcutModifiers` from `ui/src/types.ts`.

- [ ] **Step 1: Write failing provider tests**

Create `ui/src/keyboard.test.tsx` with these tests:

```tsx
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  KeyboardProvider,
  useKeyboardHandler,
  useShortcutCapture,
} from "./keyboard";

describe("KeyboardProvider", () => {
  it("dispatches enabled handlers by priority and stops after handled", () => {
    const low = vi.fn(() => "handled" as const);
    const high = vi.fn(() => "handled" as const);

    function Harness() {
      useKeyboardHandler({
        id: "low",
        priority: 10,
        handleKeyDown: low,
      });
      useKeyboardHandler({
        id: "high",
        priority: 100,
        handleKeyDown: high,
      });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("k");

    expect(high).toHaveBeenCalledTimes(1);
    expect(low).not.toHaveBeenCalled();
  });

  it("continues dispatch when a higher-priority handler ignores the event", () => {
    const low = vi.fn(() => "handled" as const);
    const high = vi.fn(() => "ignored" as const);

    function Harness() {
      useKeyboardHandler({ id: "low", priority: 10, handleKeyDown: low });
      useKeyboardHandler({ id: "high", priority: 100, handleKeyDown: high });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("k");

    expect(high).toHaveBeenCalledTimes(1);
    expect(low).toHaveBeenCalledTimes(1);
  });

  it("captures a non-modifier key with modifiers and exits capture mode", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    screen.getByRole("button").click();
    expect(screen.getByText("capturing")).toBeInTheDocument();

    fireKeyDown("Enter", { shiftKey: true });

    expect(onCapture).toHaveBeenCalledWith({
      key: "Enter",
      modifiers: { shift: true, control: false, alt: false, meta: false },
    });
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("keeps capture active for modifier-only keys", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    screen.getByRole("button").click();
    fireKeyDown("Shift", { shiftKey: true });

    expect(onCapture).not.toHaveBeenCalled();
    expect(screen.getByText("capturing")).toBeInTheDocument();
  });

  it("cancels capture on Escape", () => {
    const onCapture = vi.fn();
    const onCancel = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() =>
            capture.startCapture({ id: "go", onCapture, onCancel })
          }
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    screen.getByRole("button").click();
    fireKeyDown("Escape");

    expect(onCapture).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("captures Tab while capture mode is active", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          capture
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    screen.getByRole("button").click();
    fireKeyDown("Tab");

    expect(onCapture).toHaveBeenCalledWith({
      key: "Tab",
      modifiers: { shift: false, control: false, alt: false, meta: false },
    });
  });
});

function fireKeyDown(key: string, init: KeyboardEventInit = {}) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key,
        bubbles: true,
        cancelable: true,
        ...init,
      }),
    );
  });
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui run test -- keyboard`

Expected: FAIL because `./keyboard` does not exist.

- [ ] **Step 3: Implement keyboard provider and hooks**

Create `ui/src/keyboard.tsx`:

```tsx
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { KeyboardShortcut, KeyboardShortcutModifiers } from "./types";

export type AppKeyboardEvent = {
  key: string;
  modifiers: KeyboardShortcutModifiers;
  originalEvent: KeyboardEvent;
};

export type KeyboardHandler = {
  id: string;
  priority: number;
  enabled?: boolean;
  handleKeyDown: (event: AppKeyboardEvent) => "handled" | "ignored";
};

export type ShortcutCaptureRequest = {
  id: string;
  onCapture: (shortcut: KeyboardShortcut) => void;
  onCancel?: () => void;
};

export type ShortcutCaptureApi = {
  activeCaptureId: string | null;
  startCapture: (request: ShortcutCaptureRequest) => void;
  cancelCapture: (id?: string) => void;
  isCapturing: (id: string) => boolean;
};

type KeyboardContextValue = {
  registerHandler: (handler: KeyboardHandler) => () => void;
  shortcutCapture: ShortcutCaptureApi;
};

const KeyboardContext = createContext<KeyboardContextValue | null>(null);
const CAPTURE_HANDLER_ID = "shortcut-capture";
const CAPTURE_PRIORITY = 1000;

export function KeyboardProvider(props: { children: ReactNode }) {
  const handlers = useRef(new Map<string, KeyboardHandler>());
  const activeCapture = useRef<ShortcutCaptureRequest | null>(null);
  const [activeCaptureId, setActiveCaptureId] = useState<string | null>(null);

  const registerHandler = useCallback((handler: KeyboardHandler) => {
    handlers.current.set(handler.id, handler);
    return () => {
      const current = handlers.current.get(handler.id);
      if (current === handler) {
        handlers.current.delete(handler.id);
      }
    };
  }, []);

  const clearCapture = useCallback(() => {
    activeCapture.current = null;
    setActiveCaptureId(null);
  }, []);

  const cancelCapture = useCallback(
    (id?: string) => {
      const current = activeCapture.current;
      if (!current || (id && current.id !== id)) return;
      clearCapture();
      current.onCancel?.();
    },
    [clearCapture],
  );

  const startCapture = useCallback((request: ShortcutCaptureRequest) => {
    activeCapture.current = request;
    setActiveCaptureId(request.id);
  }, []);

  useEffect(() => {
    return registerHandler({
      id: CAPTURE_HANDLER_ID,
      priority: CAPTURE_PRIORITY,
      handleKeyDown(event) {
        const current = activeCapture.current;
        if (!current) return "ignored";
        if (event.key === "Escape") {
          cancelCapture(current.id);
          return "handled";
        }
        if (isModifierKey(event.key)) {
          return "handled";
        }

        clearCapture();
        current.onCapture({ key: normalizeCapturedKey(event.key), modifiers: event.modifiers });
        return "handled";
      },
    });
  }, [cancelCapture, clearCapture, registerHandler]);

  useEffect(() => {
    function handleKeyDown(originalEvent: KeyboardEvent) {
      const appEvent = normalizeKeyboardEvent(originalEvent);
      const sortedHandlers = [...handlers.current.values()]
        .filter((handler) => handler.enabled !== false)
        .sort((a, b) => b.priority - a.priority);

      for (const handler of sortedHandlers) {
        if (handler.handleKeyDown(appEvent) === "handled") {
          originalEvent.preventDefault();
          originalEvent.stopPropagation();
          break;
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const shortcutCapture = useMemo<ShortcutCaptureApi>(
    () => ({
      activeCaptureId,
      startCapture,
      cancelCapture,
      isCapturing: (id) => activeCaptureId === id,
    }),
    [activeCaptureId, cancelCapture, startCapture],
  );

  const value = useMemo(
    () => ({ registerHandler, shortcutCapture }),
    [registerHandler, shortcutCapture],
  );

  return (
    <KeyboardContext.Provider value={value}>
      {props.children}
    </KeyboardContext.Provider>
  );
}

export function useKeyboardHandler(handler: KeyboardHandler) {
  const context = useKeyboardContext();
  useEffect(() => context.registerHandler(handler), [context, handler]);
}

export function useShortcutCapture() {
  return useKeyboardContext().shortcutCapture;
}

function useKeyboardContext() {
  const context = useContext(KeyboardContext);
  if (!context) {
    throw new Error("KeyboardProvider is missing");
  }
  return context;
}

function normalizeKeyboardEvent(event: KeyboardEvent): AppKeyboardEvent {
  return {
    key: event.key,
    modifiers: {
      shift: event.shiftKey,
      control: event.ctrlKey,
      alt: event.altKey,
      meta: event.metaKey,
    },
    originalEvent: event,
  };
}

function normalizeCapturedKey(key: string) {
  return key.length === 1 ? key.toUpperCase() : key;
}

function isModifierKey(key: string) {
  return key === "Shift" || key === "Control" || key === "Alt" || key === "Meta";
}
```

- [ ] **Step 4: Wrap runtime and mock providers**

Modify `ui/src/AppRuntime.tsx` imports:

```tsx
import { KeyboardProvider } from "./keyboard";
```

Replace the return block at lines 298-309 with:

```tsx
  return (
    <KeyboardProvider>
      <AppStateProvider appState={appState} commandError={commandError}>
        <AppCommandsProvider commands={commands}>
          <AppShell
            activeTab={activeTab}
            onOpenConnection={() => setConnectionModalMode("manual")}
            onResume={() => setConnectionModalMode(null)}
            onSelectTab={setActiveTab}
            showConnection={showConnection}
          />
        </AppCommandsProvider>
      </AppStateProvider>
    </KeyboardProvider>
  );
```

Modify `ui/src/storybook/MockAppProviders.tsx` imports:

```tsx
import { KeyboardProvider } from "../keyboard";
```

Replace its return block with:

```tsx
  return (
    <KeyboardProvider>
      <AppStateProvider
        appState={props.appState ?? disconnectedAppViewState}
        commandError={props.commandError ?? null}
      >
        <AppCommandsProvider commands={{ ...mockAppCommands, ...props.commands }}>
          {props.children}
        </AppCommandsProvider>
      </AppStateProvider>
    </KeyboardProvider>
  );
```

- [ ] **Step 5: Run tests to verify provider passes**

Run: `npm --prefix ui run test -- keyboard`

Expected: PASS for `keyboard.test.tsx`.

- [ ] **Step 6: Commit provider foundation**

Run:

```bash
git add ui/src/keyboard.tsx ui/src/keyboard.test.tsx ui/src/AppRuntime.tsx ui/src/storybook/MockAppProviders.tsx
git commit -m "feat: add frontend keyboard provider"
```

---

### Task 2: Shortcut Formatting Helpers

**Files:**
- Create: `ui/src/shortcutFormat.ts`
- Create: `ui/src/shortcutFormat.test.ts`

**Interfaces:**
- Produces: `type ShortcutPlatform = "mac" | "other"`
- Produces: `detectShortcutPlatform(): ShortcutPlatform`
- Produces: `formatShortcut(shortcut: KeyboardShortcut, platform?: ShortcutPlatform): string`
- Consumes: `KeyboardShortcut` from `ui/src/types.ts`.

- [ ] **Step 1: Write failing formatter tests**

Create `ui/src/shortcutFormat.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { formatShortcut } from "./shortcutFormat";
import type { KeyboardShortcut } from "./types";

describe("formatShortcut", () => {
  it("uses macOS symbols", () => {
    expect(formatShortcut(shortcut("c", { meta: true }), "mac")).toBe("⌘C");
    expect(
      formatShortcut(shortcut("Enter", { shift: true, alt: true }), "mac"),
    ).toBe("⇧⌥Enter");
  });

  it("uses non-macOS labels", () => {
    expect(formatShortcut(shortcut("c", { control: true }), "other")).toBe(
      "Ctrl + C",
    );
    expect(formatShortcut(shortcut("Tab", { meta: true }), "other")).toBe(
      "Win + Tab",
    );
  });

  it("formats common key labels", () => {
    expect(formatShortcut(shortcut(" ", {}), "other")).toBe("Space");
    expect(formatShortcut(shortcut("ArrowRight", {}), "other")).toBe("Right");
  });
});

function shortcut(
  key: string,
  modifiers: Partial<KeyboardShortcut["modifiers"]>,
): KeyboardShortcut {
  return {
    key,
    modifiers: {
      shift: false,
      control: false,
      alt: false,
      meta: false,
      ...modifiers,
    },
  };
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui run test -- shortcutFormat`

Expected: FAIL because `./shortcutFormat` does not exist.

- [ ] **Step 3: Implement shortcut formatting**

Create `ui/src/shortcutFormat.ts`:

```ts
import type { KeyboardShortcut } from "./types";

export type ShortcutPlatform = "mac" | "other";

export function detectShortcutPlatform(): ShortcutPlatform {
  const userAgentData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const platform = userAgentData.userAgentData?.platform ?? navigator.platform;
  return /mac|iphone|ipad|ipod/i.test(platform) ? "mac" : "other";
}

export function formatShortcut(
  shortcut: KeyboardShortcut,
  platform: ShortcutPlatform = detectShortcutPlatform(),
) {
  const modifiers = shortcut.modifiers;
  const parts = platform === "mac" ? macModifierParts(modifiers) : otherModifierParts(modifiers);
  parts.push(formatKey(shortcut.key));
  return platform === "mac" ? parts.join("") : parts.join(" + ");
}

function macModifierParts(modifiers: KeyboardShortcut["modifiers"]) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("⇧");
  if (modifiers.control) parts.push("⌃");
  if (modifiers.alt) parts.push("⌥");
  if (modifiers.meta) parts.push("⌘");
  return parts;
}

function otherModifierParts(modifiers: KeyboardShortcut["modifiers"]) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("Shift");
  if (modifiers.control) parts.push("Ctrl");
  if (modifiers.alt) parts.push("Alt");
  if (modifiers.meta) parts.push("Win");
  return parts;
}

function formatKey(key: string) {
  if (key === " " || key === "Spacebar") return "Space";
  if (key.length === 1) return key.toUpperCase();
  if (key === "ArrowUp") return "Up";
  if (key === "ArrowDown") return "Down";
  if (key === "ArrowLeft") return "Left";
  if (key === "ArrowRight") return "Right";
  return key;
}
```

- [ ] **Step 4: Run formatter tests**

Run: `npm --prefix ui run test -- shortcutFormat`

Expected: PASS.

- [ ] **Step 5: Commit formatter helpers**

Run:

```bash
git add ui/src/shortcutFormat.ts ui/src/shortcutFormat.test.ts
git commit -m "feat: format keyboard shortcuts"
```

---

### Task 3: Settings Shortcut Capture UI

**Files:**
- Modify: `ui/src/components/SettingsTab.tsx:96-203`
- Modify: `ui/src/components/SettingsTab.test.tsx:112-198`

**Interfaces:**
- Consumes: `useShortcutCapture()` from `ui/src/keyboard.tsx`.
- Consumes: `formatShortcut()` from `ui/src/shortcutFormat.ts`.
- Produces: `ShortcutCaptureButton` internal component in `SettingsTab.tsx`.

- [ ] **Step 1: Replace shortcut tests with capture tests**

In `ui/src/components/SettingsTab.test.tsx`, replace the tests from `"updates the GO shortcut key while replacing the full settings object"` through `"updates the Cue shortcut modifier while replacing the full settings object"` with:

```tsx
  it("captures the GO shortcut while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
    expect(screen.getByText("Press shortcut...")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Enter", shiftKey: true });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          key: "Enter",
          modifiers: {
            shift: true,
            control: false,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });

  it("captures the Cue shortcut while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByRole("button", { name: "Change Cue keyboard shortcut" }));
    fireEvent.keyDown(window, { key: "q", ctrlKey: true });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        cue: {
          key: "Q",
          modifiers: {
            shift: false,
            control: true,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });

  it("does not save a shortcut for modifier-only keydown", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
    fireEvent.keyDown(window, { key: "Shift", shiftKey: true });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(screen.getByText("Press shortcut...")).toBeInTheDocument();
  });

  it("cancels shortcut capture on Escape", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
    fireEvent.keyDown(window, { key: "Escape" });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(screen.queryByText("Press shortcut...")).not.toBeInTheDocument();
  });

  it("captures Tab as a shortcut", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
    fireEvent.keyDown(window, { key: "Tab" });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          key: "Tab",
          modifiers: {
            shift: false,
            control: false,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });
```

- [ ] **Step 2: Run Settings tests to verify they fail**

Run: `npm --prefix ui run test -- SettingsTab`

Expected: FAIL because `SettingsTab` still renders text inputs and modifier checkboxes.

- [ ] **Step 3: Update SettingsTab shortcut UI**

Modify `ui/src/components/SettingsTab.tsx` imports:

```tsx
import { useShortcutCapture } from "../keyboard";
import { formatShortcut } from "../shortcutFormat";
```

Inside `SettingsTab`, after `const settings = appState.settings;`, add:

```tsx
  const shortcutCapture = useShortcutCapture();
```

Replace the shortcut panel at lines 96-127 with:

```tsx
      <Panel className="grid gap-4 p-4">
        <ShortcutCaptureButton
          id="go"
          label="GO keyboard shortcut"
          shortcut={settings.keyboardShortcuts.go}
          isCapturing={shortcutCapture.isCapturing("go")}
          onStartCapture={() =>
            shortcutCapture.startCapture({
              id: "go",
              onCapture: (shortcut) => updateShortcut("go", shortcut),
            })
          }
        />
        <ShortcutCaptureButton
          id="cue"
          label="Cue keyboard shortcut"
          shortcut={settings.keyboardShortcuts.cue}
          isCapturing={shortcutCapture.isCapturing("cue")}
          onStartCapture={() =>
            shortcutCapture.startCapture({
              id: "cue",
              onCapture: (shortcut) => updateShortcut("cue", shortcut),
            })
          }
        />
      </Panel>
```

Delete the existing `ShortcutInput`, `ShortcutModifierControls`, and `capitalize` functions. Add this component at the bottom of the file:

```tsx
function ShortcutCaptureButton(props: {
  id: "go" | "cue";
  label: string;
  shortcut: KeyboardShortcut;
  isCapturing: boolean;
  onStartCapture: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span className="text-console-muted">{props.label}</span>
      <button
        aria-label={`Change ${props.label}`}
        className="min-w-36 rounded-console-button border border-console-line bg-console-surface px-3 py-2 text-left font-mono text-console-primary transition hover:border-console-accent focus:outline-none focus:ring-2 focus:ring-console-accent"
        type="button"
        onClick={props.onStartCapture}
      >
        {props.isCapturing ? "Press shortcut..." : formatShortcut(props.shortcut)}
      </button>
    </div>
  );
}
```

- [ ] **Step 4: Run Settings tests**

Run: `npm --prefix ui run test -- SettingsTab`

Expected: PASS.

- [ ] **Step 5: Run frontend typecheck and tests**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

Run: `npm --prefix ui run test`

Expected: PASS.

- [ ] **Step 6: Commit Settings capture UI**

Run:

```bash
git add ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx
git commit -m "feat: capture keyboard shortcuts in settings"
```

---

### Task 4: Final Verification

**Files:**
- Modify only if verification exposes failures.

**Interfaces:**
- Consumes all prior task outputs.
- Produces a verified feature branch ready for merge or PR.

- [ ] **Step 1: Run format check**

Run: `npm --prefix ui run format:check`

Expected: PASS. If it fails only for files changed by this plan, run `npm --prefix ui exec prettier --write ui/src/keyboard.tsx ui/src/keyboard.test.tsx ui/src/shortcutFormat.ts ui/src/shortcutFormat.test.ts ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx`, inspect the diff, and commit formatting with the relevant task commit if possible.

- [ ] **Step 2: Run lint**

Run: `npm --prefix ui run lint`

Expected: PASS.

- [ ] **Step 3: Run typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 4: Run UI unit tests**

Run: `npm --prefix ui run test`

Expected: PASS.

- [ ] **Step 5: Run Storybook tests**

Run: `npm --prefix ui run test:storybook`

Expected: PASS.

- [ ] **Step 6: Commit any verification fixes**

If Step 1-5 required fixes, run:

```bash
git status --short
git diff
git add <only files changed by this plan>
git commit -m "fix: stabilize keyboard shortcut capture"
```

If no fixes were needed, do not create an empty commit.

- [ ] **Step 7: Report branch state**

Run: `git status --short && git log --oneline -8`

Expected: no dirty files except unrelated user changes, and recent commits include the keyboard provider, formatter, and settings capture UI commits.
