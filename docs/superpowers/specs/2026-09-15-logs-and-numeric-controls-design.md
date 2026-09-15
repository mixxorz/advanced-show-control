# Logs and Numeric Controls Design

## Goal

Refine two native GPUI surfaces without changing backend ownership or safety behavior:

1. Size Logs columns according to their content.
2. Use one editable numeric-stepper presentation for Settings values and scene X-FADE duration.

Cue-list manager refinement and automatic connection latency are deferred to GitHub issues #73 and #74.

## Logs layout

The Logs view remains a lightweight scroll list. It will not adopt the heavier interactive `DataTable` component.

Each row will use a flex layout:

- timestamp: fixed 176 px width;
- severity: fixed 88 px width;
- message: flexible width with `min_w_0`, consuming the remaining space.

The existing typography, colors, row separators, spacing, empty state, and projected log data remain unchanged. This makes the predictable metadata compact and gives operational messages most of the panel width.

## Shared editable numeric stepper

Add one native UI helper that renders the common numeric-control shell:

- decrement button on the left;
- editable GPUI `Input` in the center;
- increment button on the right;
- shared borders, dimensions, typography, hover/disabled states, and accessibility labels.

The helper owns presentation only. Parent views continue to own `InputState`, parsing, formatting, projected-state synchronization, command dispatch, and pending-command handling. This keeps the reusable unit independent of Settings and Scenes domain policy.

The control commits typed text on Enter or blur. Escape restores the current projected or optimistic value without dispatching. A valid step-button click uses the typed draft as its base; an invalid draft falls back to the current value. Step buttons commit immediately.

## Settings behavior

Settings gains one `InputState` for each numeric field. A successful edit still dispatches exactly one complete `AppSettings` replacement, preserving the `settings-full-replacement` contract and existing optimistic draft behavior.

### Fader override sensitivity

- display: integer without a suffix;
- accepted text: a base-10 integer with surrounding whitespace allowed;
- valid normalized range: 1 through 10;
- step: 1;
- an out-of-range integer is clamped;
- empty, signed-negative, fractional, or nonnumeric input is rejected and reset.

### Same-scene recall threshold

- display: integer followed by ` ms`;
- accepted text: a base-10 integer, optionally followed by `ms` in either case, with surrounding whitespace allowed;
- valid normalized range: 0 through 5000 ms;
- step: 100 ms;
- an out-of-range integer is clamped;
- empty, signed-negative, fractional, or nonnumeric input is rejected and reset.

Projected settings remain authoritative. While a complete-object optimistic draft is pending, matching projected state acknowledges it as before. A rejected command restores both numeric inputs from the latest projected settings.

## Scene X-FADE behavior

The scene editor replaces its bespoke input and `+1S`/`−1S` buttons with the shared decrement/input/increment shell. Existing duration semantics remain unchanged:

- display seconds with one decimal place and an `s` suffix;
- accept finite, nonnegative decimal seconds with an optional `s` suffix;
- normalize zero to 0 ms;
- clamp positive values to 0.1 through 120.0 seconds and round to milliseconds;
- step by 1 second;
- dispatch through the existing scene-duration command;
- preserve the latest pending duration draft across intermediate projections;
- restore projected state after invalid input or command failure.

Changing the control's visual shell must not change scene identity handling, actor ownership, or fade behavior.

## Error handling and state synchronization

Invalid text never dispatches a command. The field resets to its current authoritative or optimistic formatted value. Backend Settings normalization and Scenes duration validation remain authoritative.

The shared helper does not hold domain state and cannot mutate actor-owned values directly. Settings continues to send full-object replacements; Scenes continues to send its explicit duration command.

## Testing

Use the repository's allowed Rust test styles:

- **Pure unit tests:** Settings parsers, formatters, and stepping; existing X-FADE parsing/stepping behavior; any extracted shared presentation decisions that are meaningful without GPUI.
- **Native GPUI component tests:** type and commit both Settings numeric fields, reject invalid drafts, exercise step buttons, and verify X-FADE uses equivalent typed/step interactions through rendered controls. Tests interact through GPUI controls and command adapters rather than private actor mutation.
- **Native visual tests:** inspect and update the Logs and Settings snapshots, plus an X-FADE-containing scene view if the existing fixture exposes the control.

Run targeted `cargo nextest` checks during development, then `make check` and `make visual-test` before completion.

## Out of scope

- Sorting, resizing, filtering, or virtualizing Logs.
- Changes to Settings ranges, defaults, persistence format, or command ownership.
- Changes to X-FADE duration rules or fade execution.
- Cue-list manager changes (#73).
- Automatic connection-latency changes (#74).
