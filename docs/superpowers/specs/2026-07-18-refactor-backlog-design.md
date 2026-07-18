# Refactor Backlog Design

## Purpose

Implement the seven open `type: refactor` issues without changing intended runtime, frontend, persistence, logging, or mixer behavior:

- [#38 Add reusable tracing assertion helper for Rust actor tests](https://github.com/mixxorz/advanced-show-control/issues/38)
- [#53 Narrow private settings persistence interfaces](https://github.com/mixxorz/advanced-show-control/issues/53)
- [#55 Unify lifecycle connection finalization paths](https://github.com/mixxorz/advanced-show-control/issues/55)
- [#56 Move connection metadata transitions into Show actor](https://github.com/mixxorz/advanced-show-control/issues/56)
- [#57 Relocate startup LV1 target matching policy](https://github.com/mixxorz/advanced-show-control/issues/57)
- [#58 Inject app-lifetime actor handles directly into adapters](https://github.com/mixxorz/advanced-show-control/issues/58)
- [#59 Consolidate scenes actor command dispatch](https://github.com/mixxorz/advanced-show-control/issues/59)

The changes tighten existing actor and module boundaries. They do not add actors, services, traits, handle convenience methods, compatibility layers, or frontend contracts.

## Implementation Order

Implement the issues in this dependency-aware order:

1. `#38` establishes shared tracing test support before later actor tests are changed.
2. `#57` moves pure connection identity policy without changing lifecycle behavior.
3. `#53` narrows the settings persistence contract used by connection finalization.
4. `#59` removes duplicated safety-sensitive scenes command handling independently of lifecycle work.
5. `#58` removes app-lifetime handle pass-throughs before the lifecycle internals are consolidated.
6. `#56` moves complete connection metadata transitions into the Show actor.
7. `#55` extracts the final production and test connection finalization path after its settings and Show dependencies have their final shape.

Commits should follow coherent, verified changes rather than an arbitrary count. Each issue must satisfy its own acceptance criteria and verification checkpoint before work begins on the next issue.

## Architecture

### Reusable Tracing Test Support

Add a crate-private `#[cfg(test)]` tracing collector under shared test support. The collector captures structured event fields, `tracing::Level`, and the complete human-readable `message` field. Tests can inspect captured events and filter by stable `event` field and level without parsing formatted output.

Use scoped tracing dispatch or subscribers rather than a process-global subscriber. Captured buffers are owned by each collector instance, so parallel `cargo nextest` processes and concurrent tests cannot leak events into one another. The helper remains test-only and does not affect production subscriber setup.

Migrate at least one existing logging-heavy actor test to the helper. Do not migrate unrelated tests merely to maximize reuse.

### Startup Target Policy

Move `startup_auto_connect_target` and its pure tests from `lifecycle` to `connection_state`, beside `Lv1SystemIdentity` and `DiscoveredLv1System`. Keep it as a direct pure function.

Preserve all matching behavior:

- Consider only available discovered systems.
- Prefer an exact UUID match.
- Fall back to an exact trimmed hostname only when one available system matches.
- Reject empty hostnames and ambiguous hostname matches.
- Do not match by address and port.

Lifecycle remains responsible for loading settings and discovery state, deciding whether to advance or abort a generation, and dispatching the selected connection.

### Private Settings Persistence

Keep `PersistedSettings` private to the settings persistence implementation instead of re-exporting it through the settings module facade. The persisted document remains distinct from public `AppSettings` and continues to contain private remembered LV1 identity metadata.

Change `SettingsCommand::SetLastConnectedLv1` to reply with `Result<(), String>`. The state layer continues returning whether it wrote a changed identity internally, and the actor discards that boolean when replying. The mailbox caller does not receive or consume `changed`. A stale expected generation remains a successful no-op and cannot replace remembered identity.

Keep `SettingsCommandResult { changed }` for frontend-facing full settings replacement. Replacing public settings must continue preserving remembered identity, and remembered identity must not be projected through settings events.

### Scenes Command Dispatch

Extract one private asynchronous command dispatcher from `run_scenes_actor`. It receives each explicit `ScenesCommand` plus the actor-owned state and dependencies needed by existing command implementations. Its return value explicitly tells the loop to continue or shut down.

Both the pending-observation and normal actor-loop branches delegate mailbox commands to this dispatcher. Event handling, `tokio::select!` structure, pending-scene deadlines, settings refresh, and observation processing remain in the actor loop. Mailbox closure and `ScenesCommand::Shutdown` remain explicit shutdown paths.

Each command variant has one production match implementation. Replies, persisted-edit flags, projection reasons, logging, peer access, and recall validation remain unchanged.

### Direct App-Lifetime Handle Injection

Tauri command adapters that operate on Show or settings receive `State<ShowStateHandle>` or `State<SettingsHandle>` directly. Menu and debug paths retrieve the managed Show handle directly from `AppHandle`. The handles are already managed during Tauri setup.

Remove `AppLifecycle::current_show` and `AppLifecycle::current_settings`. Lifecycle retains its private Show and settings clones for runtime construction and cross-actor connection orchestration.

All callers continue to construct explicit command variants, create reply channels, send through dumb actor handles, and map errors at the adapter boundary. No business logic moves into adapters or handles.

### Show-Owned Connection Metadata Transitions

Replace lifecycle-owned sequences of low-level Show metadata commands with complete Show-owned transition commands:

- A successful connection transition clears pending identity, establishes connected identity, resets reconnect state, calculates the aggregate `changed` result, and publishes at most one connection metadata projection fact.
- A failed normal or startup connection transition clears connected identity, pending identity, and reconnect state.
- A failed reconnect transition preserves connected identity while clearing pending identity and reconnect state.

The Show state layer owns each atomic state transition and changed calculation. The Show actor owns projection publication. Remove low-level metadata commands when no independent production caller remains, and update actor mailbox tests to use the complete transitions.

Lifecycle chooses a transition only after the relevant LV1 validation result. Metadata must never indicate a successful connection before fresh LV1 validation succeeds.

### Unified Connection Finalization

Extract one lifecycle-private accepted-connection finalization operation used by both `connect_to_identity` and lifecycle tests. Production construction can still build actors before validation, while tests can supply controlled actor handles and tasks. Test hooks remain outside or narrowly around the shared production operation.

The successful sequence remains explicit:

1. Confirm a fresh LV1 snapshot reports `Connected`.
2. Send the Show-owned successful metadata transition.
3. Confirm the runtime generation is still current.
4. Install the accepted scenes handle into lifecycle, Show, and cue-list peers.
5. Abort only the stale transaction's handles if generation acceptance fails.
6. Persist remembered LV1 identity through the generation-guarded settings command.
7. Start the scenes actor task.

Fresh LV1 snapshot validation remains immediately before the accepted finalizer so failed and accepted paths stay distinct. Both production and lifecycle tests invoke the same finalizer for accepted metadata, installation, persistence, cleanup, and task startup.

## Error Handling And Safety

These refactors preserve the existing safety model:

- Generation validation happens before accepted peer installation and remembered-identity persistence.
- A stale transaction aborts only its own handles and cannot clear newer-generation peers.
- Failed validation cannot install scene automation or project successful connection metadata.
- Remembered-identity persistence failure is non-fatal after a successful connection and keeps its generation-guarded user-facing error log.
- Failed reconnect preserves the last connected identity only in the existing reconnect mode.
- Settings write failures do not mutate in-memory persisted settings state.
- Scenes command dispatch preserves fresh LV1 state acquisition, lockout, exact scene identity checks, generation guards, pending-scene timing, blocked/skipped behavior, and fade ownership behavior.
- No refactor bypasses actor mailboxes or moves domain logic into Tauri adapters or actor handles.

## Testing Strategy

Use only the repository's approved Rust test categories.

### `#38`

Use pure/unit tests for event capture, stable event field filtering, level filtering, complete message access, collector isolation, and scoped parallel-safe dispatch. Migrate one existing actor logging test to demonstrate the helper.

### `#57`

Move the existing pure unit tests with the target policy. Preserve coverage for availability filtering, UUID precedence, exact trimmed hostname fallback, ambiguity rejection, missing matches, and no address-only fallback. Retain lifecycle actor-style coverage for orchestration.

### `#53`

Use pure settings-state tests for document persistence and actor mailbox tests for the narrowed reply, remembered identity privacy, stale-generation rejection, and failed-write behavior.

### `#59`

Use existing actor tests through the scenes mailbox and `AppEventBus`. Verify representative commands while no scene observation is pending and while the settle timer is active. Preserve shutdown and mailbox-close behavior.

### `#58`

Use existing Tauri command and menu tests, type checking through compilation, and the Rust build. Frontend command names, arguments, and return types remain unchanged.

### `#56`

Use Show actor mailbox tests for successful connection, failed normal/startup connection, and failed reconnect transitions. Assert resulting projection state, aggregate changed results, and at most one projection fact for each changed transition. Retain lifecycle actor-style tests proving transitions occur only after fresh LV1 validation.

### `#55`

Use lifecycle actor-style tests through actor mailboxes and `AppEventBus`. Preserve coverage for accepted connection ordering, stale-generation rejection, peer installation, stale cleanup, remembered-identity persistence failure, and scenes task startup.

## Verification Checkpoints

Before moving from one issue to the next:

1. Run the issue's targeted `cargo nextest` tests.
2. Run Rust formatting and the smallest relevant clippy or build checks.
3. Run `make smoke` once.
4. Read `logs/debug-smoke-report.txt` as the authoritative result.
5. Fix any regression before continuing. A failed run may be repeated as needed to prove the fix.
6. Commit the coherent verified change.

After the seventh issue:

1. Run `make check`.
2. Run the seventh issue's `make smoke` checkpoint.
3. Read `logs/debug-smoke-report.txt` before claiming completion.
4. Fix failures and rerun the checks needed to prove each fix.

The terminal exit status or captured smoke output alone is not evidence that the hardware smoke suite passed.

## Documentation And Issue Completion

Update architecture or coding-convention documentation only if implementation reveals that the documented ownership model is inaccurate. These changes are intended to make code conform more closely to the current documents, not introduce a new architecture.

An issue is complete only when its acceptance criteria, targeted checks, and successful smoke checkpoint are satisfied. Close or reference issues through coherent commits after verification.
