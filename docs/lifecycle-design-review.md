# Connection Lifecycle Design Review

Status: proposed architecture, not implemented or approved. Reviewed against `b359d29` on 2026-09-11. This does not replace the description of the current implementation in [architecture.md](architecture.md).

Subsequent narrow refactors implemented a complete `InstalledRuntime` value and connection-bound LV1 clients for Fade, Show, and Scenes. The existing coordinator and peer-installation mechanism remain; the connection-owner actor, shared runtime reader, and task-scope rewrite below are still only proposals. The findings and LOC figures below describe the original review baseline.

## Decision under evaluation

Prefer a small connection-owner actor with a constructor-injected, read-only runtime reference. Preserve generation-scoped LV1 and Fade endpoints. Do not add handle-attachment messages, a generic supervisor framework, or a universal application command router.

This is an architectural recommendation, not a demonstrated LOC saving. A simpler transactional coordinator remains the fallback if the replacement adds more coordination than it removes.

The earlier discussion conflated three independent choices:

1. Who may change the current connection: concurrent coordinator methods or one command-processing owner.
2. How consumers obtain a changing resource: mutable peer slots, installation commands, or one shared readable reference.
3. Who stops and observes the tasks: dropping senders is not task ownership.

An actor addresses the first choice only. This proposal specifies all three.

## Findings that constrain the design

- `lifecycle/mod.rs` mixes connection transitions, app service construction/access, discovery, startup policy, and projector startup. It contains 3,007 lines, including 2,055 lines in its main test module. The remaining 952 lines also contain some test-only helpers; they are not an exact production LOC count.
- `RuntimeHandles` permits partial LV1/Fade pairs and stores their owning generation separately. The application does not need those partial combinations.
- `RuntimeHandles::abort_all()` drops senders. LV1 and Fade `spawn()` methods discard their task handles. LV1 also spawns a separate socket writer whose lifetime needs to be accounted for.
- Fade captures one LV1 handle when constructed. `Lv1Command::WriteBatch` and `RecallScene` do not carry a connection generation. Their safety depends in part on the sender addressing a generation-specific endpoint, with generation checks before command admission.
- Scenes has separate runtime availability and scene-readiness policy. Installing a sender does not mean a live scene library is available.
- Show needs an LV1 reader before connection setup has completely finished; current tests explicitly cover that ordering.
- Lifecycle constructs Scenes after obtaining Show's lockout reader, then installs the permanent Scenes handle back into Show. This bootstrap dependency must be resolved, not hidden in another setter.
- Settings persistence and Show metadata updates are acknowledged operations with generation-protected commits. Moving lifecycle into an actor cannot revoke messages already accepted by those actors.
- LV1 already owns transport retry inside a generation. A new owner must not add a competing reconnect policy.

Relevant source boundaries: [lifecycle](../src-tauri/src/lifecycle/mod.rs), [Show peers](../src-tauri/src/show/actor.rs), [Scenes peers and readiness](../src-tauri/src/scenes/actor.rs), [LV1 commands](../src-tauri/src/lv1/commands.rs), [LV1 transport tasks](../src-tauri/src/lv1/actor.rs), [Fade write admission](../src-tauri/src/fade/actor.rs), and [generation guards](../src-tauri/src/runtime/generation.rs).

## Ownership and dependency graph

| Component | Owns | Receives at construction |
| --- | --- | --- |
| Application composition | Service construction, connection-owner task observation, shutdown wiring | Tauri/platform resources |
| Connection owner | Connection admission, current runtime publication, pending setup, runtime retirement | Runtime write authority; Show, Scenes, and Settings senders; event bus |
| Runtime task scope | One generation's LV1 and Fade tasks | The fixed target identity and generation |
| LV1 actor | TCP session, transport reconnect, live mirror, socket writer lifetime | Target identity, fixed generation, event bus |
| Fade engine | Timing, active targets, readiness and overrides | Fixed LV1 sender, generation reader, event bus |
| Show | File metadata, lockout and persistence orchestration | Scenes sender, runtime reader, lockout publisher, event bus |
| Scenes/Cues | Persistent document, recall policy and queue | Runtime reader, Settings sender, lockout reader, event bus |
| Projector | Live projection and frontend emission | Runtime reader, retained app-state stream, event/log subscriptions |

Scene/cue/fade control does not travel through the connection owner's mailbox. The connection owner is not a service locator.

### Bootstrap without late permanent-dependency installation

Application composition creates the lockout channel before constructing Show and Scenes. Show receives its publishing side; Scenes receives its reading side. Show remains the authority for lockout.

Composition can then construct Settings, Scenes/Cues, Show, and the connection owner with their permanent dependencies. Ordinary channel construction before task startup resolves the wiring; it does not require mutable placeholders or an actor-builder framework. The existing reverse dependency from Scenes to Show remains a lockout reader, not a mailbox query.

## One runtime reference, not attachment messages

App-lifetime consumers receive a stable runtime reader at construction. Its authoritative record contains:

- the current monotonic generation;
- either no installed runtime or a complete generation-tagged LV1/Fade endpoint pair.

Only the connection owner receives authority to change this record. Consumers can take a pinned snapshot of its endpoints and generation; they cannot install or clear peers.

This replaces `ShowActorPeers`' changing LV1 slot, `ScenesPeers`, and the projector's access to lifecycle's internal mutex. Show's permanent Scenes dependency becomes an ordinary constructor argument. It is not another registry alongside the old ones.

The runtime record and generation fence must share a synchronization boundary: invalidating a generation and withdrawing its endpoints is one control-plane commit. The existing generation-check/command-admission guarantee remains. An actor does not remove this short critical section because other actors must fence side effects against connection changes.

Build this from the existing generation authority rather than introduce independently writable generation and availability stores. The concrete target is a short synchronous mutex around the connection record, with no exposed guard that can cross an await. Readers can snapshot the record or perform an existing generation-fenced synchronous commit; only the owner's write capability can replace endpoints or advance the generation. This also permits synchronous fail-closed revocation during owner destruction. A closed or poisoned authority admits no actions.

No lock is held across an actor request, network wait, or persistence preparation. Existing narrow settings-file commit constraints still apply: changing the mutex type does not make the final filesystem commit free or eliminate the serialization required for stale-write protection. This is a change to the existing Tokio-mutex generation implementation and must be included in the implementation/test budget, not described as removal of all synchronization.

This is deliberately a dynamic-resource reference. Constructor injection does not make connection changes disappear. It gives consumers one stable dependency instead of a second attach/detach command protocol. The reference has a fixed purpose and shape; it is not a generic service registry.

A captured runtime snapshot never retargets itself. A consumer holding generation A's LV1 sender cannot accidentally send to generation B's actor. After an await, existing generation validation is still required before protected effects.

An endpoint pair is published at most once for a generation; withdrawing or replacing it advances the generation. Consumers capture their endpoint snapshot before entering a generation-fenced commit. Commit code must not re-lock the same runtime reader: any needed validated context is captured beforehand or supplied by the commit operation. This requires auditing current patterns such as Scenes checking its separate peer slot inside `if_current`; mechanically replacing that slot with a getter on the same mutex would deadlock.

Fade does not look up the latest LV1 endpoint on every tick. It retains its fixed LV1 sender for its whole lifetime.

## Control flow and readiness

The owner's public commands are connection requests, disconnect, and shutdown, with replies. Startup auto-connect uses the same connection-admission path; discovery is not performed inside a blocking owner command handler.

The owner processes commands while polling pending asynchronous setup or retirement. External awaits are not placed inside a handler that prevents the loop from receiving disconnect.

Conceptual phases are:

- **Idle:** no installed runtime or pending connection.
- **Retiring:** old runtime resources are being stopped; an optional latest accepted connection request is waiting.
- **Starting:** a complete runtime has been constructed and published, but initialization is pending.
- **Active:** initial connection setup has completed. This does not assert that transport can never subsequently disconnect.

These are resource-lifecycle phases, not replacements for LV1 connection status or Scenes' library-readiness state.

### Successful connection

1. Admit the request and allocate its generation. Supersede any older pending request.
2. Invalidate and withdraw an older runtime before retiring it. Do not let a new runtime become operational while retirement of the previous endpoint scope is unfinished.
3. Construct the complete LV1/Fade pair, binding Fade to that LV1 endpoint. Establish subscriptions and task ownership before starting network work.
4. Publish the complete endpoint pair for the admitted generation. It is installed, not necessarily connected or ready. Show can query LV1 through this reader.
5. Obtain initial connected LV1 state. Apply connection metadata through Show's existing generation-guarded command.
6. Ask Scenes to initialize/synchronize this generation, using its constructor-supplied runtime reader. Preserve the acknowledged readiness boundary and existing reconciliation/fresh-state safety rules. No handles are carried by this command.
7. Remember the identity through Settings. Persistence failure is reported but does not turn a valid connection into a failed connection.
8. Reply and publish success only if the attempt remains current.

Scenes continues to gate recalls on its own readiness, exact identity, lockout, fresh state, and generation checks. Neither the presence of runtime endpoints nor a previous readiness acknowledgement grants permanent permission to recall.

### Disconnect and supersession

The safety effect occurs at admission: advance the generation and withdraw the runtime in one commit, then publish the corresponding facts. Do not wait for Show, Settings, or TCP before invalidating old work.

Cancel the old setup operation and retire its task scope. Dropping a requester's reply receiver does not cancel this application-owned work. Dropping a setup future does not undo remote messages it has already sent; those messages retain their generation-protected commits.

Metadata clearing is tagged with the new controlling generation, not blindly applied using the retired runtime's identity. A subsequent connection may supersede that cleanup; old completion cannot clear a newer runtime or publish a misleading success/failure message.

Retirement itself survives replacement of a pending connection request. A newer request replaces the waiting intent, not the responsibility to stop the old tasks. This keeps resource retirement bounded instead of leaving an unbounded set of detached cleanup workers.

A successful changed disconnect reply means the retired scope has stopped. Repeated disconnects must not produce duplicate transition facts or success logs. Replies describe the request's outcome, not a substitute for the versioned frontend state.

Generation allocation remains monotonic. Exact private intermediate increment counts are not the architectural contract; stale-work rejection and observable event ordering are.

## Task lifetime and failure policy

The runtime scope owns a Tokio `JoinSet` containing LV1 and Fade, not just mailbox senders. Normal retirement cancels the scope and polls task completion while the owner continues receiving control requests. `JoinSet` drop supplies cancellation on an abnormal owner exit; dropping bare `JoinHandle`s would instead detach their tasks.

For the nested socket writer, prefer polling the writer future inside LV1's connected-loop `select!` rather than spawning another unowned task. The read/command loop and writer still make concurrent progress through the bounded writer queue, but dropping the parent future also drops the writer and socket write half. This avoids the false assumption that joining an aborted LV1 parent also joins a separately aborted writer task. Writer errors, queue backpressure, flush acknowledgements and ping responsiveness require their existing protocol tests to remain intact.

These are concrete lifetime mechanisms, not a generic supervisor abstraction. Their behavior must be verified in the replacement slice, especially under socket backpressure.

Unexpected termination of LV1 or Fade invalidates that runtime and retires its sibling. No automatic restart or replay of fades, recalls, or queued mixer actions is introduced.

A transport interruption handled inside LV1 is different from actor termination. It follows the existing disconnected/reconnect event flow within the same generation; Scenes and Fade continue their existing abort/readiness behavior.

Unexpected connection-owner termination must revoke the published runtime and cancel the scope, rather than orphaning fader tasks. The owner's drop path synchronously closes its runtime write authority before its `JoinSet` fields are destroyed. The authority then rejects further protected commits. Previously cloned senders are not themselves revoked; cancellation and termination of their receiving tasks are also required. Application composition retains/observes the connection-owner task result to surface failure; it does not automatically restart the owner or replay its previous target. The same revocation is idempotent during normal shutdown. A mailbox alone supplies none of these guarantees.

No design can retract bytes already handed to the OS or applied by the console. The enforceable boundaries are fenced command admission, fixed-generation endpoints, and completed transport teardown. A rewrite must not claim a stronger physical rollback guarantee.

## Event and read flow

Keep the existing operational path:

```text
Console -> LV1 actor -> AppEventBus -> Scenes / Fade / Projector
Scenes -> validated recall command -> LV1
Scenes -> validated fade command -> Fade -> write batch -> LV1
```

Connection facts notify consumers of changes; they are not the authority for permission to use stale endpoints. The runtime reader supplies the current generation/resource snapshot when facts are delayed or lost. Projector recovery must not query a connection actor that might be waiting for setup.

The generation commit and its publication must be ordered by the same owner. App-owned document/settings projections keep the retained snapshot stream already implemented. No event replay framework or generic effect dispatcher is introduced.

Discovery stays on its independent blocking worker and retains its result-ordering protection. A late startup discovery result must be conditionally admitted against the generation it observed, so it cannot silently override a newer manual connection request. This is an explicit admission rule to cover in tests, not an assumption about frontend ordering.

## Alternatives examined

### Transactional coordinator

It can use the same complete runtime representation, stable reader, and owned task scopes. These improvements are not exclusive advantages of an actor.

It is the lower-risk fallback and may yield the smallest immediate implementation. However, simultaneous request methods and asynchronous finalizers still need shared-state arbitration. The owner actor's particular benefit is giving request ordering, pending-request supersession, and retirement responsibility one explicit place.

The actor earns its cost only if those coordinator methods, locks, and detached finalizers actually disappear. Wrapping the existing implementation in mailbox handlers is rejected.

### Actor with attach/detach messages

Rejected for this proposal. It replaces peer slots but adds a second dependency-installation protocol, acknowledges handle delivery separately from domain readiness, and still needs independent generation invalidation while detach messages are queued.

The existing acknowledged Scenes readiness operation remains because it represents real domain work. It is not repurposed as a generic handle installer.

### Permanent LV1 and Fade actors with retargetable connections

Rejected for this rewrite, despite the appealing constructor-only graph.

A permanent mailbox would survive target changes. Existing raw write and recall commands could then be consumed against a different transport unless generation fencing moved into all relevant command-consumption paths. Fade target state, ping/observation sequences, queued replies, and reconnect reset behavior would also need auditing and changes.

That could be a valid architecture in another scope, but it is not equivalent to deleting runtime lifecycle plumbing. It trades endpoint-lifetime isolation for a broader safety protocol and affects the hot write path. There is no demonstrated LOC advantage here.

### Generic supervisors, reducers, or desired-state frameworks

Not needed. This proposal uses the supervision principle of owned task lifetimes, a small command loop, and the existing actors. It does not add generic restart policies, an effect language, or routing every application action through lifecycle.

## Transition traces and verification contract

Rust verification should use actor tests through the public connection mailbox, event bus, tracing capture, and controlled LV1/peer boundaries. Pure unit tests are appropriate for any extracted transition/admission decision. Hardware smoke remains necessary for real console behavior, not as a substitute for deterministic concurrency tests.

| Scenario | Required result |
| --- | --- |
| Normal connect | Correct target, complete same-generation pair, preserved readiness ordering, one success result/log |
| B supersedes A during initial state request | A is invalidated and retired; its late reply cannot install, clear, persist, or log for B |
| Disconnect while setup or metadata is stalled | Invalidation does not await the stalled actor; runtime tasks are retired; late commits are rejected |
| Disconnect then a new connect during retirement | Old retirement still finishes; only the latest admitted pending target starts |
| Caller disappears after admission | Owned setup/retirement completes or is explicitly superseded; no stranded half-connection |
| Settings preparation is stalled | Connection invalidation remains possible; stale identity cannot become authoritative |
| Startup discovery returns after manual connect | Conditional startup request is rejected without replacing the manual target |
| Generation/readiness facts arrive late or subscriber lags | Fresh runtime authority blocks old actions; readiness and projection recover without trusting event order |
| Old consumer retains a sender | It addresses only the old endpoint; protected actions fail the generation fence |
| LV1 transport reconnects | No new competing retry loop; document UUIDs survive; readiness is re-established before automation |
| Runtime task or socket writer fails | Failure is visible; no independently running old writer or sibling fade survives retirement |
| Connection owner exits unexpectedly | Published access becomes unavailable and owned runtime tasks are canceled; no silent automatic replay |
| Show/cue operations while disconnected | Persistent document ownership and document-only commands remain available |

These preserve the safety concerns in GitHub issues #52, #54, and #66. This review does not claim those issues are all unresolved in this branch, nor that an architectural rewrite alone closes them.

## LOC accounting and decision gate

Expected deletions include the partial runtime representation, separate peer registries/setters/clearers, lifecycle-backed runtime snapshot lookup, nested finalizer/reply plumbing, and obsolete shared-state transition coordination. Direct app-service construction also removes permanent-dependency late installation and service-locator accessors; merely moving startup functions is not counted as a saving.

Required additions include a runtime reader/write authority, explicit owner phase/command handling, owned task retirement, and behavioral test fixtures. All of these count against the deletion budget, including changes in LV1, Fade, Show, Scenes, projector, adapters, and tests.

No numerical reduction is claimed before replacement code exists. Replacing 2,055 lines of tests with weaker assertions is not architectural progress. The implementation must preserve their behavioral obligations and add coverage for the newly explicit ownership and shutdown contracts.

The decision gate is a functioning replacement slice, with normal connect, supersession, stalled setup disconnect, and stale endpoint behavior demonstrated. If that slice requires broad forwarding infrastructure or cannot credibly reduce total code, use the transactional coordinator instead. There should not be two production lifecycle implementations or a permanent compatibility layer.
