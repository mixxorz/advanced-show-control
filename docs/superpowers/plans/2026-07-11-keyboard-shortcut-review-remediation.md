# Keyboard Shortcut Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent held-key, editable-control, modal, and noncanonical-setting behavior from causing misleading or unintended shortcut execution.

**Architecture:** Extend the centralized keyboard event with repeat metadata and a shared action-suppression helper. GO remains global and Cue remains local to `CueListsTab`; both consume repeats without dispatch and ignore action shortcuts from editable/dialog targets. Rust canonicalizes persisted labels while shared frontend key equality keeps display, conflicts, and execution consistent for projected or test-provided settings.

**Tech Stack:** Rust, React, TypeScript, Vitest, Testing Library, Cargo nextest.

## Global Constraints

- Shortcut capture remains priority `1000`, GO remains `100`, and Cue remains `90`.
- Capture must continue to work while its Settings button has focus.
- Matching repeated GO or Cue events are consumed without dispatch.
- Action shortcuts from `input`, `textarea`, `select`, content-editable elements, or descendants of `[role="dialog"]` are ignored so native editing and modal controls continue normally.
- Strict GO precedence applies only when action shortcuts are eligible; it must not consume text entered into blocked contexts.
- Reuse `recallCuedCue` and `cueEntry`; do not change backend recall safety paths.
- Rust test style for settings normalization is pure unit testing.
- Frontend shortcut execution and suppression use Vitest UI tests through the public keyboard provider and rendered runtime/components.

---

### Task 1: Consume OS Repeat Events Without Dispatch

**Files:**
- Modify: `ui/src/keyboard.tsx`
- Modify: `ui/src/keyboard.test.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`
- Modify: `ui/src/components/CueListsTab.tsx`
- Modify: `ui/src/components/CueListsTab.test.tsx`

**Interfaces:**
- Add `repeat: boolean` to `AppKeyboardEvent`, populated from `KeyboardEvent.repeat`.
- GO and Cue handlers return `handled` for matching repeated events without invoking commands.

- [ ] **Step 1: Add failing repeat propagation and execution tests**

Add a keyboard-provider test that records `event.repeat` from a dispatched `KeyboardEvent` with `{ repeat: true }` and expects `true`.

Extend the AppRuntime held-GO regression so the first command resolves before a second event with `repeat: true` is dispatched; assert `recallCuedCue` remains called once and the repeated event is default-prevented.

Add a Cue Lists test that selects an entry, dispatches the configured Cue event with `repeat: true`, and asserts `cueEntry` is not called while the event is default-prevented.

- [ ] **Step 2: Verify RED**

Run: `npm --prefix ui run test -- keyboard.test.tsx AppRuntime.test.tsx CueListsTab.test.tsx`

Expected: FAIL because `AppKeyboardEvent` does not expose repeat and completed GO/Cue commands can dispatch on repeated events.

- [ ] **Step 3: Implement repeat propagation and consumption**

Add `repeat` to `AppKeyboardEvent` and `normalizeKeyboardEvent`:

```tsx
export type AppKeyboardEvent = {
  code: string;
  key: string;
  modifiers: KeyboardShortcutModifiers;
  repeat: boolean;
  originalEvent: KeyboardEvent;
};

repeat: event.repeat,
```

In the GO handler, after a successful GO shortcut match and before projected-cue validation, return `"handled"` when `event.repeat` is true. Keep the in-flight guard unchanged for non-repeat overlapping events.

In the Cue handler, preserve strict GO matching first. After Cue matches, return `"handled"` when `event.repeat` is true before checking selection or dispatching `cueEntry`.

- [ ] **Step 4: Verify GREEN and commit**

Run: `npm --prefix ui run test -- keyboard.test.tsx AppRuntime.test.tsx CueListsTab.test.tsx && npm --prefix ui run typecheck`

Expected: PASS with no React act warnings.

Commit: `fix: suppress repeated shortcut actions`

---

### Task 2: Ignore Action Shortcuts From Editable And Dialog Targets

**Files:**
- Modify: `ui/src/keyboard.tsx`
- Modify: `ui/src/keyboard.test.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`
- Modify: `ui/src/components/CueListsTab.tsx`
- Modify: `ui/src/components/CueListsTab.test.tsx`

**Interfaces:**
- Export `isActionShortcutBlocked(event: AppKeyboardEvent): boolean` from `keyboard.tsx`.
- GO and Cue handlers return `ignored` before precedence or repeat handling when action shortcuts are blocked.

- [ ] **Step 1: Add failing blocked-context tests**

Add pure provider coverage for an event whose target is an `input`, `textarea`, `select`, content-editable descendant, normal button, and button inside `[role="dialog"]`.

Add AppRuntime coverage that focuses a rendered input inside the cue-list name dialog, types the configured GO key, and asserts `recallCuedCue` is not called and text entry still occurs.

Add Cue Lists coverage that dispatches the configured Cue key from an editable element and asserts `cueEntry` is not called.

Retain the existing capture-preemption test to prove Settings capture still receives keys.

- [ ] **Step 2: Verify RED**

Run: `npm --prefix ui run test -- keyboard.test.tsx AppRuntime.test.tsx CueListsTab.test.tsx`

Expected: FAIL because action handlers currently ignore event targets.

- [ ] **Step 3: Implement shared action suppression**

Add this helper using the original event target:

```tsx
export function isActionShortcutBlocked(event: AppKeyboardEvent) {
  const target = event.originalEvent.target;
  return (
    target instanceof Element &&
    target.closest(
      'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="dialog"]',
    ) !== null
  );
}
```

In both action handlers, check this helper before strict GO precedence, repeat handling, or command dispatch. Return `"ignored"` so native text and control behavior continues. Do not put this check in the provider dispatch loop because capture must remain eligible.

- [ ] **Step 4: Verify GREEN and commit**

Run: `npm --prefix ui run test -- keyboard.test.tsx AppRuntime.test.tsx CueListsTab.test.tsx SettingsTab.test.tsx && npm --prefix ui run typecheck`

Expected: PASS with capture behavior unchanged.

Commit: `fix: block shortcuts in editing contexts`

---

### Task 3: Canonicalize Persisted Shortcut Labels

**Files:**
- Modify: `src-tauri/src/settings/types.rs`
- Modify: `ui/src/keyboard.tsx`
- Modify: `ui/src/keyboard.test.tsx`
- Modify: `ui/src/components/SettingsTab.tsx`
- Modify: `ui/src/components/SettingsTab.test.tsx`

**Interfaces:**
- Rust `KeyboardShortcut::normalized_or` emits canonical one-character and known named labels.
- Export `shortcutKeysEqual(left: string, right: string): boolean` from `keyboard.tsx` and use it for execution and Settings conflict equality.

- [ ] **Step 1: Add failing pure Rust normalization tests**

Extend `normalization_clamps_sensitivity_and_trims_shortcuts` or add focused tests proving:

```rust
assert_eq!(normalize_key(" c "), Some("C".to_string()));
assert_eq!(normalize_key("space"), Some("Space".to_string()));
assert_eq!(normalize_key("ARROWDOWN"), Some("ArrowDown".to_string()));
```

Cover `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, and all four arrow labels case-insensitively. Empty input continues to use the action default.

- [ ] **Step 2: Verify Rust RED**

Run: `cargo nextest run -p advanced-show-control settings::types::tests`

Expected: FAIL because normalization only trims labels.

- [ ] **Step 3: Implement minimal Rust canonicalization**

Add a private `normalize_key` helper. Trim first, uppercase one-character labels, map known named labels case-insensitively to their browser `KeyboardEvent.key` spelling, and preserve trimmed unknown named labels rather than silently deleting user data. Use it from `normalized_or` and retain default fallback for empty labels.

- [ ] **Step 4: Add failing frontend equality tests**

Add tests proving lowercase projected `c` matches a `KeyC` event and conflicts with captured uppercase `C`. Add a Settings test with lowercase existing Cue and captured uppercase GO, expecting `Already assigned to Cue` without replacing settings.

- [ ] **Step 5: Implement shared frontend key equality**

Export:

```tsx
export function shortcutKeysEqual(left: string, right: string) {
  return left.toLocaleUpperCase() === right.toLocaleUpperCase();
}
```

Use it in `shortcutMatchesEvent` and `SettingsTab.shortcutsEqual`. Keep modifier equality exact. This is defensive for projected/test state while Rust ensures persisted settings become canonical.

- [ ] **Step 6: Verify GREEN and commit**

Run:

```bash
cargo nextest run -p advanced-show-control settings::types::tests
npm --prefix ui run test -- keyboard.test.tsx SettingsTab.test.tsx
npm --prefix ui run typecheck
```

Expected: PASS.

Commit: `fix: canonicalize shortcut key labels`

---

## Final Verification

- [ ] Run `make check`.
- [ ] Run focused shortcut tests: `npm --prefix ui run test -- keyboard.test.tsx SettingsTab.test.tsx CueListsTab.test.tsx AppRuntime.test.tsx`.
- [ ] Verify `git status --short --branch` is clean in the worktree and main remains untouched.
- [ ] Generate a whole-branch review package and dispatch a fresh verifier.
- [ ] Push `feat/keyboard-shortcut-execution` and create a PR linked to issue `#11` only after review approval.

## Verification Boundary

- Frontend tests prove repeat suppression, editable/dialog suppression, capture precedence, command routing, and conflict behavior.
- Pure Rust tests prove settings canonicalization.
- Existing backend actor tests and `make check` prove the reused `recallCuedCue` path retains backend safety behavior.
- Hardware/native accelerator execution remains not smoke-verified unless `make smoke` is run against an LV1-compatible environment.
