# Phase 6: Tauri Thin Shell Design

## Purpose

Build the first durable desktop UI foundation for the LV1 Scene Fade Utility. This phase intentionally moves ahead of the full Phase 4 capture workflow because the next workflow work will be easier to validate inside the app shell than through CLI-only tools.

This is not a throwaway prototype. The shell should become the real MVP application frame that later phases extend with capture, project storage, scene recall automation, and external control.

## Scope

Included:

- Tauri desktop app scaffold.
- React + TypeScript + Vite frontend.
- Tailwind CSS 4 styling.
- Persistent tabbed layout with `Connection`, `Scene`, and `Logs` tabs.
- Shared global header with live status and safety controls.
- Tauri command layer exposing existing Rust LV1 state and fade safety actions.
- Live app status and event/log rendering.
- Manual smoke testing path for connected and disconnected operation.

Deferred:

- Listen Mode capture workflow.
- Captured fader confirmation table.
- Save/load project files.
- Scene recall automation.
- Companion/API integration.
- Dedicated `Fade` tab.
- Frontend component tests.
- Visual polish beyond a clear, high-contrast control-room layout.

## Architecture

The existing Rust crate remains the core protocol and fade library. Tauri wraps it as the desktop runtime, and React renders app state.

```text
src/                 existing Rust library and CLI
src-tauri/           Tauri desktop host
ui/                  React + TypeScript + Vite frontend
```

Responsibility boundaries:

- `src/lv1/*` keeps owning LV1 discovery, TCP, OSC parsing, state mirroring, and command sending.
- `src/fade/*` keeps owning fade curves, fade scheduling, override detection, abort, and finish-now behavior.
- `src-tauri/` owns desktop app state, Tauri commands, event forwarding, and frontend-safe DTOs.
- `ui/` owns layout, tabs, status rendering, form state, and user intent dispatch.

The frontend must not talk to LV1 directly. All hardware control flows through Rust.

## Runtime State

The Tauri app state should hold:

- Optional `Lv1ActorHandle`.
- Optional `FadeEngineHandle`.
- Current app status snapshot.
- Recent app/LV1/fade log entries in a bounded ring buffer.
- Lockout boolean.

The initial app status should be safe when no LV1 connection exists: disconnected, no scene, zero known channels, fade idle, lockout off.

## Frontend API

Tauri commands:

```text
connect_lv1(host?: string, port?: number, timeout_ms?: number)
disconnect_lv1()
get_app_status()
abort_all_fades()
finish_fade_now()
set_lockout(enabled: boolean)
```

`connect_lv1` should start or replace the current LV1 actor connection. If an existing connection is active, it should be shut down or superseded cleanly before storing the new handle.

`disconnect_lv1` should stop exposing the current actor and fade engine through app state. It should leave the UI in a safe disconnected state.

`abort_all_fades` and `finish_fade_now` should be no-ops when no fade engine exists. They should not error simply because LV1 is disconnected.

`set_lockout` should update shell state now, even before automatic recall logic exists. Later phases will enforce it when deciding whether a scene recall may start a fade.

Tauri events:

```text
app-status-changed
lv1-event
fade-event
app-log
```

Events are for live UI updates. The frontend should still call `get_app_status` on startup and after reconnects so it can recover from missed events.

## App Status DTO

The frontend-facing status should be stable and serializable:

```ts
type AppStatus = {
  connection: "disconnected" | "connecting" | "connected";
  currentScene: null | {
    index: number;
    name: string;
  };
  sceneCount: number;
  channelCount: number;
  fadeState: "idle" | "running" | "blocked";
  lockout: boolean;
  lastEventAt: string | null;
};
```

This DTO can grow later, but Phase 6 should keep it focused on state the shell actually renders.

## UI Structure

Use a durable tabbed layout rather than a status-only prototype.

Shared header:

- Connection status badge.
- Current scene index/name.
- Fade state badge.
- Lockout toggle or indicator.
- Large `Abort All` button that is always visible.
- `Finish Now` button visible or enabled when useful.

Tabs:

- `Connection`: host and port inputs, connect/disconnect controls, discovery placeholder if discovery is not wired in this phase, and connection details.
- `Scene`: current scene summary, scene list if available, known channel count, and clear placeholders for future capture/save workflow.
- `Logs`: chronological app, LV1, and fade events with timestamp, source, severity, and message.

There is no dedicated `Fade` tab. Manual fade-test tooling remains in the CLI and does not become part of the product UI.

## Styling

Use Tailwind CSS 4.

Visual direction:

- Dark control-room interface.
- High-contrast status badges.
- Large safety controls.
- Dense but readable event logs.
- No component library in this phase.

The styling should be good enough to keep and evolve, but not polished beyond the needs of the shell.

## Safety Behavior

Disconnected operation must be safe and boring:

- The UI renders without a connected LV1 instance.
- Abort and finish controls never panic if no fade engine exists.
- Connection errors are shown in logs and status, not hidden in the console.
- Lockout is visible in the header.
- The shell does not start fades automatically.

This phase should not add product UI affordances that imply capture, storage, or automatic recall already works.

## Testing And Verification

Automated verification:

- `cargo test` for existing Rust core behavior.
- Add Rust tests for Tauri-facing state reducers or DTO conversion helpers where practical.
- Frontend type-check with `tsc`.
- Frontend production build with Vite.
- Tauri build/check command if available in the scaffold.

Deferred testing:

- Frontend component tests.
- Browser/e2e automation.
- Live LV1-dependent automated tests.

Manual smoke test:

1. Launch the Tauri app.
2. Confirm the tabbed layout renders.
3. Confirm disconnected state renders safely.
4. Toggle lockout and verify the header/log updates.
5. Use connect controls against LV1 hardware if available.
6. Confirm scene/channel/log updates appear when connected.
7. Confirm abort and finish controls do not crash when idle or disconnected.

## Exit Criteria

- The Tauri app launches on the development machine.
- The React UI renders the durable tabbed shell.
- The shell shows disconnected state without errors.
- The shell can connect to LV1 through the existing Rust actor when hardware is available.
- Current scene, scene count, channel count, lockout state, and logs are visible.
- Global safety controls are always visible and safe to press.
- `cargo test`, frontend type-check, and frontend build pass.
