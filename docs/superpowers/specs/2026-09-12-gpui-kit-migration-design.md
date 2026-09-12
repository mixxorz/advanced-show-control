# GPUI Kit Migration Design

**Status:** Approved design
**Date:** 2026-09-12

## Summary

Advanced Show Control will replace its Tauri and React application with a native Rust desktop application built with GPUI Kit. The migration will preserve all implemented production workflows, safety behavior, persisted data, and the general appearance of the current interface. The supported targets will be macOS 15 or newer and Windows 10 or newer.

This is a direct replacement performed in one continuous implementation pass on `migration/gpui-kit`. There will be no shipped dual-host state, intermediate parity gates, or partial migration deliverables. The work may follow dependency order internally, but it continues until the complete replacement satisfies the completion criteria in this document.

## Goals

- Replace the Tauri host and React frontend with GPUI Kit 0.6.1.
- Preserve every currently implemented production workflow:
  - LV1 discovery, connection, disconnect, and reconnect visibility;
  - scene selection, linking, capture/store, scope editing, fade duration, copy/paste, and recall;
  - cue-list creation, editing, ordering, selection, CUE, and GO;
  - settings editing and shortcut capture;
  - show creation, loading, saving, and Save As;
  - logs, status, lockout, fade visibility, and Abort All;
  - native menus, file dialogs, confirmations, notifications, shortcuts, and window-title updates.
- Preserve existing actor ownership, safety rules, runtime generations, and projector semantics.
- Preserve the current `.ascs` and `settings.json` formats exactly, without conversion or data loss.
- Preserve existing application-data locations so installed users retain settings and diagnostic history.
- Preserve the current layout, typography, colors, spacing, control density, and information hierarchy in general.
- Produce an unsigned macOS application bundle and an unsigned Windows GUI executable with a distributable package.
- Remove all production and development-tool dependencies on Tauri and JavaScript.

## Non-goals

- A substantial visual redesign.
- Pixel-identical rendering where native GPUI controls differ appropriately in focus, accessibility, or platform behavior.
- Support for macOS versions older than 15 or Windows versions older than 10.
- Signed or notarized packages, installers, or updater integration.
- Unrelated roadmap work. An existing issue is included only when replacing its Tauri implementation necessarily resolves it or when it blocks parity.
- Replacing the existing Tokio actor architecture with GPUI tasks.
- Changing persisted schemas or LV1 protocol behavior.

## Chosen Approach

The migration will use an in-place host replacement. Existing domain and actor modules remain the source of truth. Tauri-specific dependencies are removed from their boundaries, and a GPUI host is built around the same actor commands and projected state.

This approach is preferred over extracting and moving all domain code into a new core crate because broad restructuring would increase the regression surface without improving the migration outcome. It is preferred over a clean rewrite because the current actor implementation and tests already encode safety-critical behavior.

The final repository layout will use neutral names:

```text
app/                  production advanced-show-control crate
dev-tools/            separate non-publishable probe and hardware-smoke crate
docs/                 architecture, conventions, and migration documentation
```

The existing `src-tauri/` production crate will become `app/`. Its domain module layout remains intact. The existing development-only crate will become top-level `dev-tools/`. The `ui/` directory and JavaScript toolchain will be removed.

## Target Architecture

### Runtime ownership

The existing ownership model remains unchanged:

- `Lv1Actor` owns LV1 transport, reconnect, and mirrored live state.
- `FadeEngine` owns active fade timing and writes.
- Scenes owns scene configuration, selection, reconciliation, and recall policy.
- Cue Lists remains owned with the scene/session document boundary.
- Show owns show metadata, dirty state, lockout, connected identity, and persistence orchestration.
- Settings owns public settings and remembered private identity.
- Lifecycle owns connection generations and runtime peer installation.
- The projector remains the sole constructor of complete UI snapshots.

GPUI widgets never become authoritative owners of these domains.

### Executor boundary

GPUI owns the native application event loop, windows, focus, input, and rendering. A dedicated Tokio runtime continues to own the existing actors, network operations, timers, and asynchronous file work.

The GPUI `App`, `Window`, and context types do not enter Tokio actors. Tokio runtime types do not enter component rendering. The two runtimes communicate through a narrow native bridge:

- GPUI handlers submit explicit actor commands without blocking the GPUI thread.
- The projector publishes complete, versioned `AppViewState` values through a host-neutral projection sink.
- The bridge schedules accepted projection updates on the GPUI thread.

This boundary hides executor and thread-affinity details from both the domain and components.

### Native host boundary

Tauri bootstrap, managed state, command macros, event emission, menus, and window APIs will be replaced by native Rust modules responsible for:

- application startup and shutdown;
- Tokio runtime ownership;
- actor/lifecycle construction;
- application-data and diagnostic paths;
- projection delivery to GPUI;
- window title and native application menu integration;
- native open/save path prompts;
- frontend-safe command completion and error delivery.

These modules remain adapters. Business validation stays in the owning actor.

### Projection boundary

The projector remains the only backend component that constructs complete UI state. The Tauri-specific `app-status-changed` emitter becomes a host-neutral latest-snapshot sink while retaining these guarantees:

- every snapshot has a monotonically increasing `state_version`;
- consumers accept a snapshot only when its version is newer than the latest accepted version;
- lag recovery uses the latest complete state rather than replaying stale intermediate states;
- generation-bound LV1 and Fade facts are filtered before presentation;
- app-lifetime Show, Scenes, Cue Lists, and Settings state remains available to late subscribers;
- bounded user-facing logs remain part of projected state.

## Native UI Design

### Visual parity

The GPUI application will retain the current shell geometry, information hierarchy, typography, Fira font family, color palette, spacing, borders, interaction emphasis, and dense control layout. GPUI theme tokens will replace CSS and Tailwind variables. Native controls may differ in detailed rendering where required for focus, accessibility, or platform integration, but the application should remain immediately recognizable to current users.

The port will not reproduce the React component tree mechanically. Components will be organized around complete workflows with narrow state and command interfaces.

### Application shell

`AppRoot` owns projection subscription, accepted snapshot state, window title, theme, transient dialogs, notifications, and top-level command dispatch. It renders:

- top-level Scenes, Cue Lists, Events, and Settings tabs;
- the current Events placeholder;
- a persistent bottom status bar;
- LV1 connection, show dirty state, lockout, fade state, and Abort All visibility;
- the user-facing operational log surface;
- required dialog, sheet, and notification layers.

### Scenes workflow

The Scenes view uses a split workspace with a virtualized scene list and selected-scene editor. It includes native controls for duration, fader and pan-family scope, channel selection, store, copy/paste, link, unlink, and recall. Channel groups remain organized by LV1 topology. Duplicate-name warnings, unavailable-state explanations, disabled controls, confirmation behavior, and selection persistence are preserved.

### Cue Lists workflow

The Cue Lists view preserves list management, cue-entry insertion and ordering, active/cued selection, missing-scene indicators, double-click behavior, CUE, and GO. GPUI drag/drop primitives replace `dnd-kit`. Keyboard navigation, focus, drag previews, and modal suppression are explicit component behavior rather than browser-global behavior.

### Connection workflow

The connection dialog preserves discovery, system identity, row-local latency status, connect/disconnect behavior, current connection visibility, and single-flight operations. Runtime reconnect remains owned by `Lv1Actor`; the UI requests only explicit connect or disconnect.

### Settings workflow

Settings use controlled GPUI inputs. Each edit submits one complete `AppSettings` replacement. A transient full-object draft may be shown while awaiting the next projected snapshot, but command completion does not become authoritative state. Shortcut capture preserves normalization, conflicts, modal priority, and editable-control suppression.

### Native controls and assets

GPUI Kit supplies tabs, virtual lists, inputs, selects, switches, toggles, dialogs, sheets, menus, scrolling, notifications, and accessibility semantics. Lucide-based GPUI assets replace `lucide-react`. Custom application icons and fonts are packaged as native resources or embedded assets as appropriate.

## State and Command Flow

### Startup

1. The GPUI application initializes native assets, theme, and the main window.
2. The host resolves the existing application-data paths and initializes diagnostics.
3. The host starts the dedicated Tokio runtime and constructs app-lifetime actors and lifecycle dependencies in their required order.
4. The projector constructs the initial `AppViewState` and sends subsequent snapshots through the native projection sink.
5. The GPUI bridge accepts only newer versions and notifies `AppRoot` to render the latest state.

A partially initialized automation runtime is never presented as ready.

### Backend-to-UI state

GPUI entities render from the latest accepted projection. They may additionally hold only presentation concerns such as focus, open dialogs, drag previews, pending input text, and transient full-object settings drafts. Command responses, local polling, and optimistic fragments do not overwrite projected backend state.

### UI-to-backend commands

GPUI event handlers construct explicit owner commands through thin host-neutral adapters. Awaited mailbox work runs outside the GPUI thread. Native menu actions and visible controls call the same command functions. Results communicate completion or a frontend-safe error; resulting domain state arrives through the projector.

File prompts obtain a path and then invoke Show-owned persistence. Cancellation returns no operation and does not mutate the current show. Connection, recall, cue, fade, and settings operations retain their existing bounded and single-flight behavior.

### Logs

Runtime modules continue to use `tracing`. Diagnostic sinks retain structured JSONL output in the existing application-data location. The tracing UI sink supplies user-facing `INFO`, `WARN`, and `ERROR` entries to the projector. Notifications or dialogs may summarize a command result, but projected logs remain the operational history.

## Safety Requirements

The migration changes presentation and host integration only. It does not weaken mixer-control safety:

- lockout, active generation, fresh LV1 state, exact scene identity, live topology, configured scope, and stored targets remain backend-authoritative;
- a rejected generation check is terminal for the operation;
- stale work cannot send LV1 commands, mutate current or app-lifetime state, or publish success;
- recall validation occurs before Fade admission or aborting an existing fade;
- blocked, skipped, disabled, or unsafe recalls do not abort an active fade;
- manual override, Abort All, overlap, exact same-scene behavior, disconnect, and readiness timeout behavior remain intact;
- frontend visibility, disabled states, and confirmation controls remain advisory only;
- unsafe outcomes remain visible through projected state, frontend-safe errors, or complete `tracing` messages.

Existing contracts that refer specifically to Tauri or `app-status-changed` will retain their stable IDs and owners while their prose is updated to describe the host-neutral native projection boundary. Contract behavior will not be weakened.

## Error Handling and Shutdown

- GPUI does not block on actor replies, discovery, network work, file I/O, or path prompts.
- Closed mailboxes, runtime task failures, unavailable state, and projection-bridge failures map to categorized frontend-safe errors and complete diagnostic events.
- Raw OSC payloads, serialized settings, and serialized show/session documents remain excluded from user-facing errors and logs.
- Closing a file prompt or confirmation is normal cancellation, not an error.
- Canceled New, Open, or Save As actions leave the current show unchanged.
- Startup failure displays a native fatal-error surface and exits without leaving partial automation active.
- Normal application shutdown advances the runtime generation, removes peers, stops automation work, and then shuts down Tokio.

## Persistence Compatibility

The serialized `.ascs` schema and `settings.json` representation retain their current field names, schema versions, defaults, and read/write semantics. Existing files require no conversion, and existing fixtures continue to pass. The migration does not introduce a schema version or conversion path.

Application identifiers and platform data locations remain compatible with the current installation:

- macOS continues using `~/Library/Application Support/com.advancedshowcontrol.app`;
- Windows continues using the existing `%APPDATA%\com.advancedshowcontrol.app` location;
- diagnostic filenames, settings filename, default show location, backup naming, and retention behavior remain unchanged.

## Development and Smoke Tools

The Tauri debug application is removed. The replacement is a separate non-publishable Rust smoke CLI under `dev-tools/`.

The smoke CLI exercises production runtime commands for discovery, connection, show creation, scene configuration, scope and duration, recall, lockout, settings, and cue-list workflows. Debug-only APIs remain limited to deterministic setup or observations unavailable through production commands, including raw LV1 recall and test-channel parameter access. Debug-only code is not linked into or shipped with the production binary.

`logs/debug-smoke-report.txt` remains the authoritative smoke result. Terminal output alone is not evidence that the hardware suite passed.

The existing LV1 probe remains in the same development-only crate.

## Testing Strategy

### Allowed Rust test styles

- **Pure unit tests:** direct tests for deterministic domain functions, projection transforms, formatting, theme values, and other side-effect-free behavior.
- **Actor tests:** behavior exercised through actor mailboxes, `AppEventBus`, and a tracing listener when logs are part of the contract.
- **Smoke tests:** end-to-end runtime and hardware behavior through the separate development CLI.

Side-effecting actor behavior is not tested by mutating internals or inspecting private state.

### Existing coverage

All existing domain and actor tests remain. The initial worktree baseline is 498 production-crate tests, 19 development-tool tests, and 206 frontend tests, for 723 passing tests. Current persistence fixtures remain regression inputs.

Tauri adapter tests are replaced by host-neutral command and projection tests before the obsolete adapters disappear. Behavior changes use test-driven development.

### GPUI coverage

- `#[gpui_kit::test]` interaction tests cover focus, keyboard routing, dialogs, component state, list operations, settings drafts, connection state, and command dispatch.
- Semantic, accessibility-tree, and layout snapshots cover stable presentation behavior.
- macOS `Window::render_to_image` snapshots provide image regression coverage where GPUI rendering is reliable.
- Windows appearance receives manual visual acceptance because GPUI Kit does not currently document an equivalent reliable Windows image-regression path.
- A development-only Rust component gallery replaces Storybook for focused visual review.
- Current screenshots remain migration references until the native workflows receive general visual-parity approval.

### Platform verification

CI and local checks cover formatting, Clippy, Rust tests, GPUI tests, and release builds. Native checks run on both macOS and Windows. Manual acceptance exercises every implemented workflow on both systems before completion is claimed.

## Packaging

GPUI Kit does not provide Tauri-equivalent application bundling. Repository-owned packaging commands will:

- create an unsigned macOS `.app` with the production binary, Info.plist, application icon, fonts/assets, and the existing bundle identifier;
- build the Windows binary with the GUI subsystem so no console window appears in release builds;
- collect the Windows executable and required resources into a distributable archive;
- expose repeatable Make targets and CI artifacts for each platform.

Signing, notarization, installers, and automatic updates are intentionally deferred.

## Single-Pass Implementation Constraint

The branch will not maintain a runnable dual-host architecture, stop at intermediate parity gates, or present partial ports as completed deliverables. Once implementation begins, work continues across runtime decoupling, GPUI UI construction, workflow parity, tests, smoke tooling, packaging, documentation, and legacy removal until the final completion criteria are met.

Dependency ordering is permitted as an implementation detail. It does not create approval checkpoints or change the single-pass scope.

## Legacy Removal

The completed migration removes:

- Tauri runtime, build, test, and CLI dependencies;
- Tauri configuration, capability schemas, generated schemas, and build scripts;
- React, Vite, TypeScript, JavaScript, npm manifests, and lockfiles;
- Storybook, browser unit-test infrastructure, Playwright visual infrastructure, and `dnd-kit`;
- the Tauri debug application;
- source and documentation references that incorrectly describe the final application as Tauri or React.

Current behavioral and visual references may be retained only as neutral documentation or GPUI fixtures when they remain useful after migration.

## Completion Criteria

The migration is complete only when:

1. Every currently implemented production workflow works on macOS and Windows.
2. General visual parity with the current application is approved.
3. Existing `.ascs` and `settings.json` files load and save without conversion or data loss.
4. Safety contracts, actor ownership, generation fencing, exact-scene validation, lockout, and fade behavior remain enforced and tested.
5. GPUI interaction, semantic, layout, and supported image tests pass.
6. The hardware smoke CLI passes and its report confirms success in an LV1-compatible environment.
7. Unsigned macOS and Windows artifacts build and run without development tooling.
8. Architecture, conventions, contracts, Make targets, and CI describe and verify the native application accurately.
9. No production or development-tool dependency on Tauri or JavaScript remains.
10. The repository contains no partial dual-host implementation.
