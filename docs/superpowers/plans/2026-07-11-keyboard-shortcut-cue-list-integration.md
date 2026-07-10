# Keyboard Shortcut Cue List Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate the keyboard shortcut branch with the current cue-list workflow so GO recalls the valid cued entry and Cue targets the selected cue-list entry.

**Architecture:** Keep global GO handling in `AppRuntime`, where projected cue-list state and `recallCuedCue` are available. Keep Cue handling in `CueListsTab`, which owns the local selected cue-entry ID. Both handlers reuse the existing keyboard provider and app command paths; GO has strict precedence and backend commands continue to enforce recall safety.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, Tauri 2, Cargo nextest.

## Global Constraints

- Shortcut capture priority remains `1000`; GO uses priority `100` and Cue uses priority `90`.
- A key matching GO is consumed even when GO cannot dispatch, so Cue never acts as a fallback for duplicate settings.
- GO dispatches only when the active cue list contains the cued entry and that entry references a projected scene config.
- Cue dispatches only for the selected entry in the active Cue Lists tab.
- Reuse `recallCuedCue` and `cueEntry`; do not add Tauri commands or bypass backend lockout, identity, stale-state, generation, or recall validation.
- Preserve settings persistence, shortcut capture, conflict detection, and native File menu accelerators.

## File Structure

- Modify `ui/src/AppRuntime.test.tsx`: replace obsolete scene-cue shortcut tests with cue-list GO and strict-precedence integration coverage.
- Modify `ui/src/AppRuntime.tsx`: make the global shortcut handler validate projected cue-list state and dispatch `recallCuedCue`.
- Modify `ui/src/components/CueListsTab.test.tsx`: cover Cue shortcut dispatch and unavailable selection.
- Modify `ui/src/components/CueListsTab.tsx`: register the Cue shortcut beside the local selected-entry state.

---

### Task 1: Cue-List GO Shortcut

**Files:**
- Modify: `ui/src/AppRuntime.test.tsx`
- Modify: `ui/src/AppRuntime.tsx`

**Interfaces:**
- Consumes: `AppViewState.activeCueListId`, `AppViewState.cuedCueEntryId`, `AppViewState.cueLists`, `AppViewState.sceneConfigs`, `AppCommands.recallCuedCue()`.
- Produces: a priority-100 global GO keyboard handler that always consumes a GO match and dispatches only for a valid projected cue.

- [ ] **Step 1: Replace obsolete GO tests with failing cue-list tests**

Replace the old `cuedSceneInternalId` shortcut cases in `ui/src/AppRuntime.test.tsx` with tests equivalent to:

```tsx
it("recalls the cued cue-list entry when the configured GO shortcut is pressed", async () => {
  const services = makeServices();
  render(<AppRuntime services={services} />);

  await waitFor(() => {
    expect(screen.queryByRole("heading", { name: "Connect to LV1" })).not.toBeInTheDocument();
  });

  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: " ",
        code: "Space",
        bubbles: true,
        cancelable: true,
      }),
    );
  });

  expect(services.recallCuedCue).toHaveBeenCalledOnce();
  expect(services.recallScene).not.toHaveBeenCalled();
});

it("consumes GO without recalling when no cue-list entry is cued", async () => {
  const services = makeServices();
  const appState = {
    ...connectedAppState,
    cuedCueEntryId: null,
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

  const event = new KeyboardEvent("keydown", {
    key: " ",
    code: "Space",
    bubbles: true,
    cancelable: true,
  });
  act(() => window.dispatchEvent(event));

  expect(services.recallCuedCue).not.toHaveBeenCalled();
  expect(event.defaultPrevented).toBe(true);
});
```

Also add equivalent unavailable cases for a missing active list, missing cued entry, and cued entry whose `sceneInternalId` is absent from `sceneConfigs`.

- [ ] **Step 2: Run the GO tests and verify RED**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx`

Expected: FAIL because `AppShortcutHandler` still reads removed `cuedSceneInternalId` and calls `recallScene`.

- [ ] **Step 3: Implement projected cue validation and strict consumption**

Replace the old combined handler in `ui/src/AppRuntime.tsx` with the minimal GO-only behavior:

```tsx
const GO_SHORTCUT_PRIORITY = 100;

function AppShortcutHandler(props: {
  appState: AppViewState;
  commands: AppCommands;
}) {
  useKeyboardHandler({
    id: "app-go-shortcut",
    priority: GO_SHORTCUT_PRIORITY,
    handleKeyDown: (event) => {
      if (!shortcutMatchesEvent(props.appState.settings.keyboardShortcuts.go, event)) {
        return "ignored";
      }

      const activeCueList = props.appState.cueLists.find(
        (cueList) => cueList.id === props.appState.activeCueListId,
      );
      const cuedEntry = activeCueList?.entries.find(
        (entry) => entry.id === props.appState.cuedCueEntryId,
      );
      const cueIsValid =
        cuedEntry !== undefined &&
        props.appState.sceneConfigs.some(
          (scene) => scene.internalSceneId === cuedEntry.sceneInternalId,
        );

      if (cueIsValid) {
        props.commands.recallCuedCue();
      }
      return "handled";
    },
  });

  return null;
}
```

- [ ] **Step 4: Run the GO tests and typecheck**

Run: `npm --prefix ui run test -- AppRuntime.test.tsx && npm --prefix ui run typecheck`

Expected: GO tests pass. Typecheck may still report obsolete Cue test references until Task 2 replaces them, but `AppRuntime.tsx` must have no type errors.

- [ ] **Step 5: Commit Task 1**

```bash
git add ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx
git commit -m "fix: align go shortcut with cue lists"
```

---

### Task 2: Selected Cue-Entry Shortcut And Strict GO Precedence

**Files:**
- Modify: `ui/src/components/CueListsTab.test.tsx`
- Modify: `ui/src/components/CueListsTab.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`

**Interfaces:**
- Consumes: `useKeyboardHandler`, `shortcutMatchesEvent`, local `selectedCueEntry`, `AppCommands.cueEntry(cueEntryId)`.
- Produces: a priority-90 Cue handler active while `CueListsTab` is mounted; duplicate GO matches are consumed without cueing.

- [ ] **Step 1: Add failing Cue shortcut component tests**

Add these behaviors to `ui/src/components/CueListsTab.test.tsx`:

```tsx
it("cues the selected cue-list entry with the configured Cue shortcut", async () => {
  const user = userEvent.setup();
  const cueEntry = vi.fn();
  renderWithAppProviders(<CueListsTab />, {
    appState: cueListStateFixture,
    commands: { cueEntry },
  });

  await user.click(screen.getByRole("button", { name: /Main.*002/i }));
  window.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "c",
      code: "KeyC",
      bubbles: true,
      cancelable: true,
    }),
  );

  expect(cueEntry).toHaveBeenCalledWith("cue-2");
  expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
});

it("does not run the Cue shortcut without a selected entry", () => {
  const cueEntry = vi.fn();
  renderWithAppProviders(<CueListsTab />, {
    appState: cueListStateFixture,
    commands: { cueEntry },
  });

  window.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "c",
      code: "KeyC",
      bubbles: true,
      cancelable: true,
    }),
  );

  expect(cueEntry).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run the Cue tests and verify RED**

Run: `npm --prefix ui run test -- CueListsTab.test.tsx`

Expected: FAIL because `CueListsTab` has no keyboard handler.

- [ ] **Step 3: Implement the local Cue handler**

Import `shortcutMatchesEvent` and `useKeyboardHandler` in `CueListsTab.tsx`, then register:

```tsx
const CUE_SHORTCUT_PRIORITY = 90;

useKeyboardHandler({
  id: "cue-list-cue-shortcut",
  priority: CUE_SHORTCUT_PRIORITY,
  handleKeyDown: (event) => {
    if (shortcutMatchesEvent(appState.settings.keyboardShortcuts.go, event)) {
      return "handled";
    }
    if (
      !shortcutMatchesEvent(appState.settings.keyboardShortcuts.cue, event) ||
      selectedCueEntry === null
    ) {
      return "ignored";
    }

    void commands.cueEntry?.(selectedCueEntry.id);
    setSelectedCueEntryId(null);
    return "handled";
  },
});
```

Place the hook after `selectedCueEntry` is derived so it uses the current active-list selection. Do not lift selection into projected or runtime state.

- [ ] **Step 4: Add failing full-runtime strict-precedence test**

In `ui/src/AppRuntime.test.tsx`, configure identical GO and Cue shortcuts, set `cuedCueEntryId: null`, navigate to Cue Lists, select `cue-2`, press the duplicate key, and assert:

```tsx
expect(services.recallCuedCue).not.toHaveBeenCalled();
expect(services.cueEntry).not.toHaveBeenCalled();
```

Expected before the strict-precedence implementation is complete: FAIL if Cue falls through when GO is unavailable.

- [ ] **Step 5: Run targeted tests and verify GREEN**

Run: `npm --prefix ui run test -- CueListsTab.test.tsx AppRuntime.test.tsx`

Expected: PASS with no React `act(...)` warnings.

- [ ] **Step 6: Commit Task 2**

```bash
git add ui/src/components/CueListsTab.tsx ui/src/components/CueListsTab.test.tsx ui/src/AppRuntime.test.tsx
git commit -m "fix: cue selected entries from shortcuts"
```

---

## Final Verification

- [ ] Run shortcut-focused tests: `npm --prefix ui run test -- keyboard.test.tsx SettingsTab.test.tsx CueListsTab.test.tsx AppRuntime.test.tsx`
- [ ] Run frontend formatting: `make ui-fmt`
- [ ] Run frontend lint: `make ui-lint`
- [ ] Run frontend typecheck: `make ui-typecheck`
- [ ] Run all frontend unit tests: `make ui-test`
- [ ] Run native File menu tests: `cargo nextest run -p advanced-show-control ui::menu::tests`
- [ ] Run the standard repository check: `make check`
- [ ] Inspect `git status --short` and the branch diff against `main`.
- [ ] Request code review before closing GitHub issue `#11`.

## Verification Boundary

- Vitest proves focused-window shortcut matching, frontend command routing, local selection behavior, capture priority, conflict visibility, and strict GO precedence.
- Rust unit tests prove stable native File menu accelerator values.
- Backend safety remains exercised by existing cue recall actor tests through `recallCuedCue`; no new backend path is introduced.
- This plan does not claim hardware or smoke verification. Run `make smoke` and inspect `logs/debug-smoke-report.txt` only when an LV1-compatible target is available.
