# Pan Override Confirmation Design

## Context

Pan-family fades currently cancel when a pan report differs from the expected pan value by at least `PAN_OVERRIDE_THRESHOLD`. This protects live operation when an engineer grabs the pan control, but a scene reversal can produce a single stale LV1 pan echo immediately after the app changes direction. That single stale report can look like a manual override and incorrectly cancel the active pan-family fade.

Balance and width reports are already ignored for override detection. A pan report remains the only event that can cancel the pan family for a channel.

## Goal

Reduce false-positive pan-family manual overrides from one-off stale pan reports while preserving fast cancellation for real manual pan movement.

## Non-Goals

- Do not change fader override behavior.
- Do not allow balance or width reports to trigger pan-family cancellation.
- Do not change pan-family ownership: a confirmed pan override still cancels pan, balance, and width targets for the same group and channel.
- Do not add timing-based debounce logic.

## Design

Add a pan override confirmation count beside the existing threshold:

```rust
pub const PAN_OVERRIDE_CONFIRMATION_COUNT: u8 = 2;
```

Each active target gets an `override_deviation_count` field initialized to `0`. The count is meaningful only for `FadeParameter::Pan`; fader targets keep their current immediate override behavior, and balance/width targets remain non-participants.

For pan targets:

- An out-of-threshold report increments `override_deviation_count`.
- The first consecutive out-of-threshold report marks the pan target as suspect but does not cancel.
- Each out-of-threshold suspect hit emits a `tracing::debug!` diagnostic with enough context to identify the group, channel, reported value, expected value, threshold, and current confirmation count.
- A second consecutive out-of-threshold report confirms manual override and cancels the pan family for that group/channel.
- Any in-threshold pan report resets `override_deviation_count` to `0`.

The count lives on `ActiveTarget`, so it is naturally scoped per active target key: group, channel, and `FadeParameter::Pan`. Starting or replacing a fade creates a fresh active target with a fresh count. Cancelling or completing the target discards the count with the target.

When a pan-family channel has balance/width targets but no active pan target, the existing behavior stays immediate: a pan report for that channel still cancels the pan-family targets. Without an active pan target there is no expected pan value to compare against and no per-target state to confirm against.

## Data Flow

`Lv1Actor` publishes `PanChanged` facts to the event bus. `FadeEngine` handles those facts in `handle_pan_family_pan_report`:

1. If an active pan target exists for the reported group/channel, pass the reported value through the pan target's confirmation state.
2. If the report is in threshold, reset the pan target count and continue the fade.
3. If the report is out of threshold, increment the count and emit a debug tracing diagnostic for the suspect hit.
4. If the report is the first out-of-threshold report, continue the fade.
5. If the report reaches `PAN_OVERRIDE_CONFIRMATION_COUNT`, emit the existing `ChannelOverride` event and cancel pan, balance, and width targets for that channel.
6. If no active pan target exists but other pan-family targets exist for that channel, preserve the current immediate cancellation path.

## Error Handling And Safety

The change only delays pan-family cancellation for one pan report when an active pan target has an expected value. It does not bypass lockout checks, connection/generation guards, or write safety. A real manual pan grab should continue to generate out-of-threshold reports and cancel on the second consecutive report.

Existing manual override visibility remains unchanged once an override is confirmed: the engine emits `ChannelOverride` for pan and `ChannelCancelled` for each removed pan-family target.

Suspect hits are diagnostic-only and use `tracing::debug!`; they do not publish app events or frontend-facing logs. This keeps one-off stale echoes visible in diagnostic logs without alarming the operator before confirmation.

## Testing

Add or update Rust tests around `src/fade/actor.rs` and `src/fade/tick.rs`:

- One out-of-threshold pan report against an active pan target does not cancel.
- Two consecutive out-of-threshold pan reports against the same active pan target cancel the pan family.
- An in-threshold pan report between two out-of-threshold reports resets confirmation, so the second out-of-threshold report alone still does not cancel.
- Balance and width reports still do not trigger override cancellation.
- Existing fader override tests continue to prove immediate fader override behavior.
- The implementation emits a debug tracing diagnostic for each out-of-threshold pan suspect hit.

Run the smallest relevant targeted verification first, then broader Rust checks before claiming implementation complete.
