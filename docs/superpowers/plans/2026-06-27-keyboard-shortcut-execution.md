# Keyboard Shortcut Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make native file shortcuts, GO/Cue shortcuts, and shortcut conflict detection work in the app.

**Architecture:** Fixed file-session shortcuts are native Tauri menu accelerators. User-configurable GO/Cue shortcuts execute in the React keyboard layer using existing projected settings and `AppCommands`. Settings conflict detection stays in React and validates captured shortcuts before calling `replaceAppSettings`.

**Tech Stack:** Tauri 2 / Rust, React / TypeScript, Vitest, Testing Library, Cargo nextest.

## Global Constraints

- Implement in this order: native File menu shortcuts, then GO/Cue shortcut execution, then conflict checking.
- Preserve the existing persisted settings shape and full-object settings replacement command.
- Shortcut capture must keep higher priority than execution.
- Shortcut execution must reuse existing app commands and menu handlers instead of introducing parallel command paths.
- Do not add global OS-level shortcuts that fire while the app is unfocused.
- React conflict checking must compare against `GO`, `Cue`, `CmdOrCtrl+N`, `CmdOrCtrl+O`, `CmdOrCtrl+S`, and `CmdOrCtrl+Shift+S`.
- `CmdOrCtrl` means `meta: true` on macOS and `control: true` on non-macOS platforms, with the other command modifier false.
- Do not bypass lockout, exact scene identity validation, stale-state checks, generation guards, or backend command validation.

---

## File Structure

- Modify `src-tauri/src/ui/menu.rs`: add named accelerator constants, use them in File menu items, and test stable accelerator values.
- Modify `ui/src/keyboard.tsx`: export comparable key normalization and shortcut matching helpers so capture, execution, and conflict checks share one definition.
- Modify `ui/src/keyboard.test.tsx`: cover key normalization and shortcut matching helpers.
- Modify `ui/src/AppRuntime.tsx`: register the focused-window GO/Cue execution handler near the runtime where `appState` and `commands` are available.
- Modify `ui/src/AppRuntime.test.tsx`: cover GO/Cue shortcut execution, ignored unavailable actions, capture preemption, and GO precedence.
- Modify `ui/src/components/KeyboardShortcutInput.tsx`: optionally render red inline conflict text beside the shortcut button.
- Modify `ui/src/components/SettingsTab.tsx`: detect configurable and fixed-file shortcut conflicts before replacing settings.
- Modify `ui/src/components/SettingsTab.test.tsx`: cover duplicate and fixed-file shortcut conflict behavior.

---

### Task 1: Native File Menu Accelerators

**Files:**
- Modify: `src-tauri/src/ui/menu.rs`

**Interfaces:**
- Produces Rust constants used by tests and menu construction:
- `MENU_NEW_SESSION_ACCELERATOR: &str = "CmdOrCtrl+N"`
- `MENU_OPEN_SESSION_ACCELERATOR: &str = "CmdOrCtrl+O"`
- `MENU_SAVE_SESSION_ACCELERATOR: &str = "CmdOrCtrl+S"`
- `MENU_SAVE_SESSION_AS_ACCELERATOR: &str = "CmdOrCtrl+Shift+S"`

- [ ] **Step 1: Write the failing Rust unit test**

Add this test next to `menu_ids_are_stable` in `src-tauri/src/ui/menu.rs`:

```rust
#[test]
fn file_menu_accelerators_are_standard() {
    assert_eq!(MENU_NEW_SESSION_ACCELERATOR, "CmdOrCtrl+N");
    assert_eq!(MENU_OPEN_SESSION_ACCELERATOR, "CmdOrCtrl+O");
    assert_eq!(MENU_SAVE_SESSION_ACCELERATOR, "CmdOrCtrl+S");
    assert_eq!(MENU_SAVE_SESSION_AS_ACCELERATOR, "CmdOrCtrl+Shift+S");
}
```

- [ ] **Step 2: Run the targeted test and verify it fails**

Run: `cargo nextest run -p advanced-show-control ui::menu::tests::file_menu_accelerators_are_standard`

Expected: FAIL because the `MENU_*_ACCELERATOR` constants do not exist.

- [ ] **Step 3: Add accelerator constants**

In `src-tauri/src/ui/menu.rs`, add these constants after the existing menu id constants:

```rust
pub const MENU_NEW_SESSION_ACCELERATOR: &str = "CmdOrCtrl+N";
pub const MENU_OPEN_SESSION_ACCELERATOR: &str = "CmdOrCtrl+O";
pub const MENU_SAVE_SESSION_ACCELERATOR: &str = "CmdOrCtrl+S";
pub const MENU_SAVE_SESSION_AS_ACCELERATOR: &str = "CmdOrCtrl+Shift+S";
```

- [ ] **Step 4: Wire constants into menu items**

Replace the four `None::<&str>` accelerator arguments in `install_session_menu` with these values:

```rust
&MenuItem::with_id(
    handle,
    MENU_NEW_SESSION,
    "New Session",
    true,
    Some(MENU_NEW_SESSION_ACCELERATOR),
)?
```

```rust
&MenuItem::with_id(
    handle,
    MENU_OPEN_SESSION,
    "Open Session...",
    true,
    Some(MENU_OPEN_SESSION_ACCELERATOR),
)?
```

```rust
&MenuItem::with_id(
    handle,
    MENU_SAVE_SESSION,
    "Save Session",
    true,
    Some(MENU_SAVE_SESSION_ACCELERATOR),
)?
```

```rust
&MenuItem::with_id(
    handle,
    MENU_SAVE_SESSION_AS,
    "Save As...",
    true,
    Some(MENU_SAVE_SESSION_AS_ACCELERATOR),
)?
```

- [ ] **Step 5: Run targeted Rust verification**

Run: `cargo nextest run -p advanced-show-control ui::menu::tests`

Expected: PASS for `menu_ids_are_stable` and `file_menu_accelerators_are_standard`.

- [ ] **Step 6: Commit Task 1**

Run:

```bash
git status --short
git diff -- src-tauri/src/ui/menu.rs
git add src-tauri/src/ui/menu.rs
git commit -m "feat: add file menu accelerators"
```

---

### Task 2: GO And Cue Shortcut Execution

**Files:**
- Modify: `ui/src/keyboard.tsx`
- Modify: `ui/src/keyboard.test.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`

**Interfaces:**
- Consumes `KeyboardProvider`, `useKeyboardHandler`, `AppViewState`, and `AppCommands`.
- Produces:
- `shortcutKeyFromEvent(event: AppKeyboardEvent): string`
- `shortcutMatchesEvent(shortcut: KeyboardShortcut, event: AppKeyboardEvent): boolean`

- [ ] **Step 1: Add failing keyboard helper tests**

Append these tests to `ui/src/keyboard.test.tsx` inside `describe("KeyboardProvider", ...)`:

```tsx
it("normalizes comparable shortcut keys from keydown events", () => {
  const seen: string[] = [];

  function Harness() {
    useKeyboardHandler({
      id: "recorder",
      priority: 1,
      handleKeyDown: (event) => {
        seen.push(shortcutKeyFromEvent(event));
        return "handled";
      },
    });
    return null;
  }

  render(
    <KeyboardProvider>
      <Harness />
    </KeyboardProvider>,
  );

  fireKeyDown(" ", { code: "Space" });
  fireKeyDown("q", { code: "KeyQ" });
  fireKeyDown("@", { code: "Digit2", shiftKey: true });

  expect(seen).toEqual(["Space", "Q", "2"]);
});

it("matches shortcuts by comparable key and modifiers", () => {
  const matches: boolean[] = [];

  function Harness() {
    useKeyboardHandler({
      id: "matcher",
      priority: 1,
      handleKeyDown: (event) => {
        matches.push(
          shortcutMatchesEvent(
            {
              key: "S",
              modifiers: {
                shift: true,
                control: true,
                alt: false,
                meta: false,
              },
            },
            event,
          ),
        );
        return "handled";
      },
    });
    return null;
  }

  render(
    <KeyboardProvider>
      <Harness />
    </KeyboardProvider>,
  );

  fireKeyDown("S", { code: "KeyS", shiftKey: true, ctrlKey: true });
  fireKeyDown("s", { code: "KeyS", ctrlKey: true });

  expect(matches).toEqual([true, false]);
});
```

Also update the import at the top:

```tsx
import {
  KeyboardProvider,
  shortcutKeyFromEvent,
  shortcutMatchesEvent,
  useKeyboardHandler,
  useShortcutCapture,
} from "./keyboard";
```

- [ ] **Step 2: Run helper tests and verify they fail**

Run: `npm --prefix ui run test -- keyboard.test.tsx`

Expected: FAIL because `shortcutKeyFromEvent` and `shortcutMatchesEvent` are not exported.

- [ ] **Step 3: Export shared matching helpers**

In `ui/src/keyboard.tsx`, replace `normalizeCapturedKey` use with the new exported helper:

```tsx
export function shortcutKeyFromEvent(event: AppKeyboardEvent) {
  const key = keyFromCode(event.code) ?? printableCodeFallback(event) ?? event.key;
  return key.length === 1 ? key.toUpperCase() : key;
}

export function shortcutMatchesEvent(
  shortcut: KeyboardShortcut,
  event: AppKeyboardEvent,
) {
  return (
    shortcut.key === shortcutKeyFromEvent(event) &&
    shortcut.modifiers.shift === event.modifiers.shift &&
    shortcut.modifiers.control === event.modifiers.control &&
    shortcut.modifiers.alt === event.modifiers.alt &&
    shortcut.modifiers.meta === event.modifiers.meta
  );
}
```

Then update the capture handler to call `shortcutKeyFromEvent(event)`:

```tsx
current.onCapture({
  key: shortcutKeyFromEvent(event),
  modifiers: event.modifiers,
});
```

Remove the old `normalizeCapturedKey` function after the tests pass.

- [ ] **Step 4: Run helper tests and verify they pass**

Run: `npm --prefix ui run test -- keyboard.test.tsx`

Expected: PASS.

- [ ] **Step 5: Add failing AppRuntime shortcut execution tests**

Append these tests to `ui/src/AppRuntime.test.tsx` inside the existing `describe("AppRuntime connection lifecycle", ...)`:

```tsx
it("recalls the cued scene when the configured GO shortcut is pressed", async () => {
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });
  const scene = connectedAppState.sceneConfigs[0];

  render(<AppRuntime services={services} />);

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true, cancelable: true }),
    );
  });

  expect(services.recallScene).toHaveBeenCalledWith(scene.internalSceneId);
});

it("does not run GO when no scene is cued", async () => {
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });
  const appState = {
    ...connectedAppState,
    cuedSceneInternalId: null,
    stateVersion: connectedAppState.stateVersion + 1,
  };

  render(
    <AppRuntime
      services={makeServices({
        ...services,
        listenForAppStatus: vi.fn(async (listener) => {
          listener(appState);
          return () => {};
        }),
      })}
    />,
  );

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true, cancelable: true }),
    );
  });

  expect(services.recallScene).not.toHaveBeenCalled();
});

it("cues the selected linked scene when the configured Cue shortcut is pressed", async () => {
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });
  const scene = connectedAppState.sceneConfigs[0];

  render(<AppRuntime services={services} />);

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "c", code: "KeyC", bubbles: true, cancelable: true }),
    );
  });

  expect(services.cueScene).toHaveBeenCalledWith(scene.internalSceneId);
});

it("does not run Cue for an unlinked selected scene", async () => {
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });
  const unlinked = { ...connectedAppState.sceneConfigs[0], sceneIndex: null };
  const appState = {
    ...connectedAppState,
    sceneConfigs: [unlinked],
    selectedSceneInternalId: unlinked.internalSceneId,
    stateVersion: connectedAppState.stateVersion + 1,
  };

  render(
    <AppRuntime
      services={makeServices({
        ...services,
        listenForAppStatus: vi.fn(async (listener) => {
          listener(appState);
          return () => {};
        }),
      })}
    />,
  );

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "c", code: "KeyC", bubbles: true, cancelable: true }),
    );
  });

  expect(services.cueScene).not.toHaveBeenCalled();
});
```

- [ ] **Step 6: Run AppRuntime tests and verify they fail**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx`

Expected: FAIL because shortcuts do not execute yet.

- [ ] **Step 7: Implement GO/Cue execution handler**

In `ui/src/AppRuntime.tsx`, update the keyboard import:

```tsx
import {
  KeyboardProvider,
  shortcutMatchesEvent,
  useKeyboardHandler,
} from "./keyboard";
```

Add this component above `type ConnectionModalMode`:

```tsx
const SHORTCUT_EXECUTION_PRIORITY = 100;

function AppShortcutHandler(props: {
  appState: AppViewState;
  commands: AppCommands;
}) {
  useKeyboardHandler({
    id: "app-shortcut-execution",
    priority: SHORTCUT_EXECUTION_PRIORITY,
    handleKeyDown: (event) => {
      if (shortcutMatchesEvent(props.appState.settings.keyboardShortcuts.go, event)) {
        if (!props.appState.cuedSceneInternalId || !props.commands.recallScene) {
          return "ignored";
        }
        props.commands.recallScene(props.appState.cuedSceneInternalId);
        return "handled";
      }

      if (shortcutMatchesEvent(props.appState.settings.keyboardShortcuts.cue, event)) {
        const selected = props.appState.sceneConfigs.find(
          (scene) => scene.internalSceneId === props.appState.selectedSceneInternalId,
        );
        if (!selected || selected.sceneIndex === null || !props.commands.cueScene) {
          return "ignored";
        }
        props.commands.cueScene(selected.internalSceneId);
        return "handled";
      }

      return "ignored";
    },
  });

  return null;
}
```

Then render it inside `AppCommandsProvider` before `AppShell`:

```tsx
<AppShortcutHandler appState={appState} commands={commands} />
```

- [ ] **Step 8: Add capture preemption and GO precedence tests**

Append these tests to `ui/src/AppRuntime.test.tsx`:

```tsx
it("does not execute shortcuts while shortcut capture is active", async () => {
  const user = userEvent.setup();
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });

  render(<AppRuntime services={services} />);

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  await user.click(screen.getByRole("tab", { name: "Settings" }));
  await user.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true, cancelable: true }),
    );
  });

  expect(services.recallScene).not.toHaveBeenCalled();
});

it("gives GO precedence when GO and Cue are configured to the same shortcut", async () => {
  const services = makeServices({ startupAutoConnectLv1: vi.fn(async () => undefined) });
  const scene = connectedAppState.sceneConfigs[0];
  const appState = {
    ...connectedAppState,
    settings: {
      ...connectedAppState.settings,
      keyboardShortcuts: {
        go: { key: "C", modifiers: { shift: false, control: false, alt: false, meta: false } },
        cue: { key: "C", modifiers: { shift: false, control: false, alt: false, meta: false } },
      },
    },
    stateVersion: connectedAppState.stateVersion + 1,
  };

  render(
    <AppRuntime
      services={makeServices({
        ...services,
        listenForAppStatus: vi.fn(async (listener) => {
          listener(appState);
          return () => {};
        }),
      })}
    />,
  );

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "c", code: "KeyC", bubbles: true, cancelable: true }),
    );
  });

  expect(services.recallScene).toHaveBeenCalledWith(scene.internalSceneId);
  expect(services.cueScene).not.toHaveBeenCalled();
});
```

- [ ] **Step 9: Run frontend shortcut tests**

Run: `npm --prefix ui run test -- keyboard.test.tsx AppRuntime.test.tsx`

Expected: PASS.

- [ ] **Step 10: Run frontend typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 11: Commit Task 2**

Run:

```bash
git status --short
git diff -- ui/src/keyboard.tsx ui/src/keyboard.test.tsx ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx
git add ui/src/keyboard.tsx ui/src/keyboard.test.tsx ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx
git commit -m "feat: execute go and cue shortcuts"
```

---

### Task 3: Shortcut Conflict Checking

**Files:**
- Modify: `ui/src/components/KeyboardShortcutInput.tsx`
- Modify: `ui/src/components/SettingsTab.tsx`
- Modify: `ui/src/components/SettingsTab.test.tsx`

**Interfaces:**
- Consumes `KeyboardShortcut`, `AppSettings`, and shared shortcut equality semantics from Task 2.
- Produces `KeyboardShortcutInput` prop `conflictMessage?: string`.

- [ ] **Step 1: Add failing Settings conflict tests**

Append these tests to `ui/src/components/SettingsTab.test.tsx`:

```tsx
it("rejects a captured shortcut already assigned to the other configurable action", () => {
  renderWithAppProviders(<SettingsTab />, {
    appState: disconnectedAppViewState,
  });

  fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
  fireEvent.keyDown(window, { key: "c", code: "KeyC" });

  expect(replaceAppSettings).not.toHaveBeenCalled();
  expect(screen.getByText("Already assigned to Cue")).toBeInTheDocument();
});

it("rejects a captured shortcut reserved by a fixed File menu accelerator", () => {
  renderWithAppProviders(<SettingsTab />, {
    appState: disconnectedAppViewState,
  });

  fireEvent.click(screen.getByRole("button", { name: "Change Cue keyboard shortcut" }));
  fireEvent.keyDown(window, { key: "s", code: "KeyS", metaKey: true });

  expect(replaceAppSettings).not.toHaveBeenCalled();
  expect(screen.getByText("Already assigned to Save Session")).toBeInTheDocument();
});

it("clears shortcut conflict text after a successful non-conflicting capture", () => {
  renderWithAppProviders(<SettingsTab />, {
    appState: disconnectedAppViewState,
  });

  fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
  fireEvent.keyDown(window, { key: "c", code: "KeyC" });
  expect(screen.getByText("Already assigned to Cue")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Change GO keyboard shortcut" }));
  fireEvent.keyDown(window, { key: "Enter", code: "Enter", shiftKey: true });

  expect(screen.queryByText("Already assigned to Cue")).not.toBeInTheDocument();
  expect(replaceAppSettings).toHaveBeenCalledWith({
    ...disconnectedAppViewState.settings,
    keyboardShortcuts: {
      ...disconnectedAppViewState.settings.keyboardShortcuts,
      go: {
        key: "Enter",
        modifiers: { shift: true, control: false, alt: false, meta: false },
      },
    },
  });
});
```

- [ ] **Step 2: Run Settings tests and verify they fail**

Run: `npm --prefix ui run test -- SettingsTab.test.tsx`

Expected: FAIL because duplicate captures currently call `replaceAppSettings` and no conflict text renders.

- [ ] **Step 3: Add conflict message rendering to KeyboardShortcutInput**

In `ui/src/components/KeyboardShortcutInput.tsx`, add the prop:

```tsx
conflictMessage?: string;
```

Replace the return body with:

```tsx
return (
  <div className="flex items-center gap-3">
    <button
      aria-label={`Change ${props.label}`}
      className={`${settingControlSize} ${settingControlText} truncate rounded-console-control border px-3 py-1.5 text-center outline-none transition-colors hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white focus:border-console-line-strong ${
        props.isCapturing
          ? "!border-status-warning bg-accent-orange-soft text-status-warning shadow-[0_0_0_1px_rgba(240,180,41,0.12)]"
          : "border-console-line bg-console-panel text-accent-orange"
      }`}
      title={displayValue}
      type="button"
      onClick={props.onStartCapture}
    >
      {displayValue}
    </button>
    {props.conflictMessage ? (
      <span className="text-sm text-status-danger">{props.conflictMessage}</span>
    ) : null}
  </div>
);
```

- [ ] **Step 4: Add Settings conflict logic**

In `ui/src/components/SettingsTab.tsx`, add this state near `settingsError`:

```tsx
const [shortcutConflict, setShortcutConflict] = useState<{
  action: "go" | "cue";
  message: string;
} | null>(null);
```

Replace `updateShortcut` with:

```tsx
function updateShortcut(action: "go" | "cue", shortcut: KeyboardShortcut) {
  const conflict = shortcutConflictLabel(action, shortcut, settings);
  if (conflict) {
    setShortcutConflict({ action, message: `Already assigned to ${conflict}` });
    return;
  }

  setShortcutConflict(null);
  update((current) => ({
    ...current,
    keyboardShortcuts: {
      ...current.keyboardShortcuts,
      [action]: shortcut,
    },
  }));
}
```

Add `setShortcutConflict(null);` before each `shortcutCapture.startCapture` call for GO and Cue.

Pass conflict messages into inputs:

```tsx
conflictMessage={shortcutConflict?.action === "go" ? shortcutConflict.message : undefined}
```

```tsx
conflictMessage={shortcutConflict?.action === "cue" ? shortcutConflict.message : undefined}
```

Add these helper functions below `settingsEqual`:

```tsx
function shortcutConflictLabel(
  action: "go" | "cue",
  shortcut: KeyboardShortcut,
  settings: AppSettings,
) {
  const otherAction = action === "go" ? "cue" : "go";
  if (shortcutsEqual(shortcut, settings.keyboardShortcuts[otherAction])) {
    return otherAction === "go" ? "GO" : "Cue";
  }

  return fixedShortcutConflicts().find((item) => shortcutsEqual(shortcut, item.shortcut))?.label ?? null;
}

function shortcutsEqual(left: KeyboardShortcut, right: KeyboardShortcut) {
  return (
    left.key === right.key &&
    left.modifiers.shift === right.modifiers.shift &&
    left.modifiers.control === right.modifiers.control &&
    left.modifiers.alt === right.modifiers.alt &&
    left.modifiers.meta === right.modifiers.meta
  );
}

function fixedShortcutConflicts(): Array<{ label: string; shortcut: KeyboardShortcut }> {
  return [
    fixedCommandShortcut("New Session", "N", false),
    fixedCommandShortcut("Open Session", "O", false),
    fixedCommandShortcut("Save Session", "S", false),
    fixedCommandShortcut("Save As", "S", true),
  ];
}

function fixedCommandShortcut(
  label: string,
  key: string,
  shift: boolean,
): { label: string; shortcut: KeyboardShortcut } {
  const isMac = navigator.platform.toLowerCase().includes("mac");
  return {
    label,
    shortcut: {
      key,
      modifiers: {
        shift,
        control: !isMac,
        alt: false,
        meta: isMac,
      },
    },
  };
}
```

- [ ] **Step 5: Run Settings tests and adjust platform-sensitive test if needed**

Run: `npm --prefix ui run test -- SettingsTab.test.tsx`

Expected: PASS. If the fixed-file accelerator test fails because the test environment reports a non-macOS platform, change the event from `metaKey: true` to `ctrlKey: true` and keep the expected text as `Already assigned to Save Session`.

- [ ] **Step 6: Run broader frontend verification**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

Run: `npm --prefix ui run test`

Expected: PASS.

- [ ] **Step 7: Commit Task 3**

Run:

```bash
git status --short
git diff -- ui/src/components/KeyboardShortcutInput.tsx ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx
git add ui/src/components/KeyboardShortcutInput.tsx ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx
git commit -m "feat: detect shortcut conflicts"
```

---

## Final Verification

- [ ] Run Rust targeted menu tests: `cargo nextest run -p advanced-show-control ui::menu::tests`
- [ ] Run frontend typecheck: `npm --prefix ui run typecheck`
- [ ] Run frontend tests: `npm --prefix ui run test`
- [ ] Run formatting checks if any formatting changed: `make fmt`
- [ ] Inspect final status: `git status --short`
- [ ] If all checks pass, request code review using the project’s review workflow before declaring the implementation complete.

## Self-Review Notes

- Spec coverage: Task 1 covers native file accelerators; Task 2 covers GO/Cue execution, capture preemption, ignored unavailable actions, and GO duplicate precedence; Task 3 covers configurable and fixed-file conflict detection and inline red messaging.
- Placeholder scan: no `TBD`, `TODO`, or deferred implementation placeholders remain.
- Type consistency: helper names and prop names are introduced before use and reused consistently across tasks.
