# Explicit state machines

This inventory describes the production state owners introduced by the explicit-FSM refactor,
including existing explicit machines that remain authoritative. The audit identifiers are stable
cross-references, not a count of independent global states: several protocols span multiple owners.

A phase owns the data or resource it needs. Events and successful operations perform transitions;
a presentation label is not an additional authority. Independent dimensions remain separate
machines (for example, fade readiness versus target timing, or current cue versus next cue).
Persisted show/settings data, command interfaces and complete projection schemas are unchanged.

## Behavioral boundaries

- LV1 remains authoritative for live console state. Generation, exact identity, freshness,
  mailbox admission, manual override and readiness checks remain in their owning domains.
- SAFE blocks new ASC recall/GO and automatic fade admission; it is not an emergency stop.
- Immediate Recall → Open/New and SAFE changes during a session dialog remain unsupported.
  This refactor adds no global session epochs, rollback protocol or UI replay queue.
- Completion of a command, an admitted write, and its eventual projected presentation are
  distinct facts. Resource ownership is consumed exactly once where the original contract requires it.
- Small existing ownership/typestate representations are retained when an additional phase
  field would duplicate their authority. These cases are identified below.

## Fade

Source: `app/src/fade/{actor,state,tick}.rs`.

- F1 aggregate lifecycle: `FadeExecution::{Idle,Running(Interval),Completed,Aborted}` replaces the independent optional ticker and completion boolean. Admission starts a new running epoch; targetless admission stops ticking without reopening an already-completed epoch; completion consumes ticker and emits once; disconnect/revocation/readiness timeout/write failure abort; a write failure preserves an already-completed latch exactly as before. Readiness remains an orthogonal state, including targetless readiness and readiness surviving manual override. New pure test `completion_epoch_reopens_after_reset_or_abort`; existing actor completion/abort tests retained.
- F2 readiness: `ReadinessProgress::{AwaitingFirst{after},AwaitingSecond{after},Lagged{observed}}` replaces count+last-sequence+lag boolean. Generation/deadline are checked before the transition; second newer ping consumes barrier, resumes targets and completes owned waiter. Lag preserves observed count but cannot release; replacement starts first-ping state. New pure test `readiness_requires_two_newer_sequences_and_lag_cannot_release`; existing actor stale/duplicate pings, timeout, lag, reset, readiness-only and post-override tests retained. Barrier absence and ownership remain natural Option resource states, not duplicated classifier fields.
- F3 target timing: stored `InterpolationClock::{Running,Paused{since}}` and orthogonal `FinishIntent::{AtDuration,OnNextTick}` replace pause Option+finish bool. First pause wins; resume rebases elapsed time; finish survives pause. New pure `finishing_survives_repeated_pause_and_resume`; existing interpolation and pause tests retained.
- F4 override: stored `OverrideEvidence::{Tracking,OneDeviation,Confirmed}` replaces numeric count, tracking/reset/two-consecutive transitions. Confirmation count for diagnostic logs is derived from FSM rather than separately stored. Existing pan consecutive/reset and fader immediate tests retained; no Pan/Balance/Width cancellation policy changes.
- F5 write admission: `WriteAdmission::{Idle,WaitingForCapacity(PendingWrite)}` owns the single future/proposal until consuming take transitions back to Idle. Pending token retains Tick vs ZeroDuration kind, deadline and reply ownership. No expected value mutates before checked reservation admission. On readiness/error/feedback cancellation the state is consumed and dropped. Existing actor mailbox-capacity, intervening override, stale-generation, exact-final-value and rejected-write tests retained. Box payload keeps enum size bounded; no future copying.
- F6 snapshot/fact protocol: `SnapshotAcquisition::{AwaitingSnapshot,DrainingFacts{result,remaining}}` stores the successful or failed result and bounded fact-prefix count; only DrainingFacts can release a result. Awaiting continues feedback/readiness deadlines, stale-generation result exits immediately, drained lag poisons `FactContinuity::{Intact,Lagged}`; cancellation returns terminal error. Existing snapshot wait/lag/readiness progress actor tests retained.
- F7 keyed overlapping ownership: target's stored F3 timing/finish state plus immutable scene+key already identifies its current owner; added `EngineState::install_target` and `release_target` as explicit key replacement/removal transitions. Finish transitions owned targets via existing `finish_scene_on_next_tick`. No duplicate ownership enum/history introduced. Existing same-scene Finish/Override, partial overlap, start-value preference and zero-duration actor tests retained.



## Scenes

Source: `app/src/scenes/`.

| Audit | Implementation and transitions | Verification |
|---|---|---|
| S1 full recall | `PendingRecallOperation` is now a stored enum: Admission, DispatchSnapshot, DispatchCommand, ObservationSnapshot, ObservationHandoff. Each variant owns its token, generation and correctly typed future. `complete` consumes the exact variant into `CompletedRecallOperation`; no impossible context/result pairing or inconsistency fallback remains. Idle is absence of an owned operation; FIFO and existing `InFlightPhase` continue through observation/readiness/interval. Cancellation consumes a typed cancellation context and drops the wait. | Existing actor tests for queued dispatch, stale generation, disconnect during admission/dispatch/handoff, readiness failures and mailbox responsiveness retained.  |
| S2 observation settle | Stored `ObservationSettle::{Idle,Settling(payload)}` owns replace, deadline and one-shot take. Replacement resets deadline; take/cancel returns Idle. | New pure test `newer_observation_restarts_settle_and_is_consumed_once`; existing cancellation tests retained. |
| S3 canceled observation tombstones | Stored `LateObservationState::{Retained(identity,expiry),Consumed,Expired}`. Matching exact later observation consumes once; TTL purge transitions to Expired then drops; capacity still evicts oldest at eight. | New pure `late_observation_is_consumed_once_and_expires_at_the_boundary`; existing capacity/exact identity test retained. |
| S4 scene-list suppression | Replaces independent cached-list and timestamp options with `SceneListTracking::{Unobserved,Stable(list),Suppressed{scenes,until}}`. First list establishes baseline; changed list restarts deadline; equal list preserves deadline. Timestamp remains available after wall-clock expiry because delayed observations must be checked at their original seen-at time. Session replacement clears only suppression; runtime loss clears baseline. | New pure `identical_late_resend_preserves_suppression_for_an_earlier_observation`; existing first/equal/changed/reset tests retained. |
| S5 store-from-LV1 | Stored `StoreState::{Idle,Fetching(PendingSnapshotOperation)}`. Fetching owns generation, target UUID, reply and future; guarded completion or cancellation consumes operation and returns Idle before mutation. Existing freshness/readiness checks remain authoritative. | Existing actor store blocked/disconnect/generation and responsiveness tests retained. |
| S6 lag recovery | Stored `LagRecoveryState::{Idle,Refreshing(PendingLagRecovery)}`. New lag replaces refresh; disconnect/generation cancels; completion consumes and conditionally accepts snapshot only with exact generation + scene-list revision + current peers. | Existing lag-recovery actor tests retained. |
| S7 linked identity | **Retained adequate simple representation:** nullable scene_index is already the authoritative linked/unlinked data state, paired with a preserved last-known name. Centralized real transitions as `SceneConfig::link_to` / `unlink`, used by conservative alignment and operator link. No duplicate enum/classifier or public/persisted-schema break. Existing exact/unique-name/single-rename alignment machine retained. | New pure `link_transitions_preserve_config_identity_and_unlinked_name`; existing serialization and alignment cases retained. |
| S8 clipboard | Stored `ClipboardState::{Empty,Copied(contents)}` owns copy/change detection and content read. Session replacement -> Empty, disconnect unchanged, paste preserves Copied. | Existing copy/paste/no-op/session-reset/disconnect tests retained. |
| S9 command lanes | Stored `CommandStream::{Open,Closed}` per scene/cue/lockout stream; stored `SceneCommandLane::{Ready,Deferred(command)}` for one held request. Existing phase guards retain serial mailbox ordering, shutdown preemption and independently closed streams. | Existing actor behavior tests retained; no redundant framework polling tests added. |


## Runtime, connection and projection

Source: `app/src/lifecycle/, app/src/lv1/, app/src/runtime/, app/src/projector/`.

| Audit | Explicit implementation and transitions | Verification basis |
|---|---|---|
| R1 connection installation/acceptance | `BuiltConnectedRuntime::install` produces `InstalledCandidate`; only installed typestate can spawn into `StartedConnectedRuntime`. `RuntimeSlot::{Vacant,Installed}` owns endpoint pair. `ConnectedSnapshotPhase::{RequestSnapshot,AwaitConnectedFact}` drives fixed-deadline readiness. `AcceptancePhase::{ShowMetadata,ScenePeers,RememberIdentity,Confirm}` drives accepted-order handoff; failures return through existing generation-scoped cleanup. `DisconnectPhase::{ClearMetadata,ClearRuntime,Publish}` carries the successful clear transaction into publication. | Existing lifecycle actor-level tests cover current/stale install, await connected fact, both timeout locations, stale show reply/peer/success fences, cleanup and same-generation disconnect. |
| R2 generation capability/admission | `AdmissionPhase::{CheckGeneration,ReserveCapacity,Commit}` owns reservation outcome until final generation+predicate fence. `ReplyPhase::{Awaiting,Validate}` owns reply through final generation validation. Shared `RuntimeGeneration` remains the explicit monotonic authority, not a duplicate cached current/revoked bit. Existing owned-permit paths already use affine permit typestate and remain fenced. | Existing stale-request test plus lifecycle/fade actor tests exercising waits/revocation. |
| R3 transport/reconnect and heartbeat | `TransportPhase::{Connecting,Registering,Draining,Connected,RetryDelay,Stopped}` is the real actor loop and owns each client. `Heartbeat::{Awaiting,TimedOut}` owns last-ping state and is transitioned on pong admission/deadline. Public `ConnectionStatus` behavior unchanged, including initial/retry snapshots. | Existing LV1 actor tests exercise pending connect/register responsiveness, stale-command flush boundary, retry and disconnected command outcomes, timeout/read failure. |
| R4 writer transfer/flush | `WriterOwnership::{Registration,Transferred}` replaces optional write half, transfers once. `WriterPhase::{Receiving,Writing,Flushing,Failed,Closed}` drives queue and failure/flush acknowledgments. Failure drains queued flush replies and signals connection before exit, same as baseline. | Existing writer-task failure/flush tests and TCP client tests retained. |
| R5 split scene observation | Stored `SceneBuffer::{Empty,Index,Name}` eliminates impossible both-pending representation; repeated same side replaces it; opposite side emits and returns Empty. | Existing arrival-order, overwrite-name, consumption tests; added pure behavior test for repeated-index replacement and no index reuse. |
| R6 transport library readiness | `SceneLibrary::{AwaitingNotification,Known(entries)}` owns live mirror; valid notification replaces Known, malformed preserves it, disconnect resets. Snapshot maps to existing Option wire/API boundary, preserving known-empty. | Existing actor scene-list readiness tests retained. |
| R7 incremental frame decoder | Stored `DecodePhase::{Prefix,Payload{length,header}}` drives parsing; validated prefix precedes payload waiting; emission resets Prefix for concatenation. Invalid prefix returns original error before payload buffering requirement. | Existing every-byte split, concatenated frames, invalid lengths/header, payload round-trip tests retained. |
| R8 discovery/startup | `DiscoveryPhase::{Collecting,Publishing,AwaitingAcceptance}` owns worker/result/reply under existing serialized slot. `DiscoveryWindow::{Listening{deadline,found},Complete}` owns bounded UDP collection. `StartupConnectionPhase::{Discovering,Selecting}` drives no-match-safe startup; no remembered identity exits before any discovery. | Existing discovery responsiveness/failure-retry and no-remembered/ambiguous-match lifecycle tests retained. |
| R9 projector once-only | `ProjectorState::{NotStarted,Running(sink)}` stored under lifecycle mutex; repeated readiness subscribes existing sink. | Existing first-log-source/start-once lifecycle test retained. |
| R10 publication/coalescing | `PublicationState::{Clean,Dirty}` replaces boolean, consumed only on timer publication; input changes invalidate. `PublishedView::{AwaitingFirst,Ready(snapshot)}` replaces watch Option; public latest API unchanged. | Existing projector coalescing, nonmaterial events, late subscriber, log lag/version tests retained. |
| R11 retained display reducer | Already explicit `AppConnectionState`/`AppFadeState` with `AppStateSnapshot::apply` event transition table and generation fence. Retained unchanged; no duplicate shadow FSM, replay machinery, or operational authority introduced. | Existing runtime state reducer / projector integration tests retained. |


## Persistence, cues, settings and updates

Source: `app/src/show/, app/src/cue_lists/, app/src/settings/, app/src/updates/, app/src/atomic_file.rs, app/src/show_file.rs`.

| Audit | Implemented owner/state | Transition and preservation |
|---|---|---|
| P1 Show metadata | `FileIdentity::{Untitled,Saved{path,saved_at}}`, orthogonal `Cleanliness::{Clean,Dirty}` | Save/load adopts path and timestamp atomically; template/New discard backing identity; dirtiness independent. Added pure metadata transition test. |
| P2 Show scheduler | `ShowExecution::{Idle,Running(PendingShowOperation)}`; `CommandAdmission::{Accepting,Deferred(ShowCommand),Drained}` | Running consumes facts/read commands; first mutation changes mailbox admission to Deferred, completion returns execution Idle and then executes held command. Existing pending-save responsiveness/order actor tests retained. |
| P3 Dirty-during-operation | `ObservedEdits::{Clean,Dirty}` inside owned operation | Persisted edit/lag makes latch Dirty permanently for operation; completion reapplies dirty after metadata commit. Existing pending-save edit actor test retained. |
| P4 New/load preparation | Existing `PreparedLoadImport` and `PreparedLoad` typestates plus new consuming `ValidatedSession` | Prepared document must pass connected/current/exact-list revalidation to produce a token; only token `.commit()` enters replacement. Metadata follows successful owner commit. Existing New/template/load/revalidation actor tests retained. |
| P5 Replacement ticket | Already explicit `session::ReplacementState` | Unchanged. Pending/Committed/Canceled single-owner ticket already models timeout/commit arbitration. |
| P6 Atomic publication | `Publication::{Staged,Published}` | Only successful atomic replacement changes phase; Drop cleans Staged temporary. No API changes. Existing persistence failure/save tests retained. |
| P7 Backup-before-save | `BackupPublication::{Reserved(File),Synced,Published}` | Copy/sync owns open file; transition closes handle before publish; published backup permits pruning/main save. Existing overwrite/retention/failure actor tests retained. |
| P8 Cue cursor | `CueSelection::{NoList,Active{list},Armed{list,entry}}`; `PlaybackCursor::{NotDispatched,Current(entry)}` | Internal source of truth no longer two independent persisted options; document conversion preserves old schema. Replacement rejects/clears invalid references; orphaned imported cue flags normalization. Current dispatch remains orthogonal to next cue. Added orphaned-import behavioral test; existing cue list reorder/reconcile tests retained. |
| P9 GO dispatch | `CueDispatch::{Idle,AwaitingDispatch(PendingRecall)}` | Await owns reply and entry; `finish` consumes once; success advances same cued identity only, failure/cancel does not. Existing Scenes-owner cue actor tests retain integration coverage. |
| P10 Settings persistence | `SettingsPreparation::{Unchanged,Staged}`; consuming `StagedSettingsUpdate::publish` produces `PublishedSettings` | No-op skips disk; only published typestate provides document for memory commit. Existing settings actor failure/no-op tests retained. |
| P11 Remembered identity | Same preparation/publication FSM inside existing generation authority gate | Prepared stage can be discarded by stale generation without memory/file mutation. Existing generation gate already explicitly models authority; no new epoch or parallel transaction. Existing stale-identity actor tests retained. |
| P12 SAFE | `AdmissionMode::{Normal,Safe}` | Internal state replaces bool; public watch/projection remain compatible bool. Repeated setting no-op. No cancellation/extra dialog support introduced. Metadata test checks transition/idempotence. |
| P13 Diagnostics | `DiagnosticVerbosity::{Normal,Extensive}` stored in `AtomicU8` | Two-state atomic gate controls INFO/DEBUG file filter; same relaxed ordering/UI separation. Existing filter tests retained. |
| P14 Updater acquisition | `ReleaseCapability::{Empty,Available(candidate),Downloaded{candidate,plan}}`; `UpdateWorker::{Idle,Checking,Downloading}`; `AutomaticCheck::{None,Queued}` | Explicit payload ownership removes candidate/staged option pairing, worker phase separate from retained capabilities and projected status. Refresh can keep downloaded plan; explicit Check clears it. Existing updates actor scenarios retained. |
| P15 Channel authority | `UpdateChannel::{Stable,Nightly}` in worker/plan authority; `InstallAuthority::{Unbound,Bound}` | Current channel comparison stays value-based, not epoch history; toggling away clears capability/replies but return may accept completion. Retained policy checked before plans/completion. Existing coalesced-toggle/late-completion tests retained. |
| P16 Deferred install | `InstallIntent::{Unarmed,Armed(plan)}` under mutex, bound plan authority | Arm validates, take consumes exactly one, drops never launch. UI host remains separate owner. Existing issued/armed plan authority tests retained. |


## Native UI

Source: `app/src/native_ui/`.

| Audit | Implementation | Transitions and evidence |
|---|---|---|
| UI1 GO admission/presentation | `state.rs::GoAcknowledgments` | AwaitingBoth, AwaitingCommand, AwaitingProjection, Complete replace two flags. Command and projection transitions, rebase and prefix pruning all mutate this owner. Existing GO behavioral tests cover both arrival orders, duplicates, failure and rebasing. |
| UI2 Probe ownership | `connection.rs::ProbeState` | VisiblePending, HiddenPending, Complete in a single keyed map replace separate in-flight and display stores. Disappearance hides owned attempt, reappearance restores it, matching completion removes hidden attempt or records visible result. Existing extensive latency tests retained and adapted to visible projection accessor. |
| UI3 Inline editor | `cues.rs::CueEditPhase`, `CueEditorTarget` | Editing(error) / Submitting(command ID); existing vs insertion target owns marker only for insertion. Successful completion closes editor, matching failure returns editing with error, Escape respects submitting phase. Marker/drop FSM is orthogonal. |
| UI4 Insertion marker | `cue_marker.rs::MarkerMovement`, `MarkerDrop`, `MarkerInsertion` | Each is Synchronized / named pending intent. Reconciliation consumes only changed projection, unchanged projections retain intent. All original combinations and reconciliation order preserved, including coalesced move+insertion. Existing marker tests retained. |
| UI5 Cue manager | `cues.rs::CueManager`, `ManagerEditor` | Closed, Open(editor), Submitting(command ID, operation, retry editor). Nested editor is Idle/Naming/Deleting, so mutually exclusive forms cannot coexist. Matching result releases worker but retains retry form on failure; snapshot target invalidation may clear form while command remains owned. Added pure tests for stale completion and target removal during pending command. |
| UI6 Duration edit | `state.rs::ProjectedEdit<(Uuid,u64)>` used by `scenes.rs` | Projected, Submitting, AwaitingProjection. Same command success never replaces authoritative state. Failure removes only matching current edit; selection/matching projection clears. Shared pure test checks newer edit survives old failure and successful duplicate results retain draft. Existing duration behavior tests retained. |
| UI7 Settings edit | `ProjectedEdit<AppSettings>` in `settings_view.rs` | Same lifecycle with complete settings value carried in active phase, removing draft/command-ID independent options. Matching projection acknowledges; new edits build on pending value. |
| UI8 Deferred numeric commit | `state.rs::DeferredCommit` / `DeferredCommitPhase` | Editing / Scheduled(revision), explicit actions cancel and advance revision, matching callback consumes scheduled commit. Shared by all settings numeric controls and scene duration. Added test verifies stale callback cannot consume newer schedule and schedule is one-shot. |
| UI9 Link confirmation | `scenes.rs::OverwriteConfirmation` | Closed, Awaiting(exact request+return focus), RestoringFocus(focus). Confirmation/dismissal/invalidation take exact request and restore focus through named transition. Existing exact target/source checks remain. |
| UI10 Update restart handoff | `app.rs::UpdateRestart` | Idle, Preparing(command), AwaitingSession(plan), Disconnecting(command). Plan cannot coexist with a preparation worker. Matching event type and ID required, cancellation drops only retained session plan; existing dirty guard/disconnect/arm order retained. Added pure test rejects wrong-phase/wrong-ID/duplicate worker results. |
| UI11 Connection chooser | `ConnectionDialogMode`, `ConnectionRequest` | Explicit Closed/Startup/Manual; request Idle/Selecting(identity)/Connecting(identity,command). Selecting owns identity before dispatch, only matching completion releases it. Existing tests retained. |
| UI12 Shortcut recording | `settings_view.rs::ShortcutRecording`, `keyboard.rs::ShortcutCapture` | Idle/Capturing(action+capture)/Conflict(action+message), internal Idle/Recording(binding ID). Eliminates independent capture/action/conflict options. Modifier keeps capture, Escape cancels, binding produces full settings replacement or conflict. Existing shortcut behavior tests retained. |
| UI13 Snapshot/error ownership | `state.rs::SnapshotPhase`, `CommandOutcome` | Uninitialized/Initialized controls strict accepted-version rule; Pending/Succeeded/Failed(error) carries visible command result, with latest ID ownership retained. Existing presentation tests cover version ordering and late command completions. |
| UI14 Save fallback | `app.rs::OrdinarySave` | Idle/Saving(command)/ChoosingDestination. Matching destination event opens picker; cancellation/error returns Idle; chosen path enters Saving with returned command ID. Existing native file guard behavior retained. |
| UI15 Serial UI command lane | `dispatcher.rs::SerialLane` | AwaitingCommand/Executing(command ID+future)/Closed; FIFO receiver yields one owned future, completion yields correlated event, closure terminates. SessionGuard remains source of busy state mirrored atomically for existing cross-thread admission; no revocation of admitted work. Existing dispatcher actor/adapter tests retained. |
| UI16 Navigation/focus/modal/menu | `menu.rs::SessionMenuState`, `shell.rs::SessionMenuVisibility` plus existing `MainTab` | Closed/Open(menu entity) owns lazy popup; dismissal consumes it. Shell stores Closed/Open visibility, active tab remains explicit MainTab. Modal owners now explicit above; focus/editable flags are event observations, not a competing stored workflow. Existing route_action and native menu tests remain. No giant global UI product FSM introduced. |
| UI17 Host one-shot resources | `entry.rs::RuntimeOwnership`, `runtime.rs::ProjectionOwnership` | Running(runtime)/Shutdown, Available(subscription)/Installed. Taking consumes resource exactly once; quit callback and fallback share same runtime owner. NativeRuntime::shutdown already consumes self; no concurrent new shutdown flow. Existing shutdown behavior tests retained. |
| UI18 Render handoff | `clipped_overlay.rs::OverlayChild` | Layout(element)/Deferred; layout accesses owned child, prepaint transfers child exactly once to framework deferred drawing. Content mask remains framework-provided fact. No tests duplicating GPUI render lifecycle added. |
| Already explicit SessionGuard | Retained existing `GuardPhase` and `PendingAction` | Querying/choice/destination/save/continuing phase implementation unchanged, atomic busy remains compatibility projection of that owner. No session epochs or unsupported overlap protocols restored. |


## Verification strategy

Pure unit tests exercise transition boundaries without side effects. Existing actor tests continue
to exercise mailbox/event behavior, generation fencing, I/O failures and ordering; existing native
UI/adapter tests cover presentation and input behavior. The refactor does not replace these tests
with checks of enum names or framework internals. Native macOS and Windows CI supplies compilation,
Clippy, test and build verification. Hardware smoke remains a separate LV1-dependent check and is
not implied by a passing state-machine test suite.
