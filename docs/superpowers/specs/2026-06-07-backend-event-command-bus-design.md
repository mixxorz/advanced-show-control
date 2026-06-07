# Backend Event And Command Bus Design

## Purpose

The backend needs to support automatic LV1 scene recall triggers now and a broader automation system later. The current architecture already uses local actors, but modules still communicate through concrete handles and actor-specific subscriptions. This design introduces two shared runtime abstractions:

- `AppEventBus` for notifications about facts that already happened.
- `AppCommandBus` for acknowledged requests routed to the module that owns the work.

The goal is to let future modules, especially automation, react to events and issue commands without knowing about `Lv1ActorHandle`, `FadeEngineHandle`, or other concrete module handles.

## Architecture

The backend will keep the actor model, but route communication through buses.

```text
Lv1Actor
  receives routed commands
  publishes LV1 events

FadeEngine
  receives routed commands
  subscribes to LV1 events
  sends LV1 commands through AppCommandBus
  publishes fade events

AutomationEngine
  subscribes to app events
  sends commands through AppCommandBus
  publishes automation events

ShellStateProjector
  subscribes to app events
  updates ShellState
  emits AppViewState to Tauri UI

Tauri commands
  send commands through AppCommandBus
  read ShellState snapshots for UI responses

Runtime dispatcher
  owns concrete actor handles
  routes AppCommand variants to their owning actors
```

Concrete actor handles should be contained in the runtime dispatcher layer. Feature modules should depend on `AppEventBus` and `AppCommandBus`, not on each other.

## Event Bus

`AppEventBus` is for facts. It should be backed by `tokio::sync::broadcast` so publishers never wait for subscribers.

Events should be past-tense notifications, for example:

```rust
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    Automation(AutomationEvent),
    CommandFailed { command: String, message: String },
}
```

Initial delivery semantics:

- Publishing is non-blocking.
- Subscribers receive events independently.
- Slow subscribers may miss events.
- Missed events are logged with the number of skipped events.
- No replay, recovery, or durable event log is required for the first version.

Subscriber loops should handle lag explicitly:

```rust
match rx.recv().await {
    Ok(event) => handle(event).await,
    Err(broadcast::error::RecvError::Lagged(count)) => {
        log::warn!("event subscriber lagged and missed {count} events");
    }
    Err(broadcast::error::RecvError::Closed) => break,
}
```

Recovery behavior can be added later if lag becomes a real problem.

## Command Bus

`AppCommandBus` is for requests. Commands are not broadcast. They are sent through an `mpsc` channel to a runtime dispatcher, which routes each command to the module that owns it.

Commands should be acknowledged by default. Even commands that look fire-and-forget should return at least `Result<(), AppCommandError>` after the owning actor accepts or processes them.

Example command shape:

```rust
pub enum AppCommand {
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    GetLv1State {
        reply: oneshot::Sender<Result<Lv1StateSnapshot, AppCommandError>>,
    },
    StartFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAllFades {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishFadeNow {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}
```

The public command bus should expose ergonomic methods so callers do not construct oneshot channels manually:

```rust
command_bus.set_gain(group, channel, gain_db).await?;
command_bus.start_fade(config).await?;
command_bus.abort_all_fades().await?;
```

## Runtime Dispatcher

The dispatcher is the only layer that owns concrete actor handles such as `Lv1ActorHandle` and `FadeEngineHandle`.

Responsibilities:

- Keep track of optional runtime handles.
- Route LV1 commands to `Lv1Actor`.
- Route fade commands to `FadeEngine`.
- Return a clear `AppCommandError` if the target actor is unavailable.
- Publish command failure events when useful for logs/UI.

This keeps automation and future modules independent from concrete actors while preserving explicit command ownership.

## ShellState Interaction

`ShellState` remains the mutable UI projection state. It should not be the event bus. Instead, a `ShellStateProjector` task subscribes to `AppEventBus`, applies events to `ShellState`, and emits `AppViewState` to the Tauri frontend.

The existing generation guard for LV1 connections should remain or move into the runtime layer so stale connection events cannot mutate current state.

`ShellState` should eventually receive both LV1 and fade events so `fade_state` reflects actual fade engine state rather than command intent.

## Command And Event Separation

Commands and events must stay separate.

Commands are requests:

```text
Set gain
Start fade
Abort all fades
Finish fade now
```

Events are facts:

```text
LV1 connected
Scene changed
Fader changed
Fade started
Fade completed
Automation rule triggered
```

This avoids ambiguous event handlers, duplicate command execution, and unclear success/failure reporting.

## Phasing

Implementation should be incremental:

1. Add `AppEventBus` and `AppCommandBus` types.
2. Add a runtime dispatcher that owns concrete actor handles.
3. Route Tauri commands through `AppCommandBus`.
4. Change `Lv1Actor` to publish LV1 events to `AppEventBus` instead of owning subscriber lists.
5. Change `FadeEngine` to consume LV1 events from `AppEventBus` and send LV1 commands through `AppCommandBus`.
6. Move ShellState projection to an event-bus subscriber task.
7. Add an architecture document that explains backend modules, event flow, command flow, ownership boundaries, and the command/event distinction.
8. Add the automatic scene recall trigger on top of the buses.
9. Defer durable event logs, replay, and lag recovery until there is evidence they are needed.

## Non-Goals

- No durable event store in the first version.
- No replay or recovery for missed events in the first version.
- No distributed messaging or cross-process bus.
- No generic plugin framework.
- No automation rule engine as part of the bus refactor itself.

## Testing

Tests should cover the bus and routing behavior before adding automation features:

- Event publishing does not require subscribers.
- Subscribers receive published events.
- Lagged subscribers log and continue.
- Command bus returns an error when the target actor is unavailable.
- Command bus routes LV1 commands to the LV1 handler.
- Command bus routes fade commands to the fade handler.
- ShellState projector applies LV1 and fade events into `AppViewState`.
- FadeEngine can react to LV1 events from the bus and send LV1 commands through the command bus.
- The architecture document matches the implemented module boundaries and command/event flow.

## Open Future Extensions

The design leaves room for later additions:

- Durable automation event log.
- Event replay for automation debugging.
- Command authorization or lockout checks in the dispatcher.
- Domain-level automation events separate from raw LV1 events.
- Coalesced state channels for high-volume telemetry if fader events become too noisy.
