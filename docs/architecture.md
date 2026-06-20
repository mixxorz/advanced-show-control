# Backend Architecture Description

## 1.0 Purpose

This document defines the backend architecture for the Advanced Show Control application. The document identifies the runtime components, ownership boundaries, actor interfaces, event distribution model, lifecycle model, projection model, file organization conventions, and safety requirements.

The backend controls live mixer parameters. Therefore, this architecture treats state ownership, command routing, generation validation, lockout enforcement, and scene identity validation as safety-critical design constraints.

## 2.0 Scope

This document applies to the Rust backend located under `src-tauri/src/` in the `advanced-show-control` crate. It also defines the backend-to-frontend boundary used by the React and TypeScript user interface located under `ui/`.

This document does not define LV1 protocol semantics, user interface design requirements, show-file schema details, or mixer operating procedures except where those subjects affect backend architecture.

## 3.0 System Overview

The backend is an actor-oriented Rust and Tauri runtime. Each core domain owns its state within a module boundary. Other domains request work by sending explicit mailbox commands to the actor that owns the affected state or behavior.

The backend distributes state facts through `AppEventBus`. The backend sends command requests directly to the owning actor handle.

LV1 is the authoritative source for live console state. The application stores and executes application-managed scene fade behavior as an overlay on top of LV1 scene workflows.

The runtime consists of the following primary components:

| Component   | Responsibility                                                                                                                            |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `lv1`       | Maintains the LV1 TCP connection and raw LV1 state mirror.                                                                                |
| `fade`      | Executes active fade timing, overlap behavior, and LV1 parameter writes.                                                                  |
| `scenes`    | Performs scene recall automation and recall policy enforcement.                                                                           |
| `show`      | Maintains show document state, show-file input/output, discovery state, lockout state, and application-managed scene configuration state. |
| `lifecycle` | Constructs the connected runtime, wires actor peers, installs handles, tears down runtime state, and owns generation changes.             |
| `projector` | Maintains the backend-to-frontend projection cache and emits `app-status-changed`.                                                        |
| `ui`        | Performs Tauri setup and provides thin frontend command adapters.                                                                         |
| `runtime`   | Provides shared runtime events, error types, event bus primitives, and generation guards.                                                 |

## 4.0 Architectural Requirements

The backend architecture shall satisfy the requirements in this section.

### 4.1 State Ownership

1. Each module shall own the state for its domain.
2. A module shall not mutate state owned by another module.
3. A module shall expose only its intentional public interface through `mod.rs`.
4. Submodules shall remain private unless an external interface is required.
5. External modules shall import public items from the owner module root, not from private submodules.

### 4.2 Actor Boundary

1. Each actor module shall receive work through an explicit mailbox command enum.
2. Each actor handle shall be a cloneable mailbox sender.
3. Actor handles shall not hide domain operations behind convenience methods.
4. Call sites shall construct command enum variants explicitly.
5. A command caller shall include a `oneshot` reply channel when the caller requires a result.
6. Business logic shall reside in the owning actor or owning module.
7. Business logic shall not reside in Tauri command adapters or actor handles.

### 4.3 Event Distribution

1. The backend shall publish runtime facts through `AppEventBus`.
2. The backend shall send command requests to the actor that owns the requested behavior.
3. The backend shall not use broadcast events as requests.
4. Log-only diagnostics shall use `tracing`, not `AppEventBus`.
5. Consumers shall ignore generation-bearing events that do not match the active runtime generation.

### 4.4 Frontend Projection

1. The frontend shall receive backend application state only through projector-owned `app-status-changed` events.
2. Tauri command adapters shall not emit `app-status-changed`.
3. Tauri command adapters shall not construct partial `AppViewState` snapshots.
4. The projector shall be the only backend component that owns frontend state projection.

## 5.0 Actor Model

An actor module normally defines four interface concepts:

1. A command enum, such as `Lv1Command`, `FadeCommand`, `ScenesCommand`, or `ShowCommand`.
2. A handle, such as `Lv1ActorHandle`, `FadeEngineHandle`, `ScenesHandle`, or `ShowStateHandle`.
3. A task object, such as `Lv1ActorTask`, `FadeEngineTask`, `ScenesTask`, or `ShowActorTask`.
4. A peer-wiring object, when the actor requires direct access to other actors after construction.

The actor handle owns a Tokio sender. The handle shall remain dumb. It shall not provide domain-specific helpers that hide mailbox command construction.

The actor task owns the event loop and the state mutation path. The task receives commands, validates requests, updates owned state, publishes facts, and sends command replies.

## 6.0 Mailbox Command Model

Mailbox commands are acknowledged requests to one owning actor. A command variant may include a `oneshot::Sender` when the caller requires a response.

The standard command sequence is:

```text
caller
  -> construct command enum variant
  -> attach oneshot reply channel, when required
  -> send command through actor handle mailbox
  -> actor validates request
  -> actor performs work
  -> actor updates owned state, when required
  -> actor publishes facts, when state changes
  -> actor sends reply, when required
```

Mailbox commands shall not be broadcast.

A caller that requires LV1 state, fade state, show state, or scene recall behavior shall send a command to the actor that owns the required state or behavior.

Command failures that cross the user interface boundary shall map through `runtime::errors::AppCommandError`. Tauri commands shall return frontend-safe string errors.

## 7.0 Application Event Bus

`AppEventBus` is a Tokio broadcast bus for runtime facts. It carries `AppEvent` values from `runtime::events`.

`AppEvent` includes the following event families:

```rust
Runtime(RuntimeLifecycleEvent)
Lv1 { generation, event }
Fade { generation, event }
Scenes { generation, event }
Show(ShowEvent)
```

`AppEventBus` shall satisfy the following rules:

1. Events shall represent facts, not requests.
2. Event publication shall be non-blocking.
3. Subscribers shall operate independently.
4. The event bus shall not provide replay.
5. The event bus shall not provide durable event storage.
6. A lagged subscriber shall log lag and continue from the newest available event.
7. A consumer shall ignore generation-bearing events when the event generation does not match the active runtime generation.

## 8.0 Direct Peer Wiring

Actors that must call other actors receive peer handles through peer-wiring structures before actor tasks are spawned.

Peer installation is runtime construction work. It is not a mailbox command.

`lifecycle` performs peer installation while it owns the unspawned actor tasks.

The following peer relationships are defined:

| Actor    | Peer Handles Received                                                             | Purpose                                                                                                         |
| -------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `fade`   | `Lv1ActorHandle` through `FadeEnginePeers`                                        | Allows active fades to write parameters to LV1.                                                                 |
| `scenes` | `ShowStateHandle`, `Lv1ActorHandle`, and `FadeEngineHandle` through `ScenesPeers` | Allows recall automation to validate show state, validate LV1 state, send LV1 recall commands, and start fades. |
| `show`   | `Lv1ActorHandle` through `ShowActorPeers`                                         | Allows show-owned workflows to obtain fresh LV1 state.                                                          |

## 9.0 Runtime Lifecycle and Generations

`AppLifecycle` owns connected-runtime setup and teardown.

The connected-runtime startup sequence is:

```text
begin connecting
  -> increment active generation
  -> publish active generation changed
  -> build LV1 actor
  -> build fade engine
  -> build scenes actor
  -> wire actor peers
  -> install runtime handles, if generation is still current
  -> spawn actor tasks
  -> spawn projector, when frontend is ready
```

The generation model prevents stale asynchronous work from affecting a newer connection. Disconnect and reconnect operations advance the active generation.

Actors and projector consumers compare event generations against the active generation before they mutate current state or emit user-interface-visible state.

`RuntimeGeneration` is the shared asynchronous guard used by generation-sensitive tasks before they dispatch safety-critical work.

## 10.0 Tauri Command Adapters

Tauri commands reside under `src-tauri/src/ui/commands/`.

Tauri command modules are adapters. They are not business logic modules.

A Tauri command adapter shall perform the following functions:

1. Deserialize frontend arguments.
2. Obtain the required actor handle or lifecycle handle from Tauri state.
3. Construct the explicit command enum variant.
4. Create a `oneshot` reply channel when a reply is required.
5. Send the command through the actor mailbox.
6. Await the actor reply.
7. Map the reply into a frontend-safe result.

A Tauri command adapter shall not perform the following functions:

1. Mutate domain state directly.
2. Validate business rules that belong to an actor.
3. Emit `app-status-changed`.
4. Construct partial `AppViewState` snapshots.

The projector is the sole backend owner of `app-status-changed`.

## 11.0 Projector Cache and Frontend Emission

The projector converts backend facts into `AppViewState` for the React frontend. It subscribes to `AppEventBus` and to user-interface log events. It maintains a `ProjectionCache` and emits `app-status-changed` through Tauri.

The projector interval is defined as:

```rust
PROJECTOR_INTERVAL = 100 ms
```

This interval caps user-interface projection at 10 hertz.

Incoming facts mark the projection cache dirty. On each interval tick, the projector emits a new `AppViewState` only when the cache is dirty. After emission, the projector clears the dirty flag.

The projection cache applies facts incrementally as follows:

| Fact Source               | Projection Effect                                                                                                                   |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| LV1 facts                 | Update connection status, current scene, scene list, channels, faders, mutes, and pan-family values.                                |
| Fade facts                | Update frontend fade state.                                                                                                         |
| Show projection facts     | Update show-file state, discovery metadata, connection metadata, lockout state, and application-managed scene configuration fields. |
| User-interface log events | Append bounded frontend log entries.                                                                                                |
| Runtime generation facts  | Update the active generation used for event filtering.                                                                              |

The frontend shall listen only to `app-status-changed`. The frontend shall not subscribe directly to actor events or backend logs.

## 12.0 Module Responsibilities

### 12.1 `lv1`

The `lv1` module owns the LV1 TCP connection lifecycle and the raw LV1 mirror.

The module owns the following responsibilities:

1. TCP connect, read, and write behavior.
2. OSC frame encoding and decoding.
3. Raw channel topology.
4. Live parameter values.
5. Raw current scene state.
6. Raw LV1 scene list state.
7. LV1 write commands.
8. LV1 discovery helpers.

The module publishes `Lv1Event` facts and accepts `Lv1Command` requests.

### 12.2 `fade`

The `fade` module owns active fade execution.

The module owns the following responsibilities:

1. Fade timing.
2. Fade tick scheduling.
3. Fade target interpolation.
4. Fader-law conversion for fader targets.
5. Linear interpolation for pan-family targets.
6. Overlap behavior.
7. Same-scene behavior.
8. Manual override cancellation.
9. Fade abort behavior.
10. Disconnect safety behavior.

The module publishes `FadeEvent` facts and accepts `FadeCommand` requests.

### 12.3 `scenes`

The `scenes` module owns scene recall automation.

The module owns the following responsibilities:

1. Recall policy decisions.
2. Recall-trigger handling from LV1 scene facts.
3. Recall request validation using fresh LV1 state and show state.
4. Dispatch of validated LV1 recall commands through wired peers.
5. Dispatch of validated fade-start commands through wired peers.
6. Recall status facts for skipped, blocked, and started recall outcomes.

The module publishes `ScenesEvent` facts and accepts `ScenesCommand` requests.

### 12.4 `show`

The `show` module owns show-level application state and persistence.

The module owns the following responsibilities:

1. Show document state.
2. Application-managed scene configuration data.
3. Selected scene state.
4. Cued scene state.
5. Scene scope toggles.
6. Scoped channel configuration.
7. Scene capture from current LV1 state.
8. Show-file import and export.
9. Show-file path metadata.
10. Show-file dirty state.
11. Show-file save timestamps.
12. Lockout state.
13. LV1 discovery metadata projected to the user interface.
14. LV1 connection metadata projected to the user interface.

The module publishes `ShowEvent` facts and accepts `ShowCommand` requests.

### 12.5 `lifecycle`

The `lifecycle` module owns runtime lifetime.

The module owns the following responsibilities:

1. Active generation tracking.
2. Connection startup transactions.
3. Teardown transactions.
4. Actor construction.
5. Actor peer wiring.
6. Runtime handle installation.
7. Runtime handle cleanup.
8. Reconnect state changes that cross actor boundaries.

### 12.6 `projector`

The `projector` module owns frontend state projection.

The module owns the following responsibilities:

1. `ProjectionCache`.
2. `AppViewState` construction.
3. `app-status-changed` emission.
4. 10 hertz dirty-cache throttling.
5. User-interface log cache entries.

### 12.7 `ui`

The `ui` module owns Tauri setup and frontend command boundaries.

The module owns the following responsibilities:

1. Tauri command registration.
2. Dialog integration.
3. Frontend serialization boundaries.
4. Thin command adapter modules.

### 12.8 `runtime`

The `runtime` module owns shared runtime primitives.

The module owns the following responsibilities:

1. `AppEventBus`.
2. `AppEvent`.
3. Runtime lifecycle events.
4. Runtime generation guard state.
5. User-interface-facing command error types.

## 13.0 Module File Conventions

Actor and domain modules shall use consistent file names.

| File                | Required Content                                                                                                |
| ------------------- | --------------------------------------------------------------------------------------------------------------- |
| `mod.rs`            | Declares private submodules and re-exports only the intentional public facade.                                  |
| `actor.rs`          | Defines actor construction, task type, runtime loop, command handling, and peer dependency use.                 |
| `commands.rs`       | Defines the mailbox command enum and command-specific reply or result data transfer objects.                    |
| `handle.rs`         | Defines the dumb cloneable mailbox sender. This file shall not contain hidden business methods.                 |
| `events.rs`         | Defines facts published on `AppEventBus`.                                                                       |
| `state.rs`          | Defines actor-owned state and pure state-change functions.                                                      |
| `types.rs`          | Defines domain types that are not commands, events, or actor state.                                             |
| `policy.rs`         | Defines named decision logic for modules that contain non-trivial policy rules.                                 |
| Narrow helper files | Define focused subdomains owned by the module, such as `capture.rs`, `show_file.rs`, `tcp.rs`, or `parsers.rs`. |

The backend shall observe the following boundary conventions:

1. Submodules shall remain private unless a concrete external need exists.
2. Other modules shall import from the owner module root.
3. Other modules shall not import from private submodules.
4. Test-only constructors and raw internals should be protected with `#[cfg(test)]` when practical.
5. Re-export bridges shall not weaken module boundaries.
6. Domain logic shall not be placed in `ui/commands/*`.
7. Convenience helpers shall not hide the command enum and reply channel pattern.

## 14.0 Safety Requirements

The application controls live mixer faders. The backend shall implement the following safety requirements.

1. The backend shall not send fader commands when LV1 state is unavailable.
2. The backend shall not send fader commands when LV1 is disconnected.
3. The backend shall not send fader commands when LV1 state is stale.
4. The backend shall not send fader commands when LV1 state is unsafe.
5. The backend shall not bypass generation guards.
6. The backend shall not bypass lockout checks.
7. The backend shall not bypass exact scene identity validation.
8. Scene recall automation shall validate the recall request before it aborts an existing fade.
9. A blocked recall shall not abort an existing fade.
10. A skipped recall shall not abort an existing fade.
11. A disabled recall shall not abort an existing fade.
12. Recall automation shall use fresh LV1 state when event subscriber ordering could otherwise produce stale decisions.
13. The backend shall make safety blocks visible through logs, facts, or projected user-interface state.
14. The backend shall preserve manual override behavior.
15. The backend shall preserve fade abort behavior.
16. The backend shall preserve overlap behavior.
17. The backend shall preserve same-scene behavior.
18. The backend shall preserve disconnect behavior.

## 15.0 Backend File Structure

Rust backend code resides under `src-tauri/src/` in the `advanced-show-control` crate.

Important directories are defined as follows:

| Directory                  | Contents                                                                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/lv1/`       | LV1 protocol, TCP actor, discovery, mirror state, and LV1 commands.                                                             |
| `src-tauri/src/fade/`      | Fade engine actor, fade state, interpolation, fader law, and fade events.                                                       |
| `src-tauri/src/scenes/`    | Scene recall actor, recall commands, recall events, and recall policy.                                                          |
| `src-tauri/src/show/`      | Show actor, show document state, scene configuration state, show-file input/output, discovery state, and show projection facts. |
| `src-tauri/src/lifecycle/` | Runtime connection lifecycle and actor graph wiring.                                                                            |
| `src-tauri/src/projector/` | Projection cache, application view model, and 10 hertz frontend emission loop.                                                  |
| `src-tauri/src/ui/`        | Tauri setup and command adapters.                                                                                               |
| `src-tauri/src/runtime/`   | Event bus, runtime errors, and generation guards.                                                                               |
| `ui/`                      | React and TypeScript frontend.                                                                                                  |

## 16.0 Explicit Non-Goals

The backend architecture does not provide the following capabilities:

1. Durable event storage.
2. Event replay.
3. A distributed event bus.
4. Frontend-owned backend state projection.
