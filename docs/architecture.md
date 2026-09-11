# Backend Architecture

## Purpose and Scope

Advanced Show Control is a Rust/Tauri fader-fade overlay for LV1. LV1 remains authoritative for scene creation, scene recall, and normal console state. ASC owns fade metadata and moves only configured fader and pan-family controls. Because it controls live faders, ownership, generation guards, lockout, and exact-scene validation are safety boundaries.

The Rust backend is `src-tauri/src/`; the React/TypeScript frontend is `ui/`.

## Runtime Ownership

| Component   | Lifetime and responsibility                                                                                                                     |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `lv1`       | Generation-scoped actor. Owns TCP transport/reconnect, OSC, and the LV1 live-state mirror.                                                      |
| `fade`      | Generation-scoped actor. Owns fade timing, interpolation, readiness, override, abort, overlap, and writes.                                      |
| `scenes`    | One app-lifetime actor/document. Owns configs, selection, clipboard, scene-library reconciliation, capture/link/edit, recall policy, and queue. |
| `cue_lists` | Synchronous domain component inside the Scenes actor. Holds cue documents and active/cued entries; has no task, peers, or event subscription. |
| `show`      | App-lifetime actor. Owns show-file metadata/dirty state, lockout, discovery/connected-LV1 metadata, and persistence orchestration.              |
| `settings`  | App-lifetime actor. Owns app settings and private remembered LV1 identity in app-config `settings.json`.                                        |
| `lifecycle` | Owns connection-generation transitions and generation-scoped peer installation/removal.                                                         |
| `projector` | App-lifetime `AppViewState` cache and the sole `app-status-changed` emitter.                                                                    |
| `runtime`   | Owns `AppEventBus`, lifecycle facts, generation guards, and frontend-safe command errors.                                                       |
| `ui`        | Tauri setup and thin command adapters.                                                                                                          |

## Commands and Facts

Native File menu actions call the same Tauri command functions used by the frontend. Dialog behavior, mailbox dispatch, and error mapping have one implementation in `ui/commands/show.rs`.

Actors receive explicit mailbox command enums. Scenes, Cue Lists, Settings, and Fade handles are typed Tokio senders, not forwarding wrapper objects. The app-lifetime Scenes handle is always available from lifecycle; only its connection-dependent operations can be unavailable. Shared adapter helpers own request/reply plumbing while call sites still construct explicit command variants. A caller attaches a `oneshot` reply only when it needs a result. Business logic and validation belong to the owning actor, not a handle or Tauri adapter.

`AppEventBus` is a non-blocking Tokio broadcast bus for facts, never requests. It has no replay or durable storage. Its families are:

```text
Runtime(ActiveGenerationChanged)
Lv1 { generation, event }
Fade { generation, event }
Scenes { generation, event }
CueLists(event)
Show(event)
Settings(event)
```

LV1 and Fade facts are generation-bound and consumers ignore stale generations. Scenes facts carry a generation for runtime context, but their document is app-lifetime; projector and Show do not discard valid document facts solely because of that tag. Cue Lists, Show, and Settings facts are app-lifetime.

`Lv1Event::PingReceived { sequence }` is an operational keepalive fact: it drives post-recall Fade readiness and is not frontend state. `SceneObservation { sequence, scene }` is a connection-local sequence. It identifies an observation occurring _after_ an ASC recall dispatch; it is not a durable scene ID or general ordering guarantee.

## Lifecycle, Connections, and Peers

`AppLifecycle` advances `RuntimeGeneration` for every explicit connect, disconnect, and teardown transaction. A connect transaction:

1. advances and publishes the active generation;
2. constructs LV1 and Fade for that generation and installs their handles only if still current;
3. starts LV1/Fade, confirms a connected initial LV1 snapshot, and updates connected-LV1 metadata;
4. installs Scenes' accepted generation peers; then
5. sends `ScenesCommand::RuntimePeersReady` with the initial scene list.

`Scenes` is created once at app startup with an event subscription, `SettingsHandle`, initial settings, and `ShowLockoutReader`. The lockout reader is a latest-value dependency, avoiding a reverse Show mailbox dependency. Scene recall refreshes settings at settled observation boundaries and fails closed if settings are unavailable.

Direct peers are intentional:

- `FadeEngine` requires its `Lv1ActorHandle` when constructed and sends `Lv1Command::WriteBatch` directly. This immutable, generation-scoped dependency has no optional peer slot, installation step, or peer mutex.
- `Scenes` receives the active generation's `Lv1ActorHandle` and `FadeEngineHandle` after lifecycle acceptance.
- `Show` holds app-lifetime Scenes/Cue Lists peers and the current LV1 peer only while connected.
- Scenes and Cue Lists have separate bounded command endpoints, processed by the same app-lifetime owner. Neither sends mailbox requests to the other.

Lifecycle runs multicast discovery on a blocking I/O worker, then sends only the resulting system list to Show. A discovery-only mutex serializes refreshes so older results cannot overwrite newer ones; it is independent of connection transitions and Show's mailbox. Lockout commands and generation changes remain responsive while discovery waits on the network. Startup and frontend discovery share this path.

`Lv1Actor` owns transport reconnect within its assigned generation. A transport failure clears connection-dependent live state, publishes `Disconnected`, and retries after its reconnect delay. The frontend requests explicit connect/disconnect only; it owns neither transport reconnect nor connection generations.

## Scenes Library and Recall

Scenes preserves its document—durable config UUIDs, selection, and settings clipboard—across disconnects and generations. Its LV1-derived runtime library is explicitly:

1. `AwaitingPeers`: no accepted LV1/Fade peers for the active generation.
2. `AwaitingSceneList`: peers exist but no authoritative scene list exists.
3. `Ready`: accepted peers and the active generation's scene list exist.

Reconnect clears the runtime library and recall tracking but not the document, selection, or clipboard. Recall, capture/store-from-current-LV1, and link-to-current-LV1-scene require `Ready`; document-only edits remain available.

Scenes owns an eight-request FIFO for ASC-originated explicit recalls. Each caller reply remains held until that request actually dispatches, rather than merely entering the queue. After an LV1 recall dispatch, the queue requires an exact matching scene observation with a later `SceneObservation.sequence`, then Fade readiness: two newer `PingReceived` facts in the same generation. One five-second deadline spans observation and readiness. Timeout, disconnect, lockout, generation change, or unsafe recovery cancels queued intent; a bounded late-canceled-observation record suppresses late matching observations, with a five-second fail-closed suppression fallback on overflow.

The in-flight recall owns its Fade readiness receiver directly. The Scenes event loop polls that receiver alongside commands and LV1 facts; canceling the recall drops the receiver. No detached completion-forwarding task or intermediate completion queue survives cancellation. Skipped and blocked observations share the same readiness handoff while retaining distinct diagnostic outcomes.

A recall is validated with fresh LV1 state, generation, lockout, exact scene index/name, linked config, live topology, scopes, and stored targets before Fade is admitted. A genuinely blocked, skipped, or disabled pre-admission recall does not abort an active fade. Once a recall is validated and admitted—including no-target, disabled-scope, or zero-duration cases—it enters the readiness protocol; a readiness timeout aborts paused fades and cancels queued recall intent.

A repeated exact-scene recall is identified by the exact LV1 index/name retained with active targets. With same-scene finishing enabled, matching active targets finish after readiness. With it disabled, matching targets restart from their current interpolated or live values for the configured full duration. Both modes use the same generation-wide two-ping readiness barrier.

Fade feedback remains active during readiness. A manual fader override beyond the fader-law position threshold cancels that fader target; pan requires confirmed consecutive deviations, while balance and width feedback do not cancel targets. A final manual cancellation produces a terminal fade completion. Disconnect, explicit Abort All, and generation change cancel active or paused fades.

## Show, Cue Lists, and Persistence

`Show` does not own scene configs, selection, clipboard, or cue-list documents. It owns show-file path/name, dirty state, save timestamp, lockout, discovery, and connected-LV1 metadata. Persisted Scenes/Cue Lists edits publish `persisted_*_edit: true`; Show observes these app-lifetime facts without generation filtering, marks dirty, and publishes file metadata. On Show event-bus lag it conservatively marks the file dirty.

New and load require a currently connected LV1 snapshot to initialize or align the scene document against the live scene list. Save does not require LV1, but queries the current app-lifetime Scenes and Cue Lists documents before writing and then marks the show saved. File replacement is not inherently dirty; load marks dirty for import normalization, generated IDs, scene alignment, or cue reconciliation.

Scenes reconciles configs and directly reconciles cue references when the owned scene identities change. Cue entries survive missing scenes; invalid active/cued references are cleared. Projected scene facts cannot mutate the cue document, and there is no cue subscriber, generation cache, or lag-recovery query. For `FileReplacement`, Show still explicitly supplies both documents and the imported scene UUID set.

Cue recall enters the existing recall queue locally. The owner polls its dispatch reply without a forwarding task, advances only after successful LV1 dispatch, and keeps subsequent cue commands bounded in their mailbox until completion. Scene commands and runtime safety events continue to be processed while a cue awaits queued dispatch.

Settings loads normalized defaults or persisted values from `settings.json`, saves changed full-object replacements immediately, and publishes `SettingsEvent::StateChanged`. Remembered LV1 identity is private metadata in the same file and is accessed by lifecycle through dedicated commands, not projected as public settings.

## Projection and Frontend Boundary

The projector applies facts to `ProjectionCache`, accepts generation-bound LV1/Fade state only for its active generation, and emits dirty snapshots at most every 100 ms. It receives UI log input from the tracing UI sink; `INFO`, `WARN`, and `ERROR` become bounded frontend log entries, while runtime modules use `tracing` rather than facts solely for logging.

Every emitted `AppViewState` has a monotonically increasing `state_version`. The frontend applies a snapshot only when its version is newer than the latest accepted version; command responses, polling, and event delivery may arrive out of order and must not overwrite newer UI state.

On event-bus lag, the projector drains retained facts, resets generation-bound cache state, obtains an authoritative connected LV1 snapshot when possible, and mailbox-resynchronizes Show, Scenes, Cue Lists, and Settings. This restores app-lifetime state even when generation-scoped facts were lost.

## Debug Smoke Boundary

The debug Tauri app is development-only. Its JavaScript runner uses production Tauri commands for discovery, connection, show creation, scene configuration, scope/duration, recall, lockout, settings, and cue-list workflows; it validates projected snapshots and live LV1 fader values. Debug-only commands are limited to smoke report/exit and deterministic setup or observation unavailable to production commands, such as raw LV1 recall and test-channel gain access.

`make smoke` requires LV1-compatible hardware. Its terminal output is not authoritative: always inspect `logs/debug-smoke-report.txt` for the suite result.

## Safety Requirements

- Never send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Never bypass generation guards, lockout, fresh-state validation, or exact scene identity.
- Validate before Fade admission; preserve the pre-admission no-abort rule and the admitted-recall readiness timeout behavior.
- Preserve manual override, abort, overlap, exact same-scene, and disconnect behavior.
- Make blocked or unsafe outcomes visible through facts, projected state, or complete `tracing` messages.

## Module Layout

Public facades live in each `mod.rs`; submodules remain private unless externally required. Import public items from the owner module root. Typical actor modules use `actor.rs`, `commands.rs`, `handle.rs`, `events.rs`, `state.rs`, and `types.rs`, plus focused helpers such as `scene_alignment.rs` and `tcp.rs`.
