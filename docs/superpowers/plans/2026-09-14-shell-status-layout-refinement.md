# Shell Status and Layout Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan.

**Goal:** Match the approved shell reference more closely by removing Events, clarifying connection state and console selection, and rebalancing the bottom bar around a substantially larger GO button.

**Architecture:** Keep the change presentation-only in `native_ui/state.rs` and `native_ui/shell.rs`. Derive status presentation from projected `AppConnectionState`, preserve the existing connection-dialog callback and GO command path, and verify the result through pure mapping tests plus the real GPUI visual harness.

**Tech Stack:** Rust, GPUI Kit 0.6.1, GPUI native visual tests, `cargo nextest`.

## Global Constraints

- Production targets remain macOS 15 or newer and Windows 10 or newer.
- Use GPUI under `app/src/native_ui/`; do not add JavaScript, npm, React, Tauri, or browser-test dependencies.
- Remove only the Events presentation tab and placeholder; do not remove or rename runtime events, `AppEventBus`, diagnostics, or Logs.
- Read all connection presentation from the projected snapshot; do not add UI-owned connection truth.
- Keep the console name and chevron as one accessible control that opens the existing connection dialog.
- Preserve all GO availability, single-flight, lockout, modal, session-menu, shortcut-capture, exact-scene, generation, and actor safety behavior.
- Reuse theme tokens and GPUI Kit primitives; avoid a custom dropdown or separate chevron action.
- Follow RED-GREEN-REFACTOR and use only pure unit tests and native GPUI component tests for this presentation change.
- Keep the existing session-menu appearance, focus/action context, shortcuts, and popup lifecycle unchanged.

---

### Task 1: Refine the top navigation, connection status, console chooser, and bottom status bar

**Files:**
- Modify: `app/src/native_ui/state.rs`
- Modify: `app/src/native_ui/shell.rs`
- Modify: `app/src/native_ui/visual.rs`
- Modify: `app/src/native_ui/visual_snapshots/*.rgb`
- Modify: `site/docs/application-shell.md`
- Modify: `site/docs/assets/screenshots/application-shell.png`

**Interfaces:**
- Consumes: projected `AppConnectionState`; existing `AppShell::open_connection` callback; existing `bottom_status`, `status_cell`, and GO dispatcher path; GPUI Kit component `Button` and its built-in dropdown caret.
- Produces: connection-status presentation mapping; rendered status-dot/test IDs; bottom-bar cell/test IDs; no new backend or actor interface.

- [ ] **Step 1: Add failing behavior and geometry coverage**

  Manually inspect `app/src/CONTRACTS` plus local `go-single-flight` and `cued-scene-resolution` contracts. If `cc-check` is available, list contracts for `state.rs`, `shell.rs`, and `visual.rs`; otherwise record the unavailable tool and manual review.

  Before production edits:

  - Add a pure unit test with hard-coded expectations for Connected/Connecting/Disconnected label and color mapping: green `STATUS_CUED`, amber `STATUS_WARNING`, red `STATUS_DANGER`.
  - In the real AppRoot visual harness, require `tab-Events` to be absent and assert that the Logs tab begins immediately after Cue Lists.
  - Add stable test IDs and expected geometry assertions for an 8 px connection dot aligned with the label, the console chooser control, `bottom-status`, `go-cell`, `go`, and the four named status cells.
  - Assert GO fills at least 80% of its cell width and 75% of its cell height, the GO cell is approximately 14% of the footer width, and the four status-cell widths differ by no more than 1 px.
  - Exercise the connected console chooser and prove the existing connection dialog opens; close it before continuing the visual suite.

  Run focused tests and `make visual-test`. Record RED caused by the missing presentation mapping/IDs, remaining Events tab, and old GO/footer geometry. Fix only test setup errors until failures describe missing behavior.

- [ ] **Step 2: Implement the approved shell layout minimally**

  - Remove `MainTab::Events`, its rendered tab, and its placeholder content. Do not touch application event infrastructure.
  - Extract a small presentation mapping from `AppConnectionState` to label and existing status color token.
  - Render an 8 px `rounded_full` status dot beside the connection text with consistent spacing. Use green for Connected, amber for Connecting, and red for Disconnected.
  - Replace only the console-name control with GPUI Kit's component `Button`, keep its current minimum width/border/disabled behavior/callback, and enable the built-in trailing dropdown caret. Keep one accessible button and do not create a second chevron action.
  - Give GO a dedicated non-growing section near 14% width. Give CUED, CURRENT, MODE, and TIME equal zero-basis flex growth for the remainder.
  - Keep the current bottom-bar height unless the reference comparison shows the larger control needs one explicit shell-local height. Add consistent inset and make GO fill most of the section; increase its label prominence without changing the existing action handler or guards.
  - Keep right-edge border treatment coherent, including the final TIME cell.

- [ ] **Step 3: Refactor while green and review visuals**

  Run focused shell and visual tests. Refactor only clear duplication in status presentation or cell construction.

  Run `make visual-test`; expect old signatures to fail after material navigation/footer changes. Then run `make visual-update` and inspect every generated PNG affected by shared top/bottom chrome. Confirm dots, caret, tab order, GO size, equal footer cells, menu appearance, modal overlays, and SAFE state remain coherent. Refresh `site/docs/assets/screenshots/application-shell.png` from the accepted ready capture.

- [ ] **Step 4: Update user documentation**

  In `site/docs/application-shell.md`:

  - remove the Events-tab claim;
  - document green/amber/red connection dots;
  - describe the console-name control and trailing arrow as the connection chooser;
  - describe the larger GO section without changing safety claims.

- [ ] **Step 5: Compare against the reference and verify**

  Compare `dist/visual/native-shell-ready.png` with `/Users/mixxorz/Downloads/Codex Image Sep 14, 2026, 01_18_47 AM.png` using normalized proportions rather than raw pixels. Record a checklist for navigation order, status dot, console caret, GO prominence, footer distribution, and intentional differences.

  Run:

  ```bash
  cargo fmt --all -- --check
  cargo nextest run -p advanced-show-control native_ui::shell::tests
  make check
  make visual-test
  ```

  Expected: all commands pass with no test or snapshot failures. Existing dependency future-incompatibility notices may be recorded separately.

- [ ] **Step 6: Inspect and commit**

  Run `git status --short`, `git diff --check`, and inspect the focused diff. Confirm there are no actor, event-bus, connection-policy, recall, fade, session-menu, or persistence changes. Stage only intended source, docs, screenshot, and reviewed signatures. Commit with a concise `feat:` or `fix:` message and append exact RED/GREEN/reference-comparison evidence to `.superpowers/sdd/task-1-report.md`.
