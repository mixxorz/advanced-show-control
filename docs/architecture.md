# Backend Architecture

## Overview

The runtime is split into six boundary pieces:

- `Lv1Actor`
- `FadeEngine`
- `ShowState`
- `SceneRecallFader`
- `Tauri Shell`
- `AppEventBus`
- `AppCommandBus`

The rule is simple: no module reaches into another module's state directly. Modules publish facts on `AppEventBus` and send requests through `AppCommandBus`.

## Bus Contracts

- `AppEventBus = facts`
- `AppCommandBus = commands and queries`

`AppEventBus` carries broadcast facts only.

- Non-blocking publish.
- Independent subscribers.
- No replay.
- Missed events are tolerated and surfaced as lag, not hidden state coupling.

`AppCommandBus` carries acknowledged requests only.

- Commands and queries are routed to the current live target.
- Requests are not broadcast.
- If the target is unavailable, the caller gets a clear failure.

## Core Ownership

- `Lv1Actor` owns the LV1 TCP connection and the mirrored LV1 state.
- `FadeEngine` owns fade timing, overlap behavior, and LV1 fader writes.
- `ShowState` owns show data only: scene configs, scoped faders, stored target values, and show-file persistence.
- `SceneRecallFader` owns scene recall policy and decision-making.
- `Tauri Shell` owns UI projection, shell commands, and user-facing state derived from the runtime.

`ShellState` is the Tauri-side view of the runtime, not the owner of show logic or recall policy.

## Event Flow

`LV1 TCP -> Lv1Actor -> AppEventBus -> ShowState / FadeEngine / SceneRecallFader / Tauri Shell`

```text
┌─────────┐     ┌──────────┐     ┌─────────────┐
│ LV1 TCP │ ──▶ │ Lv1Actor │ ──▶ │ AppEventBus │
└─────────┘     └──────────┘     └──────┬──────┘
                                        │
                    ┌───────────────────┼────────────────────┐
                    │                   │                    │
                    ▼                   ▼                    ▼
          ┌────────────────┐   ┌────────────┐   ┌──────────────────┐
          │ ShowState      │   │ FadeEngine │   │ SceneRecallFader │
          └────────┬───────┘   └─────┬──────┘   └────────┬─────────┘
                   │                │                   │
                   ▼                ▼                   ▼
          ┌────────────────┐   ┌────────────┐   ┌──────────────────┐
          │ Tauri Shell    │   │ LV1 writes │   │ recall policy    │
          │ projection     │   │ / overlap  │   │ / decisions      │
          └────────────────┘   └────────────┘   └──────────────────┘
```

`Lv1Actor` translates incoming LV1 traffic into facts. Subscribers consume those facts independently. `SceneRecallFader` must not depend on Tauri projection ordering; it reads fresh LV1 state through `AppCommandBus` before it decides whether a recall should start, skip, or continue.

## Command Flow

`Tauri Shell / FadeEngine / SceneRecallFader -> AppCommandBus -> current LV1 / fade targets`

```text
┌──────────────┐   ┌────────────┐   ┌──────────────────┐
│ Tauri Shell  │   │ FadeEngine │   │ SceneRecallFader │
└──────┬───────┘   └─────┬──────┘   └────────┬─────────┘
       │                 │                   │
       └─────────────────┼───────────────────┘
                         │
                         ▼
                 ┌───────────────┐
                 │ AppCommandBus │
                 └───────┬───────┘
                         │
            ┌────────────┴────────────┐
            │                         │
            ▼                         ▼
       ┌──────────┐            ┌──────────────┐
       │ Lv1Actor │            │ FadeEngine   │
       └──────────┘            └──────────────┘
```

`FadeEngine` owns overlap behavior. Different scenes can overlap on unrelated faders. A new recall takes over only overlapping faders. There is no `finish_now` command; same-scene behavior is handled inside fade ownership and overlap rules.

## Scene Recall Ownership

`SceneRecallFader` owns recall policy.

- It listens for LV1 scene recall facts.
- It asks for a fresh LV1 snapshot before deciding.
- It validates exact scene identity, lockout state, connection state, stored scene config, scoped targets, stored fader values, and live topology.
- It treats duration `0` scenes as disabled.
- It starts validated fades through the command bus.
- It does not reach into `ShowState` internals.

`ShowState` owns show data only.

- It stores and projects the app's show configuration.
- It does not decide recall policy.
- It does not own validation rules for scene recall.

`FadeEngine` owns overlap behavior.

- It starts fades from live values.
- It overlaps on unrelated faders.
- It takes over only overlapping channels for a new recall.
- It does not expose a finish-now command.

## Runtime Lifecycle

- `connect` installs the current command targets and starts the actor, fade, recall, and shell projection tasks.
- `disconnect` and reconnect clear command targets and abort old runtime tasks.
- Generation guards prevent stale events, snapshots, or handles from mutating current state.

The Tauri shell projection only applies events for the active generation, and stale runtime handles are rejected instead of being installed. `SceneRecallFader` also checks the active generation before validation, before logging automation status, and immediately before dispatching a fade start so a stale recall task cannot send after disconnect or reconnect.

## File Structure

Core runtime modules live under `src/`:

- `src/lv1/` for LV1 connection, events, commands, handles, and state.
- `src/fade/` for fade engine commands, events, state, timing, and fader law.
- `src/show/` for show state, show commands, event handling, capture, and shared scene/channel types.
- `src/scene_recall/` for recall policy, recall state, events, and the scene recall fader actor.
- `src/runtime/` for bus-level commands and events.
- `src/osc.rs` and `src/vegas.rs` for transport or protocol helpers.

The Tauri shell lives under `src-tauri/src/`:

- `src-tauri/src/app_state/` for `ShellState`, projections, logs, show-file mapping, and view models.
- `src-tauri/src/commands.rs` for shell commands.
- `src-tauri/src/connection_state.rs` and `src-tauri/src/connection_preferences.rs` for shell-facing connection state.

## Non-Goals

- No durable event store.
- No replay.
- No distributed bus.
- No plugin runtime.
