# Backend Architecture

## Overview

The backend is a local actor-based runtime built around two shared abstractions:

- `AppEventBus` for broadcast facts and events.
- `AppCommandBus` for acknowledged commands routed directly to the current LV1 and fade targets.

Actors own their own state and communicate through these buses instead of reaching into each other through concrete handles.

## Core Actors

- `Lv1Actor` owns the TCP connection and the LV1 state mirror. It publishes LV1 events onto `AppEventBus`.
- `FadeEngine` owns active fade timing. It receives fade commands, consumes LV1 events from `AppEventBus`, sends LV1 commands through `AppCommandBus`, and publishes fade events.
- `ShellState` owns the UI projection and show-file/editing state. It is updated by the shell-state projector, which consumes `AppEventBus`.

## Events And Commands

Events are broadcast facts:

- Publishing is non-blocking.
- Subscribers receive events independently.
- Subscribers may lag and log missed counts.
- Events are not replayed.

Commands are acknowledged requests:

- They are not broadcast.
- They are routed to the current target actor handle.
- If the target is unavailable, callers get a clear error and a `CommandFailed` event is published for logging/UI.

## Event Flow

`LV1 TCP -> Lv1Actor -> AppEventBus -> ShellState / FadeEngine / future automation`

`Lv1Actor` translates incoming LV1 traffic into LV1 events. Those events are broadcast to the rest of the runtime. `ShellState` projects them into UI state, `FadeEngine` reacts to relevant LV1 changes, and future automation should subscribe to the same bus.

## Command Flow

`Tauri / FadeEngine / future automation -> AppCommandBus -> current LV1 / fade handles`

`AppCommandBus` keeps the current LV1 and fade targets and sends commands directly to them. Tauri commands use the bus, `FadeEngine` uses it for LV1 writes, and future automation should do the same.

## Runtime Lifecycle

- `connect` installs the current command targets and starts the actor, fade, and projector tasks.
- `disconnect` and reconnect clear command targets and abort the old runtime tasks.
- Generation guards prevent stale events, snapshots, or handles from mutating current state.

The shell-state projector only applies events for the active generation, and stale runtime handles are rejected instead of being installed.

## Automation Boundary

Future automation should depend on `AppEventBus` and `AppCommandBus`, not on concrete actor handles. That keeps automation aligned with the same runtime contract used by LV1, fading, and the Tauri shell.

## Non-Goals

- No durable event store.
- No replay.
- No distributed bus.
- No plugin runtime.
