# Shell State Projection Efficiency Design

## Context

The Tauri shell currently projects every active-generation LV1 event into a full `app-status-changed` snapshot. High-rate runtime events such as fader, pan, balance, width, and mute changes can flood the frontend with full `AppViewState` payloads. The shell also emits a separate raw `lv1-event`, but the current React UI only listens to `app-status-changed`.

## Decision

Remove raw `lv1-event` delivery from the Tauri shell. The frontend-visible runtime contract should be `app-status-changed` only.

The shell projector should still apply every active-generation runtime event to `ShellState`, but frontend snapshot emission should be bounded uniformly. Runtime events should be coalesced and emitted on a 10 Hz cadence rather than special-casing immediate frontend updates.

## Frontend Projection Cadence

The frontend should receive `app-status-changed` snapshots at most 10 times per second while runtime changes are pending. This cadence applies to all runtime event categories, including connection changes, scene changes, fade events, diagnostics, scene recall policy events, and routine fader or pan-family updates.

The UI is not the safety decision-maker. Runtime safety remains in the backend actors and command paths. A 10 Hz projection cadence is still faster than practical human reaction time and is sufficient for displaying safety-relevant state without flooding the UI.

## Runtime State Application

All active-generation runtime events should update `ShellState` as they are received. The projector should avoid building a full `AppViewState` snapshot for every event. Instead, event handlers should apply state changes, mark projection as dirty, and let the projector emit one coalesced snapshot on the next 10 Hz tick.

The projector loop must keep draining the runtime event bus while waiting for the next projection tick. It should not sleep after receiving an event in a way that prevents timely bus reads, because that would increase the chance of broadcast subscriber lag.

## Testing

Tests should cover that raw `lv1-event` is no longer emitted, that multiple runtime events are coalesced into fewer frontend snapshots, and that event application continues to update `ShellState` before the next emitted snapshot.

Projection cadence tests should use Tokio's paused time facilities, such as `#[tokio::test(start_paused = true)]` with explicit time advancement, rather than real sleeps or wall-clock delays. Tests may still use condition-based polling/yielding when waiting for already-scheduled async work to run, but they should not depend on actual elapsed time for the 10 Hz interval.

## Non-Goals

- No frontend schema redesign.
- No partial snapshot or patch protocol.
- No durable event log or replay behavior.
- No changes to safety checks, generation guards, fade ownership, or LV1 command behavior.
