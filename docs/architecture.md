# Backend Architecture

## Overview

The runtime is split into eight boundary pieces:

- `Lv1Actor`
- `FadeEngine`
- `ShowState`
- `SceneRecallFader`
- `Tauri UI Adapter`
- `AppLifecycle`
- `AppEventBus`
- `AppCommandBus`

The rule is simple: no module reaches into another module's state directly. Modules publish facts on `AppEventBus` and send requests through `AppCommandBus`.

The runtime facts bus and logging pipeline are separate. `AppEventBus` broadcasts runtime facts used for state projection and policy decisions. Logging uses `tracing`; Tauri installs the desktop sinks for diagnostic files, stdout, and frontend log state.

## Bus Contracts

- `AppEventBus = facts`
- `AppCommandBus = commands and queries`

`AppEventBus` carries broadcast facts only. It currently carries LV1, fade, scene-recall, and show/app facts.

`AppEventBus` must not carry log-only events. If something is only a diagnostic or user-facing log, emit a `tracing` event instead.

- Non-blocking publish.
- Independent subscribers.
- No replay.
- Missed events are tolerated and surfaced as lag, not hidden state coupling.

`AppCommandBus` carries acknowledged requests only.

- Commands and queries are routed to the current live target.
- Requests are not broadcast.
- If the target is unavailable, the caller gets a clear failure.
`AppCommandBus` does not own or receive `AppEventBus`; modules that publish facts own their event-bus reference directly.

Low-risk show/app mutations, show-file import/export mapping, UI-requested recall validation/dispatch, projector-cache runtime projection, and projector-owned UI log input route through their target module boundaries. The projector is the only backend emitter for `app-status-changed`, and the Tauri command layer no longer returns or emits `AppViewState` snapshots.

React listens only to `app-status-changed`. There is no direct status emission from logging or Tauri command adapters.

## Core Ownership

- `Lv1Actor` owns the LV1 TCP connection lifecycle and mirrored LV1 state. During a connected session, a scoped writer task owns the TCP write half and reports write failures back to the actor.
- `FadeEngine` owns fade timing, overlap behavior, and LV1 fader writes.
- `ShowState` owns show data only: scene configs, one shared scoped channel list, `FADERS` and `PAN` scene toggles, stored target values, show-file persistence, and show/app snapshot-change fact publication. It is app-lifetime state behind a cloneable mutex-backed handle, not a spawned Tokio actor.
- `SceneRecallFader` owns scene recall policy and decision-making.
- `Tauri UI Adapter` owns Tauri setup, command registration, dialogs, and frontend serialization boundaries.
- `AppLifecycle` owns the current runtime command-bus holder seam and delegates generation-sensitive runtime handle installation/cleanup to the backend runtime lifecycle.

There is no `ShellState`; frontend projection is owned by the projector and UI listener flow.

## Event Flow

`LV1 TCP -> Lv1Actor -> AppEventBus -> projector / Tauri UI listener`

```text
┌─────────┐     ┌──────────┐     ┌─────────────┐
│ LV1 TCP │ ──▶ │ Lv1Actor │ ──▶ │ AppEventBus │
└─────────┘     └──────────┘     └──────┬──────┘
                                        │
                    ┌───────────────────┼────────────────────┐
                    │                   │                    │
                    ▼                   ▼                    ▼
          ┌────────────────┐   ┌────────────┐   ┌──────────────────┐
            │ FadeEngine    │   │ SceneRecallFader │   │ projector + UI │
           └─────┬─────────┘   └────────┬─────────┘   └──────┬────────┘
                 │                      │                    │
                 ▼                      ▼                    ▼
           ┌────────────┐       ┌──────────────────┐   ┌────────────────┐
            │ LV1 writes │       │ recall policy    │   │ projection     │
            │ / overlap  │       │ / decisions      │   │ + show state   │
           └────────────┘       └──────────────────┘   └────────────────┘
```

`Lv1Actor` translates incoming LV1 traffic into facts. Subscribers consume those facts independently. `SceneRecallFader` must not depend on frontend projection ordering; it reads fresh LV1 state and app show state through `AppCommandBus` before it decides whether a recall should start, skip, or continue. Scene recall fade dispatch is generation-checked at the command-bus boundary. Recall tasks may read state over several awaits, but the final fade start must compare the expected generation while cloning the current fade target so a stale recall cannot land on a newer connection.

## Logging Flow

```text
Core + Tauri tracing events
  -> Tauri tracing subscriber
    -> DEBUG+ JSONL diagnostic file
    -> DEBUG+ stdout
    -> INFO+ frontend app state
```

## Command Flow

`Tauri UI Adapter / AppLifecycle / FadeEngine / SceneRecallFader -> AppCommandBus -> current LV1 / fade targets`

```text
┌──────────────────┐   ┌────────────────┐   ┌────────────┐   ┌──────────────────┐
│ Tauri UI Adapter │   │ AppLifecycle   │   │ FadeEngine │   │ SceneRecallFader │
└────────┬─────────┘   └──────┬─────────┘   └─────┬──────┘   └────────┬─────────┘
         │                    │                   │                    │
         └────────────────────┼───────────────────┼────────────────────┘
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

`FadeEngine` owns overlap behavior. Different scenes can overlap on unrelated faders. A new recall takes over only overlapping faders. There is no `finish_now` command; same-scene behavior is not a separate command path and is handled inside `FadeEngine` ownership and overlap rules when a valid scene recall fade starts.

Low-risk show/app mutations, show-file import/export mapping, UI-requested recall validation/dispatch, and projector-cache runtime projection route through their target module boundaries. The Tauri command layer no longer returns or emits `AppViewState` snapshots.

`app-status-changed` emission is now owned only by the projector runtime.

`FadeEngine` tracks parameter-aware targets keyed by `(group, channel, FadeParameter)`. Fader targets use fader-law interpolation and fader-law override detection. Pan, balance, and width targets use direct linear interpolation. Pan-family manual override is driven only by pan movement. A pan override cancels pan, balance, and width for that channel together. Balance and width reports do not trigger override cancellation. Fader fades are not cancelled by pan-family override.

High-rate fade writes use `write_batch`. The command bus reports an unavailable LV1 target when no actor is installed. Once a batch reaches an LV1 actor, the actor may still drop queued writes during disconnect cleanup; this is intentional for the 25 Hz fade stream and must be surfaced through diagnostics rather than retried blindly.

## Scene Recall Ownership

`SceneRecallFader` owns recall policy.

- It listens for LV1 scene recall facts.
- It asks for a fresh LV1 snapshot before deciding.
- It validates exact scene identity, lockout state, connection state, stored scene config, scoped targets, stored fader values, and live topology.
- It skips scenes whose fader scope toggle is disabled and starts validated fader moves even when duration is 0.
- It starts validated fades through the command bus.
- It does not reach into `ShowState` internals.

`ShowState` owns show data only.

- It stores and projects the app's show configuration.
- It keeps one shared scoped channel list that both `FADERS` and `PAN` toggles reference.
- It does not decide recall policy.
- It does not own validation rules for scene recall.

`FadeEngine` owns overlap behavior.

- It starts fades from live values.
- It fades pan, balance, and width with direct linear interpolation.
- It overlaps on unrelated faders.
- It takes over only overlapping channels for a new recall.
- It does not expose a finish-now command.

`Lv1Actor` mirrors `/Notify/Track/Pan`, `/Notify/Balance`, and `/Notify/PanArcWidth`, and it sends the matching `/Set/Track/Pan*` commands for pan-family writes.

## Runtime Lifecycle

- App startup constructs `ShowState` without spawning Tokio work.
- `connect` installs the current command targets and starts the LV1 actor, fade, recall, and shell projection tasks.
- `disconnect` and reconnect clear command targets and abort old runtime tasks.
- Generation guards prevent stale events, snapshots, or handles from mutating current state.

The Tauri shell projection only applies events for the active generation, and stale runtime handles are rejected instead of being installed. `SceneRecallFader` also checks the active generation before validation, before logging automation status, and immediately before dispatching a fade start so a stale recall task cannot send after disconnect or reconnect.

## File Structure

Rust modules live under `src-tauri/src/` in the `advanced-show-control` package. Tauri-specific adapter code and core app modules are separated by module boundaries, not crate boundaries.

The Tauri UI adapter and lifecycle seams live under `src-tauri/src/`:

- `src-tauri/src/ui/` for Tauri setup and frontend command adapter exports.
- `src-tauri/src/lifecycle/` for app runtime lifecycle ownership seams.
- `src-tauri/src/commands.rs` for existing command implementations during the transition.
- `src-tauri/src/app_state/` for projections, logs, show-file mapping, and view models.
- `src-tauri/src/connection_state.rs` and `src-tauri/src/connection_preferences.rs` for shell-facing connection state.

Low-risk show/app mutations, show-file import/export mapping, and UI-requested recall validation/dispatch are routed through `AppCommandBus`. Projector cache and logging projection are owned by the projector and logging modules, and the Tauri command layer no longer returns `AppViewState` snapshots.

## Non-Goals

- No durable event store.
- No replay.
- No distributed bus.
- No plugin runtime.
