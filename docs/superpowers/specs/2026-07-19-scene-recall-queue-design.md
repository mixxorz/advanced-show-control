# Scene Recall Queue Design

## Context

Advanced Show Control currently replies to `ScenesCommand::RecallScene` after the
LV1 writer queue accepts `/Set/CurSceneIndex`. LV1 may still be processing that
recall when another direct Recall or cue-list Go request reaches the same path.
Real LV1 systems can miss these rapid recalls.

GitHub issue #61 requires one bounded, runtime-only FIFO for every
ASC-originated scene recall. The queue must preserve operator order, including
repeated requests for the same scene, while retaining the existing meaning of a
successful command reply: the requested recall was actually dispatched to LV1,
not merely admitted to the queue.

The existing post-recall readiness behavior from issue #35 remains the single
definition of when LV1 is ready after an exact scene observation: two later
same-generation LV1 keepalive pings. This design extends that behavior to queue
progress without adding a fixed delay or a second competing ping timer.

## Goals

- Route direct Recall and cue-list Go through one `scenes`-owned FIFO.
- Dispatch the first valid request immediately and later requests in FIFO order.
- Wait for an exact post-dispatch settled scene observation and two subsequent
  same-generation pings before dispatching the next request.
- Apply the same wait to ASC recalls with no timed fade targets.
- Validate before admission and again from fresh state immediately before
  dispatch.
- Bound the total queue at eight requests, including the in-flight request.
- Fail closed and clear delayed intent on timeout, disconnect, generation
  change, shutdown, Abort All, or loss of safe runtime state.
- Preserve existing scene identity, lockout, generation, fade, manual override,
  overlap, same-scene, and disconnect safety behavior.

## Non-Goals

- Persisting recall queue state in show files or settings.
- Coalescing or deduplicating repeated scene recalls.
- Moving show-aware recall policy into the LV1 transport actor.
- Adding a configurable queue capacity or readiness timeout.
- Treating LV1 writer-queue acceptance as recall completion.
- Changing LV1's OSC recall command or current-scene identity model.
- Redesigning direct Recall, cue-list Go, or frontend status projection.

## Ownership

### Scenes

`ScenesTask` owns the recall FIFO, admission and dispatch validation, the
in-flight scene-observation phase, queue cancellation, and queue progression.
Its runtime state contains:

- A `VecDeque` of waiting recall requests.
- At most one in-flight recall.
- A total capacity of eight requests across the in-flight and waiting entries.
- A per-request ID, requested app scene identity, runtime generation, and caller
  reply sender.
- For the in-flight request, the LV1 scene-observation boundary and one absolute
  five-second completion deadline measured from successful dispatch.

Waiting caller replies remain owned by `scenes` until their requests dispatch or
fail. The actor event loop must continue processing mailbox commands and runtime
facts while replies are pending; queue admission must not await another
request's completion inline.

### Fade

`FadeEngine` continues to own the generation-wide post-recall readiness barrier,
ping counting, pausing and rebasing targets, timeout behavior after handoff, and
all fader writes. The barrier accepts the original absolute deadline from
`scenes` for ASC recalls so observation time consumes part of the same five
seconds rather than restarting the timeout.

ASC recalls that produce a fade configuration attach a queue-completion sender
to the existing fade-recall command. This includes zero-duration configurations,
whose existing immediate writes remain unchanged. ASC recalls that produce no
fade configuration use a narrow readiness-only fade command. Both commands
initialize the same private barrier state and use the same ping boundary,
generation checks, lag handling, release, and timeout behavior. There is no
second readiness state machine.

The delayed readiness result is separate from the fade command's immediate
acceptance reply. `scenes` can therefore keep processing its mailbox while
`FadeEngine` resolves readiness later.

### Show Lockout

`show` remains the sole owner of lockout. It publishes the latest lockout value
through a show-domain watch channel whenever show state changes, including file
replacement. `scenes` receives a read-only watch endpoint and borrows its latest
value during admission and dispatch validation.

This avoids a direct `scenes -> show` mailbox query. Such a query would create a
cycle because show-file workflows already await `scenes`, allowing `show` and
`scenes` to deadlock while each waits for the other actor.

### LV1 Observation Boundary

The LV1 actor owns a monotonically increasing, connection-local scene
observation sequence. Accepted current-scene observations carry this sequence,
and an LV1 recall dispatch result reports the latest sequence at the dispatch
boundary. `scenes` accepts completion only from an exact settled `(index, name)`
observation with a sequence newer than that boundary.

The sequence distinguishes a post-dispatch observation from an older event that
was already published but not yet consumed by `scenes`. It is runtime-only and
does not change the OSC protocol or persisted data.

## Command And Event Flow

### Admission

1. Every production ASC recall entry point sends `ScenesCommand::RecallScene`.
   Direct Recall and cue-list Go already share this command path.
2. `scenes` obtains fresh LV1 state and reads the latest show-owned lockout
   value.
3. It validates the active generation, connection safety, lockout, app scene
   configuration, exact LV1 scene-list identity, and request linkage.
4. If validation fails, the request receives an error and never enters the
   FIFO. Existing fades and any in-flight recall remain unchanged.
5. If eight requests are already in flight or waiting, the request receives a
   visible overflow error. No existing entry is dropped.
6. Otherwise the request is admitted. If no recall is in flight, queue draining
   starts immediately. If another recall is in flight, the reply remains
   pending with the queued entry.

### Dispatch

1. Before dispatch, `scenes` reacquires fresh LV1 state, reads current lockout,
   and repeats all safety and exact-identity validation.
2. If a queued request is now invalid, it receives its own error. `scenes` then
   tries the next FIFO entry without changing active fades.
3. The first valid entry sends `Lv1Command::RecallScene` with the expected
   generation and exact requested identity.
4. LV1 writer-queue acceptance returns the scene-observation sequence boundary.
5. `scenes` records the request as in flight with an absolute deadline of five
   seconds from dispatch.
6. Only then does `scenes` send the caller's successful reply. Cue-list
   auto-next therefore retains its meaning: it advances after that cue's recall
   was actually dispatched.

The first request is still validated both for admission and for dispatch. These
checks occur back-to-back when the queue was empty, but remain distinct safety
boundaries.

### Exact Observation

`scenes` continues to use the normal 25 ms settled scene-observation path. An
in-flight request advances only when the settled event:

- Belongs to the active runtime generation.
- Has a scene-observation sequence newer than the dispatch boundary.
- Reports both the exact requested LV1 scene index and exact scene name.
- Is confirmed by the normal fresh LV1 snapshot check.

A mismatch, old observation, stale-generation event, scene-list edit event, or
duplicate pre-dispatch observation does not advance the request. Repeated
same-scene requests remain valid because each dispatch requires its own newer
observation sequence.

Pings received before the exact observation cannot count. Readiness is not
handed to `FadeEngine` until this observation phase succeeds.

### Fade Readiness Handoff

After exact observation, `scenes` runs normal scene-recall and fade policy using
fresh state and settings. It then hands the in-flight request ID, generation,
scene identity, and original absolute deadline to `FadeEngine`:

- A started fade policy decision, including a zero-duration configuration, uses
  the fade-recall command with the attached delayed completion sender.
- A disabled, skipped, blocked, empty-scope, or otherwise no-configuration ASC
  recall uses the readiness-only command.

The readiness-only path creates no targets and sends no fader commands. It can
still pause unrelated active targets because LV1 readiness is connection-wide.
The zero-duration fade path keeps its existing immediate parameter writes, then
uses the barrier only to control queue progression and any unrelated active
targets.

`FadeEngine` records the current fresh LV1 ping sequence as the observation
boundary. It counts only strictly newer pings from the same generation. One ping
leaves the barrier closed; the second resolves the completion sender and applies
the existing timeline-rebase behavior.

### Queue Progression

When `scenes` receives successful readiness for its current request, it clears
the in-flight entry and drains the FIFO. Invalid waiting entries fail
individually in order. The first valid waiting entry dispatches, or the queue
becomes empty.

Readiness outcomes carry the request ID and generation. A delayed result for a
canceled or replaced runtime cannot release a newer request.

## Failure And Cancellation

### Individual Request Failures

Admission failure, queue overflow, or fresh dispatch-time validation failure
affects only that request. The actor sends a typed error to the waiting caller,
does not send an LV1 recall, does not change the fade engine, and continues queue
draining when applicable.

An LV1 recall dispatch failure indicates loss of the command path. The failing
request receives the transport error and all later queued intent is canceled.
No fade command is sent and an existing fade remains unchanged.

### Timeout

One five-second absolute deadline starts when LV1 accepts the recall dispatch.

- Before exact observation, `scenes` owns the timer. Expiry clears the in-flight
  wait and all queued requests without changing active fades.
- After exact observation, `FadeEngine` owns the remaining deadline. Expiry
  preserves issue #35 behavior: all targets paused by the readiness barrier are
  aborted and no deferred writes are sent. Fade reports the failed readiness to
  `scenes`, which clears all queued recalls rather than advancing.

This phase distinction reconciles queue safety with established fade safety. A
pre-observation queue timeout does not invent a fade abort; a post-observation
ping timeout retains the existing fail-closed fade abort.

### Runtime Safety Changes

LV1 disconnect, active generation change, actor shutdown, lockout activation,
closed peer channels, event-bus lag or closure, or an invalid shared generation
guard clears the in-flight queue wait and every waiting request. A lagged
subscriber cannot infer that a missed observation, disconnect, or generation
fact was safe. Pending caller replies receive cancellation errors. The caller
for a request that already dispatched has already received success, so later
cancellation is reported operationally rather than attempting a second reply.

If readiness was already handed to Fade, non-Abort queue cancellation detaches
queue progression but does not invent a new fade transition. Fade continues to
apply its existing disconnect, generation, manual override, and timeout rules.
Any delayed readiness result for the canceled request is ignored by request ID
and generation.

### Abort All

Abort All becomes a `ScenesCommand` at the Tauri boundary. `scenes` first clears
the in-flight queue wait and every waiting recall, then forwards
`FadeCommand::AbortAll` and returns its result. Fade remains the owner of target
abortion; `scenes` owns cancellation of delayed recall intent.

This makes Abort All a global operator stop without adding queue knowledge to
the UI adapter or LV1 transport actor.

### Shutdown

`ScenesCommand::Shutdown` explicitly resolves all waiting reply senders with a
cancellation error before the actor exits. Dropping oneshot senders is not the
normal shutdown contract.

## Logging

Use `tracing` with stable `event` fields and complete user-facing messages.
`scenes` is the owning layer for admission blocks, queue overflow, dispatch
failure, and queue cancellation. Tauri adapters and cue lists do not duplicate
those facts.

Normal admission and internal queue depth changes remain `DEBUG` unless an
operator action is delayed in a way that needs explicit UI visibility. Queue
overflow, safety blocks, unexpected cancellation, and timeout are user-facing
`WARN` or `ERROR` events according to the existing logging policy. Explicit
Abort All can report one `INFO` outcome describing both fade-target and queued
recall cancellation.

For a non-ASC readiness barrier, Fade retains its existing timeout warning. For
an ASC barrier with a completion owner, Fade returns the timeout context and
keeps low-level details diagnostic; `scenes` emits one combined user-facing
message explaining that readiness timed out, paused fades were aborted, and
queued recalls were canceled. This prevents duplicate timeout messages while
keeping ownership explicit.

Messages include scene identity, generation, queue count, request ID, timeout,
and reason as structured diagnostic fields where applicable. The human-readable
message must remain sufficient without those fields.

## Testing

Tests follow the repository's allowed Rust test categories.

### Pure Unit Tests

Use direct, side-effect-free tests for any extracted queue-capacity, FIFO-order,
or observation-boundary decision functions. Do not inspect actor internals to
test side effects.

### Scenes Actor Tests

Test through the scenes mailbox, `AppEventBus`, fake LV1 and Fade command
channels, the show lockout watch, and the shared tracing capture helper when
asserting operational logs. Cover:

- Immediate dispatch when no recall is in flight.
- FIFO dispatch and successful reply timing.
- Repeated same-scene requests without coalescing.
- Capacity of eight total requests and visible overflow.
- Exact newer scene index/name observation requirements.
- Mismatched, pre-dispatch, stale-generation, and scene-list-edit observations.
- Pre-observation pings, one post-observation ping, and two-ping release.
- Timed and no-fade ASC recalls.
- Fresh admission and dispatch revalidation.
- Lockout or identity changes while queued.
- Invalid queued-item failure followed by the next valid item.
- Pre-observation timeout without active-fade disruption.
- Post-observation timeout cancellation after Fade reports its existing abort.
- LV1 send failure and cancellation of later intent.
- Disconnect, generation change, lockout activation, peer closure, event-bus
  lag or closure, shutdown, and stale delayed readiness outcomes.
- Abort All clearing queue state and forwarding the fade abort once.
- One owning-layer user-facing log for each overflow, cancellation, or timeout
  outcome.

### Fade Actor Tests

Test through the fade mailbox, fake LV1 mailbox, and `AppEventBus`. Cover:

- Readiness-only barriers with no fade targets.
- ASC completion success after exactly two qualifying pings.
- Original absolute-deadline handoff rather than a restarted five seconds.
- Completion cancellation on timeout, disconnect, generation change, and Abort
  All.
- Existing target pausing, timeline rebasing, overlap, same-scene behavior,
  manual override, and fail-closed timeout abortion.
- No duplicate user-facing timeout warning when `scenes` owns the ASC outcome.

### LV1 Actor Tests

Test scene-observation sequence publication and the sequence boundary returned
after recall dispatch. Preserve OSC `/Set/CurSceneIndex` encoding, writer-queue
acknowledgement semantics, current-scene pairing, ping publication, and pong
behavior.

### Cue-List Actor Tests

Use a fake scenes mailbox that can retain and later resolve recall replies. Prove
that auto-next does not occur while a recall is merely queued, advances after
actual dispatch success, does not advance on cancellation or revalidation
failure, and preserves rapid Go order.

### Smoke And Hardware Verification

Extend the debug smoke workflow where the result is observable through
production commands and authoritative state. Real LV1 hardware that exhibits
the missed-recall behavior remains the authoritative check for rapid multi-scene
recall order.

Final verification must run:

```bash
cargo nextest run -p advanced-show-control scenes
cargo nextest run -p advanced-show-control cue_lists
cargo nextest run -p advanced-show-control fade
cargo nextest run -p advanced-show-control lv1_actor
make check
make smoke
```

After `make smoke`, read `logs/debug-smoke-report.txt` and use that report as the
authoritative suite result. Do not infer smoke success from terminal output or
exit status alone. Report any unavailable hardware or environment limitation
explicitly rather than claiming the smoke passed.

## Documentation Changes

Update `docs/architecture.md` to document:

- The `scenes`-owned bounded recall FIFO and delayed reply semantics.
- The show-owned latest-value lockout reader used by recall validation.
- The transfer of one absolute completion deadline from scene observation wait
  to Fade's existing post-recall ping barrier.
- Abort All cancellation of both fades and delayed recall intent.
- Runtime-only scene-observation sequencing used to establish a dispatch
  boundary.

No frontend contract or persisted show-file schema documentation changes are
required.

## Alternatives Considered

### Move Readiness Ownership To Scenes

`scenes` could count pings and command Fade to pause, resume, or abort. This would
place queue and readiness state together but would broadly relocate established
fade timing and timeout safety. It also creates more cross-actor commands around
every target transition. The design rejects this because issue #61 does not
require changing Fade ownership.

### Add A Dedicated Readiness Actor

A new coordinator could own pings and deadlines for both `scenes` and `fade`.
This gives the policy a distinct boundary but adds actor construction, lifecycle,
peer wiring, shutdown, and failure handling for one existing state machine. The
extra architecture is not justified.

### Duplicate Readiness Logic In Scenes

`scenes` could independently count two pings while Fade retains its current
barrier. Subscriber ordering, lag, timeout, or generation handling could then
make queue dispatch disagree with fader-write readiness. This violates the
single-definition requirement and is rejected.

## Resolved Decisions

- Every ASC recall waits, including disabled, zero-duration, and empty-scope
  recalls.
- The total capacity is eight requests, including the in-flight recall.
- One five-second timeout begins at successful LV1 dispatch.
- Invalid queued items fail individually and queue draining continues.
- Abort All cancels both fades and every queued or in-flight recall wait.
- The existing issue #35 fade abort remains in force for a post-observation ping
  timeout.
- `FadeEngine` remains the readiness owner through an explicit deadline and
  completion handoff.
