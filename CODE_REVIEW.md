Code Maintainability Review

Executive Summary

The architecture of this codebase is genuinely good: clear actor boundaries, a pure and exhaustively tested recall policy function, a documented event/command bus contract, atomic
show-file writes, and strong generation-guard discipline in the shell projection. The OSC codec, fader-law module, and scene-list reconciliation classifier are exemplary.

The problems cluster in three places:

1. An unfinished refactor left two verified data-integrity regressions. The extraction of show data out of ShellInner into ShowStateHandle orphaned a ~400-line test file
   (capture*tests.rs is not declared in mod.rs and no longer compiles), left a never-constructed ShowActorError forcing a Result<Result<bool, String>, *> shape, and introduced a let \_ =
   ...? pattern that silently discards all domain validation errors and never marks the show file dirty after scene-config edits. The orphaned tests assert exactly the lost behavior.
2. Safety mechanisms that rely on caller discipline rather than enforcement. The generation guard — the project's core safety invariant — is implemented as check-then-act at every call
   site (7 times in one recall function, two lock acquisitions on the bus), leaving real TOCTOU windows. The one test named for this property (stale_generation_does_not_start_fade) asserts
   nothing.
3. Errors discarded at nearly every boundary. Fade-engine LV1 write failures, /Channels parse failures, disconnect reasons, write_batch drops while disconnected, and CommandFailed bus
   events all vanish or go to stderr (invisible in a packaged Tauri app). For a tool that moves live mixer faders during shows, the field-debuggability story is the weakest part of the
   system.

There is also one straightforward reliability bug: an idle connection never detects ping timeout, because the check only runs at the top of the actor loop and the select! has no timer
branch.

Top Findings

1. Show mutators silently swallow domain validation errors

Severity: Critical
Location: src-tauri/src/app_state/shell.rs:93-181 (all six show mutators)
Category: Error Handling

Issue: Verified. Every mutator uses let \_ = self.show.set_scene_duration(...).await.map_err(...)?. The ? only propagates the outer ShowActorError — which is never constructed anywhere
(src/show/handle.rs bodies are all Ok(self.state.lock().await...)). The inner Result<bool, String> carrying real validation errors ("Fade duration must be 0 or between 100 ms and 120000
ms", "Scene config not found") is discarded, and the command returns a success snapshot to the UI.

Why It Matters: Invalid user input silently no-ops with no error banner. The orphaned capture_tests.rs asserts the opposite behavior (unwrap_err() == "Scene config not found"),
confirming this is a regression rather than a design choice.

Suggested Improvement: Propagate the inner result: bind it and apply ? a second time. Combine with finding 4 (delete the dead ShowActorError layer) and this becomes a single mechanical
cleanup.

2. Scene-config edits never mark the show file dirty

Severity: Critical
Location: src-tauri/src/app_state/shell.rs:93-181; dirty flag only set in app_state/events.rs:54,91,143,262
Category: State Management

Issue: Verified — same let \_ = discards the changed: bool that ShowState returns specifically for this purpose. Storing a scene config, changing a fade duration, or toggling
channel/FADERS/PAN scope leaves show_file_dirty == false. Only lockout, reconciliation, and load-with-pruning mark dirty.

Why It Matters: show_file_dirty drives the unsaved-changes indicator. An operator can edit fade configs and quit believing everything is saved — a direct data-loss path in a live-show
tool. The orphaned tests assert snapshot.show_file_dirty after each of these operations.

Suggested Improvement: Capture the inner Ok(changed) in each mutator and set inner.show_file_dirty = true when changed. Restore the capture tests (finding 3) to lock it in.

3. capture_tests.rs is orphaned: never compiled, stale API, ~400 lines of lost regression coverage

Severity: High
Location: src-tauri/src/app_state/capture_tests.rs; src-tauri/src/app_state/mod.rs:1-12 (no mod capture_tests; — verified)
Category: Testing

Issue: The file exists but is not declared in mod.rs, so it never compiles or runs. It references a pre-refactor API (inner.scene_configs[0]) and would fail to build if re-added. Its
assertions guard precisely the two Critical regressions above.

Why It Matters: The coverage vanished silently, and the file's presence misleads reviewers into believing it exists. Findings 1 and 2 shipped because of this.

Suggested Improvement: Add #[cfg(test)] mod capture_tests;, port the file to the current API, and fix production code until it passes. Consider a tiny CI check that every \*\_tests.rs is
declared in its mod.rs.

4. Vestigial ShowActorError double-Result enables the swallowing bug

Severity: High
Location: src/show/handle.rs:10-152 (verified: six methods return Result<Result<bool, String>, ShowActorError>)
Category: Architecture

Issue: ShowStateHandle is mutex-backed (per the architecture, deliberately not an actor), yet every method returns the actor-era ShowActorError, whose variants are never constructed.
This forces the awkward nested-Result shape and let \_ =/ok()?/unwrap_or_else noise at every call site across two crates.

Why It Matters: Dead error plumbing makes every caller handle an impossible failure while obscuring the real one. New call sites will keep copying the broken pattern.

Suggested Improvement: Delete ShowActorError and the outer Result; return Result<bool, String> directly. This mechanically simplifies ~15 call sites and makes findings 1–2 structurally
impossible to reintroduce.

5. Idle connections never detect ping timeout

Severity: High
Location: src/lv1/actor.rs:257-262 (verified)
Category: Reliability

Issue: if state.last_ping.elapsed() > PING_TIMEOUT is evaluated only at the top of the connected loop, and the select! below has no timer branch. If the peer goes silent without closing
the socket (network partition, unplugged cable) and no commands arrive, the select never wakes and the actor stays Connected indefinitely with a stale state mirror.

Why It Matters: This contradicts docs/tcp-handling.md ("ping timeouts … end the connected actor loop") and the safety rule that fader commands must not be issued against stale LV1 state
— a fade started in this window writes into a dead socket.

Suggested Improvement: Add a select branch: \_ = tokio::time::sleep_until((state.last_ping + PING_TIMEOUT).into()). Add an integration test that a silent server produces Disconnected
within roughly PING_TIMEOUT.

6. Generation guard is check-then-act everywhere, not enforced atomically

Severity: High
Location: src/runtime/commands.rs:30-77,209-217; src/scene_recall/actor.rs:111-224 (guard repeated 7×)
Category: State Management (safety-critical)

Issue: The bus exposes get_generation/set_generation as separate lock acquisitions, and no command method checks generation itself. The recall actor does get_generation() then
start_fade() — an await window in which connect can install new targets, so a stale task's fade can land on the new LV1 target. clear_targets() doesn't bump the generation; disconnect
correctness depends on callers remembering to call both. FadeCommand::RecallSceneFade carries no generation, so nothing downstream re-verifies. Today the property holds only because
three mechanisms (task abort, FadeEngine abort-on-disconnect, the guards) each almost cover it.

Why It Matters: "Generation guards must prevent stale tasks sending fader commands" is the project's first safety rule, and it currently rests on caller discipline across two lock
acquisitions — the kind of guarantee that erodes silently as call sites are added.

Suggested Improvement: Add generation-checked bus variants that compare under the same lock as the target clone (e.g. start_fade_if_generation(expected, config) -> Err(StaleGeneration)),
make clear_targets bump the generation, and migrate the recall path. Extract a publish_guarded helper in the recall actor to collapse the 7 repeated checks.

7. Safety-named test stale_generation_does_not_start_fade asserts nothing

Severity: High
Location: src/scene_recall/actor.rs:394-414
Category: Testing

Issue: The test publishes a SceneChanged and immediately aborts the actor before the 25 ms settle delay can fire, with no fake fade handle and no assertions. It passes regardless of
behavior.

Why It Matters: It is the only test naming the generation-guard property and provides false confidence about the system's most important safety invariant.

Suggested Improvement: Install the fake fade handle, advance past the settle delay and snapshot polling, and assert zero fade starts and no SceneRecall events. Also add a test where the
generation flips between policy decision and start_fade to pin the TOCTOU behavior in finding 6.

8. Fade engine swallows all LV1 write errors; zero logging in src/fade/

Severity: High
Location: src/fade/actor.rs:241-243 (let \_ = command_bus.write_batch(writes).await;); src/scene_recall/actor.rs:189-198
Category: Error Handling / Observability

Issue: Every fader write failure during a live fade is dropped — no log, no event. Compounding it, write_batch itself is fire-and-forget on the bus (src/runtime/commands.rs:195-207):
while disconnected the actor drains batches as => {} and the bus returns Ok(()), contradicting the documented contract that "if the target is unavailable, the caller gets a clear
failure." The recall actor likewise discards the actual start_fade error and publishes the fixed string "failed to start fade".

Why It Matters: A fade can appear to run while doing nothing, with no trace to diagnose after the show. This directly violates the stated rule that safety blocks must be visible through
logs or UI state.

Suggested Improvement: In send_batch, log (rate-limited) and/or publish a FadeEvent::WriteFailed; consider aborting on persistent failure. Have the actor's disconnected drain publish a
diagnostic when discarding a batch — or, if fire-and-forget is the deliberate trade-off for the 25 Hz stream, document that exception in docs/architecture.md and on the method. Include
the real error in the Blocked reason string.

9. Stale-generation race mutates show data before the guard re-check

Severity: High
Location: src-tauri/src/app_state/events.rs:218-283 (SceneListChanged handling)
Category: State Management

Issue: The flow checks generation, drops the lock, calls self.show.reconcile_scene_list(...) — which mutates app-lifetime ShowState — and only then re-checks generation to protect the
projection. A disconnect/reconnect between the check and the reconcile lets a stale event's scene list reconcile into (and potentially prune) the user's scene configs. The existing test
covers only the up-front check.

Why It Matters: Scene fade configs are the product's core data; reconciliation against a stale list can clear durations and channel snapshots via the FIFO fallback. Three separate
is_generation_current calls in one match arm are also very hard to reason about.

Suggested Improvement: Do the check-and-mutate in one critical section — either hold the inner lock across the show call (the inner → show lock order is already the convention in
snapshot()) or add a reconcile_if(expected_generation, ...) seam. Document the lock-ordering convention on ShellState.

10. Override of the last active fade target never emits a terminal event

Severity: High
Location: src/fade/actor.rs:170-181, 245-273
Category: State Management / Observability

Issue: FadeCompleted fires when targets drain via ticks, and FadeAborted via abort/disconnect — but when a manual override removes the last target, both override paths only clear the
tick interval. No terminal event is published. Existing consumers (the CLI fade-test loop in src/main.rs:740-766, any UI tracking running state from FadeStarted) wait on exactly those
terminal events and would hang.

Why It Matters: The fade lifecycle is a state machine whose "ended via override" transition is invisible; consumers can't determine fade-running state without mirroring engine internals.

Suggested Improvement: When !state.is_active() after an override removal, publish a terminal event, and add a test asserting it after overriding the sole target.

11. UI applies full snapshots from five unordered sources with no versioning

Severity: High
Location: ui/src/App.tsx:27-154; ui/src/commands.ts:35-51
Category: State Management

Issue: setAppState is fed by the app-status-changed push event, the startup invoke, a 5 s discovery poll, a 2 s reconnect poll, and every command response — each applying a full snapshot
wholesale, last-write-wins. A slow in-flight refresh_lv1_discovery resolving after a scene-recall push overwrites the UI with pre-recall state.

Why It Matters: Symptoms are intermittent and unreproducible — a toggled channel snapping back, a stale fade indicator — exactly the bug class that erodes operator trust during a show.

Suggested Improvement: Add a monotonic stateVersion: u64 to AppViewState and make a single setAppState wrapper drop snapshots whose version is not newer. One small change fixes all five
race sources.

12. Dead "duration is 0" dedup logic — three mechanisms, none live

Severity: High
Location: src/scene_recall/actor.rs:36,98,205; src/scene_recall/state.rs:36-42,118-121
Category: Maintainability

Issue: The actor gates Skipped publication on reason != "duration is 0", but decide_scene_recall never produces that reason anymore (zero-duration scenes now Start). The actor's
duration_zero_logged set can never populate, and SceneRecallState carries a second, parallel, also-unused dedup implementation plus an uncalled reset_for_generation.

Why It Matters: Three pieces of state claim to own one behavior and none does anything; the string-matched coupling between actor and policy has already drifted once and will do so
silently again.

Suggested Improvement: Delete all of it. If reason-specific handling returns, make RecallPolicyDecision::Skip carry an enum reason instead of a free-form string.

13. SceneRecallState::accepts_at mutates state on rejection paths inside an implicit state machine

Severity: High
Location: src/scene_recall/state.rs:66-116
Category: Understandability

Issue: The armed branch checks repeat suppression, overwrites last_scene/last_triggered_at with baseline values, re-checks the same condition, then overwrites again on acceptance — and
one rejection path returns false while leaving mutated state behind. The intent (the first scene seen around arming is the pre-existing scene, not a recall) is stated nowhere, and a
method named accepts mutates.

Why It Matters: This gate decides whether automation may move live faders. Four optional fields threaded through two overlapping suppression rules with double mutation is a regression
magnet for the next person who tunes the windows.

Suggested Improvement: Model it explicitly (enum RecallGate { Unarmed, Arming { baseline, deadline }, Armed { last_trigger } }) with named transitions, keep the existing tests, and
document why a baseline-equal scene shortly after arming must be suppressed.

Notable Medium findings (condensed)

- Three exhaustive Lv1Command matches with a silent flush divergence — the post-connect drain duplicates drain_commands_for except Flush replies Ok in one and Err(NotConnected) in the
  other, uncommented despite three recent flush-fix commits; the unexplained yield_now stale-command drain at the reconnect boundary looks deletable but is load-bearing
  (src/lv1/actor.rs:120-211). Extract a shared helper and document both invariants.
- Disconnect reasons computed then discarded — five distinct failure causes collapse into TcpError and Lv1Event::Disconnected carries no reason; in a reconnect loop there is no way to
  tell why the connection keeps dropping (src/lv1/actor.rs:214-227). One diagnose call fixes the worst of it. /Channels parse failures are also silently swallowed
  (src/lv1/state.rs:207-212) — the message most likely to break on an LV1 firmware update.
- Recall timing protocol entirely uncommented — four interacting empirical windows (25 ms settle, 500 ms edit suppression, 2 s arming, 500 ms repeat delay) plus a subtle dual suppression
  check, with no rationale in code or docs/scene-tracking.md. The recall loop also head-of-line blocks up to 2 s while polling for a fresh snapshot (src/scene_recall/actor.rs).
- MIN_SEND_DELTA_POS unit bug — a constant documented as fader-position space (0.0–1.0) is applied to raw pan (±100), balance, and width deltas, so pan fades send on effectively every
  tick (src/fade/tick.rs:8-9,120-128). Add per-parameter send deltas mirroring the existing override thresholds.
- Show-file field additions are 5–6-file shotgun edits with hand-rolled two-way mapping — a missed field compiles fine and silently drops data on save/load (src-tauri/src/show*file.rs,
  show_file_mapping.rs). Add From impls and one structural export→import→compare round-trip test. Also: validate_show_file silently deletes data (rename to prune*…), and backups accumulate
  unboundedly.
- scene_id "{index}::{name}" format hand-rolled in six places across two crates, parsed back with silent fallbacks that can produce a junk 0::"" config (src/show/capture.rs:38-46).
  Centralize format + parse next to scene_id() in src/show/types.rs.
- Dead ShowEvent machinery — variants declared and projected but never published in production, actively misleading about where dirty-tracking lives (src/show/events.rs). Delete or wire
  up; decide once.
- Projector mixes three inconsistent error sinks — diagnostics file, stderr, in-app log; CommandFailed goes to stderr only, invisible in a packaged app
  (src-tauri/src/commands.rs:675-781). Route it to push_log. Same for the bus lag reporter (src/runtime/events.rs:43-45).
- Blocking mDNS discovery (up to 6 s) on the async runtime during connect (src-tauri/src/commands.rs:327) while the sibling call site correctly uses spawn_blocking.
- Misleading tests — pan_family_addresses_match_expected_osc_paths asserts string literals equal themselves (src/lv1/actor.rs:459-470); a byte-identical duplicate flush integration test
  claims untested writer-ordering coverage (tests/lv1_actor.rs:422-525); the cross-module bus test syncs on a 200 ms sleep (tests/runtime_bus.rs:48-95).
- UI: hand-maintained types.ts mirrors view.rs with unchecked invoke<T> casts and no codegen; stringly-typed command dispatch; Header shows 0-based scene index while SceneTab shows
  1-based; no UI tests or ESLint at all.

Cross-Cutting Themes

1. Refactor residue. The ShowState extraction was structurally completed but behaviorally unfinished: orphaned tests, never-fired error variants, dead event plumbing, dead dedup state,
   stale #[allow(dead_code)], the unexplained cover_state_variants() hack in the production constructor. Both Critical findings are residue from this one migration.
2. let \_ = as a habit. Discarded Results appear in shell mutators (critical), fade writes (high), diagnostics, probe tooling, and tests. Each should be propagated, logged, or commented
   as intentional.
3. Safety by caller discipline. Generation guards, lag handling, and flush semantics all depend on every call site doing the right thing rather than the bus or actor enforcing it. The
   codebase already shows the cost: 7 repeated guards, 3 drifting match copies, 8 duplicated Lagged/Closed arms.
4. Error context destroyed at boundaries. Err(_), map_err(|_| ...), stringified errors, and reason-less events throughout. Field debuggability — the thing you need most at front-of-house
   — is the system's weakest property.
5. String-typed protocols. Policy reasons matched as strings, "{index}::{name}" ids, epoch-millis strings, OSC addresses duplicated as literals across files. Each has already caused or
   nearly caused drift.
6. Implicit state machines. The recall gate, the fade lifecycle, and connection status are state machines expressed as scattered Options and emptiness checks; all three produced concrete
   findings.

Recommended Refactor Plan

1. Immediate safety/correctness fixes: restore capture_tests.rs and fix the swallowed-validation-error and dirty-flag regressions (findings 1–3); add the ping-timeout select branch (5);
   fix the no-assertion generation test (7); surface fade write failures and dropped batches (8).
2. High-leverage simplifications: delete ShowActorError and the double-Result (4); delete the dead duration-zero dedup and ShowEvent machinery (12); extract the shared Lv1Command drain
   helper and the recall actor's guard/event helpers.
3. Enforce the generation invariant: generation-checked bus methods, clear_targets bumps generation, generation carried in FadeConfig (6, 9); document the lock-ordering and
   one-bus-per-connection contracts.
4. Test coverage: disconnect-aborts-active-fade; generation flip between decision and dispatch; fader-override-preserves-pan-targets; override-of-last-target terminal event; show-file
   structural round-trip; per-mutator validation-error and dirty-flag tests; replace sleep-based sync in tests/runtime_bus.rs.
5. Cleanup and consistency: centralize scene_id format/parse, OSC address strings, and the triplicated timestamp helper; per-parameter send deltas; route CommandFailed and lag reports
   into the in-app log; UI stateVersion and typed command wrappers; document the four recall timing windows and the types.ts↔view.rs sync contract.

Positive Observations

- decide_scene_recall is a pure, synchronous, exhaustively tested decision function with a clear Blocked/Skip/Start trichotomy — exactly the right shape for a safety boundary.
  Validate-before-abort holds structurally: recall never calls abort_all.
- src/osc.rs and src/fade/fader_law.rs are rigorous: strict validation, adversarial tests, measured knots with provenance comments.
- The writer-task lifecycle is inherently generation-safe — a stale writer owns the old write half and cannot touch a new connection.
- write_show_file does atomic temp-file + fsync + rename with backup-before-overwrite; the scene-list reconciliation classifier has unusually thorough ambiguity tests.
- Generation-guard coverage for the shell projection is strong and clearly intentional, and the UI's reconnect attempt/timeout protocol is well designed end-to-end, including correct
  Tauri listen cleanup under StrictMode.
- docs/tcp-handling.md accurately matches the implementation apart from the idle-ping gap; SceneBuffer's doc comment and the INITIAL_MUTE_SETTLE_MS rationale are model "why" comments.

Open Questions

1. Is write_batch's fire-and-forget semantics a deliberate trade-off for the 25 Hz fade stream? If yes, finding 8's bus half reduces to documentation; if no, it needs a failure path.
2. Is the dead ShowEvent machinery intended for the planned batched-writes work (docs: plan lv1 batched writes), or deletable?
3. Is retaining scene_list across disconnects intentional? Is the 1_000 ms default duration in capture.rs (vs 0 elsewhere) a deliberate UX choice?
4. Is SceneSummary.index 0-based on real hardware? The Header/SceneTab off-by-one depends on it.
5. Should a recall blocked by the 2 s snapshot timeout be retried or surfaced more prominently? Currently the operator's recall silently gets no fade.

The full per-subsystem findings (about 70 in total, including all the Low-severity items I omitted here) came back from five parallel reviewers; I can expand any area — lv1, fade/recall,
show/shell, runtime/probe, or UI — or start on the fix plan, beginning with the two Critical regressions.
