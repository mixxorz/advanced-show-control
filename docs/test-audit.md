# Test audit

Baseline: `54a9f75`. Scope: all Rust test modules/integration binaries, developer-tool tests, frontend unit tests, Storybook interactions, visual fixtures, and the hardware smoke runner.

The audit applies `mc-test-driven-development`: assert observable application behavior, mock external boundaries rather than implementation details, use independent expected values, and avoid tests of framework guarantees. This evaluates current test design, not historical red/green development order.

## Suite changes

| Suite | Before | After |
| --- | ---: | ---: |
| Rust workspace | 572 | 473 |
| Developer tools | 17 | 16 |
| Frontend unit cases | 166 | 159 |
| Storybook stories | 104 | 104 |
| Current visual baselines | 104 | 104 |
| Hardware smoke checks | 15 | 15 |

Counts are test cases, not behavioral assertions. Parameterized scenarios retain distinct inputs and failure conditions within fewer tests. Fifteen orphan visual baselines were removed separately; no current visual state was deleted.

## Coverage ownership

- **Fade:** pure curve/fader-law/interpolation tests own numerical behavior. Mailbox/event-bus tests own readiness, overrides, cancellation, exact-scene behavior, generation rejection, and batch completion. Private `EngineState` and side-effecting pan-handler tests were replaced or subsumed, not retained as a second test API.
- **LV1:** pure OSC/framing/parser tests own wire formats. TCP peers plus the actor mailbox own live-state updates, invalid/inapplicable reports, recall dispatch, gain/mute writes, pings, reconnect, and flush ordering. The writer task's mailbox covers pending-flush failure. Dumb sender-forwarding tests were removed.
- **Scenes/Cues:** pure domain tests own alignment, identity, scope, and document transformations. Actor tests own lockout, queues, generation handoffs, exact-once late-observation suppression, overflow fallback, and authoritative recovery after mixed-event lag. Duplicate actor-local copies of pure state tests and unused event helpers were removed.
- **Show/Settings:** real actor commands own persistence effects. Load tests no longer call private DTO helpers or bypass missing LV1 peers. Save/backup/retention/failure tests use an injected, isolated backup directory with cleanup. Settings tests verify persisted public/private state through a fresh actor and capture real update logs.
- **Lifecycle/Session:** tests retain distinct cancellation and supersession schedules, asserting mailbox routing, projection, and events rather than private holder fields. Test-only connection command variants were removed. Session projection tests use an ordered generation fact instead of arbitrary scheduler-yield loops.
- **Projector/Logging:** snapshot tests own frontend projection, generation filtering, bounded logs, and lag recovery. Logging tests exercise real tracing output rather than test-only event-name lists. Disconnect tests now seed nonempty live state; no-emission tests use paused time.
- **Frontend:** unit tests own payloads, projected-state rules, stale versions, keyboard behavior, and races. Story interactions retain composition/browser obligations and setup needed for visual snapshots. CSS-class assertions were replaced with semantic assertions or left to visual coverage. Native button forwarding and duplicate composition tests were removed.

Similar actor and wire-level scenarios remain when they protect different boundaries. Visual setup actions are fixtures, not additional claims of behavioral coverage.

## Strengthened regressions

- Lower-version disconnected UI snapshots arriving after connected state.
- Cue creation clearing a previously cued entry, rather than an already-empty field.
- Session replacement establishing a fresh recall gate.
- Invalid mute values producing neither state changes nor mute facts.
- Malformed channel diagnostics captured from an actual LV1 actor.
- Truncated OSC blobs reaching the intended decoder error rather than padding validation.
- Stale cleanup preserving newer LV1/Fade mailboxes and Scenes readiness.
- Smoke auto-connect matching the independently discovered identity.

Late-observation tests were mutation-checked: disabling overflow suppression or exact-record consumption causes the corresponding actor test to fail.

## Deliberate limits

- Deterministic OS/socket writer saturation is not forced. The small queue-full error mapping is code-reviewed; creating transport injection solely to test that mapping would add disproportionate complexity.
- Exact backup/temp filename collision suffixes are not forced with a new clock abstraction. Public backup preservation, distinct saves, retention, and failure cleanup are tested; the atomic `create_new` retry loops are code-reviewed.
- The session dispatch/replacement test queues replacement while dispatch is pending. It does not claim dispatch can finish after a committed replacement: the owner awaits dispatch before processing replacement.
- The decreasing-duration smoke check validates final targets, not duration accuracy, and is named accordingly. Hardware smoke requires a compatible LV1 target and was not executed during this audit.

Verification: `make check` and Docker-backed `make visual-test`. Test commands use project-local `TMPDIR` during agent runs; fixtures remain portable to normal Cargo/CI temporary directories.
