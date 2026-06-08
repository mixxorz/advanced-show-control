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
- `SceneRecallFader` listens for LV1 scene recall events and starts app-managed scoped fader fades after safety validation.

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

`LV1 TCP -> Lv1Actor -> AppEventBus -> ShellState / FadeEngine / SceneRecallFader`

```text
┌─────────┐     ┌──────────┐     ┌─────────────┐
│ LV1 TCP │ ──▶ │ Lv1Actor │ ──▶ │ AppEventBus │
└─────────┘     └──────────┘     └──────┬──────┘
                                        │
                    ┌───────────────────┼───────────────────┐
                    │                   │                   │
                    ▼                   ▼                   ▼
          ┌──────────────────┐   ┌────────────┐   ┌──────────────────┐
          │ ShellState       │   │ FadeEngine │   │ SceneRecallFader │
          │ projector        │   │            │   │                  │
          └────────┬─────────┘   └─────┬──────┘   └────────┬─────────┘
                   │                   │                   │
                   ▼                   ▼                   ▼
          ┌──────────────────┐   ┌────────────┐   ┌──────────────────┐
          │ UI snapshots     │   │ Override + │   │ Scene recall     │
          │ and logs         │   │ disconnect │   │ automation       │
          └──────────────────┘   └────────────┘   └──────────────────┘
```

`Lv1Actor` translates incoming LV1 traffic into LV1 events. Those events are broadcast to the rest of the runtime. `ShellState` projects them into UI state, `FadeEngine` reacts to relevant LV1 changes, and `SceneRecallFader` reacts to scene recall events.

Broadcast subscribers receive events independently. A subscriber must not assume another subscriber has already processed the same event. For scene recall automation, `SceneRecallFader` asks `AppCommandBus` for a fresh LV1 state snapshot before validating the recall so it is not dependent on shell projector ordering.

## Command Flow

`Tauri / FadeEngine / SceneRecallFader / future automation -> AppCommandBus -> current LV1 / fade handles`

```text
┌────────────────┐   ┌────────────┐   ┌──────────────────┐
│ Tauri commands │   │ FadeEngine │   │ SceneRecallFader │
└───────┬────────┘   └─────┬──────┘   └────────┬─────────┘
        │                  │                   │
        └──────────────────┼───────────────────┘
                           │
                           ▼
                   ┌───────────────┐
                   │ AppCommandBus │
                   └───────┬───────┘
                           │
             ┌─────────────┴─────────────┐
             │                           │
             ▼                           ▼
      ┌─────────────┐             ┌────────────────────┐
      │ Current LV1 │             │ Current FadeEngine │
      │ set/state   │             │ start/abort/finish │
      └─────────────┘             └────────────────────┘
```

`AppCommandBus` keeps the current LV1 and fade targets and sends commands directly to them. Tauri commands use the bus, `FadeEngine` uses it for LV1 writes, and `SceneRecallFader` uses it to read fresh LV1 state, abort the previous fade after a valid recall, and start the new scene fade.

## Runtime Lifecycle

- `connect` installs the current command targets and starts the actor, fade, scene recall fader, and projector tasks.
- `disconnect` and reconnect clear command targets and abort the old runtime tasks.
- Generation guards prevent stale events, snapshots, or handles from mutating current state.

The shell-state projector only applies events for the active generation, and stale runtime handles are rejected instead of being installed. `SceneRecallFader` also checks the active generation before validation, before logging automation status, and immediately before dispatching a fade start so a stale recall task cannot send after disconnect or reconnect.

## Automation Boundary

Automation should depend on `AppEventBus` and `AppCommandBus`, not on concrete actor internals. That keeps automation aligned with the same runtime contract used by LV1, fading, and the Tauri shell.

`SceneRecallFader` is the first automation runtime. It owns the LV1-scene-recall-to-fade bridge only:

```text
┌────────────────────────┐
│ Lv1Event::SceneChanged │
└───────────┬────────────┘
            │
            ▼
┌──────────────────┐
│ SceneRecallFader │
└──────────┬───────┘
           │
           ▼
┌──────────────────────────────┐
│ Get fresh LV1 snapshot       │
│ through AppCommandBus        │
└──────────┬───────────────────┘
           │
           ▼
┌───────────────────────┐
│ ShellState validation │
└──────────┬────────────┘
           │
     ┌─────┴─────┐
     │           │
     ▼           ▼
┌────────────┐  ┌──────────────┐
│ Block/skip │  │ Valid recall │
└─────┬──────┘  └──────┬───────┘
      │                │
      ▼                ▼
┌───────────────┐  ┌─────────────────────┐
│ Log + refresh │  │ Abort previous fade │
└───────────────┘  └──────────┬──────────┘
                              │
                              ▼
                       ┌───────────────────┐
                       │ Start scoped fade │
                       └─────────┬─────────┘
                                 │
                                 ▼
                       ┌───────────────────┐
                       │ Logs + UI refresh │
                       └───────────────────┘
```

- It listens for `Lv1Event::SceneChanged`.
- It validates exact scene identity, lockout state, connection state, stored scene config, scoped targets, stored fader values, and live topology through `ShellState`.
- It treats duration `0` scenes as disabled for automatic fades.
- It aborts the previous fade only after the incoming recall validates.
- It starts fades from current live fader values through `FadeEngine`.
- It publishes automation refresh events after automation logs so the UI receives an updated snapshot even when no fade event follows.

Future HTTP, WebSocket, and Companion automation should reuse the same command and validation boundaries instead of bypassing safety checks.

## Non-Goals

- No durable event store.
- No replay.
- No distributed bus.
- No plugin runtime.
