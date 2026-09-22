# Dirty Session Guard Design

## Purpose

Protect unsaved app-owned scene-fade and cue-list configuration when the engineer closes the app or requests New Session, New from Template, or Open Session.

## Considered approaches

1. **Presentation-owned navigation state machine (selected).** `AppRoot` owns one pending destructive action, presents Save/Discard/Cancel, sequences save and file pickers, and continues only after the save command reports success. This keeps transient intent near GPUI while Show remains authoritative for dirty/path state.
2. **Show-actor guard.** Add pending navigation to Show and have it reject destructive commands while dirty. This would protect callers but would put dialogs and application-close intent into the persistence actor and require a new backend/UI request protocol.
3. **Independent guards in each action handler.** This is smaller initially but duplicates ordering and error behavior across menu, keyboard, in-app, and close routes.

The selected state machine is the smallest deep boundary: all routes converge before destructive work, while backend persistence and LV1 behavior remain unchanged.

## Behavior

A clean session immediately continues the requested action. A dirty session opens one native GPUI prompt with **Save**, **Discard**, and **Cancel**.

- **Save:** use the existing show save command. A titled session saves to its current path. An untitled session first opens Save As. Continue the original action only after `CommandFinished` reports success.
- **Discard:** continue immediately without saving.
- **Cancel:** clear pending intent and leave the session unchanged.
- Cancelling Save As, failing to open its picker, or receiving a save error clears pending intent. The existing notification path reports errors.

New from Template and Open present their file picker only after the guard resolves. Cancelling either picker remains a no-op. Close-window, Quit action, application menu, shortcuts, and in-app menu converge on the same guard. Re-entrant destructive actions are ignored while one guard is pending.

The close callback always vetoes the platform close while dirty and schedules the guarded close. After Save or Discard, the continuation explicitly quits the single-window application. A clean close is accepted synchronously.

## Architecture and boundaries

`AppViewState.show_file_dirty` and `show_file_path` remain authoritative projected inputs. A focused pure state machine under `native_ui` models pending action and save phase; `AppRoot` supplies GPUI prompts, path pickers, command dispatch, notifications, and final continuations. Command work remains on the Tokio runtime through `CommandDispatcher`; no actor reply or file I/O blocks GPUI.

Pending navigation is presentation-only state. It is not projected and does not enter Show, Scenes, Cue Lists, lifecycle, LV1, or Fade. The guard dispatches only existing show persistence/session commands and therefore cannot recall scenes or send fader writes.

## Contracts

A declaration-level contract on the guard requires destructive actions to proceed only when clean, explicitly discarded, or successfully saved. Cancellation, picker cancellation, and save failure must clear pending intent without dispatching the destructive continuation. A close-hook contract requires dirty close requests to be vetoed until that same guard resolves.

Existing applicable subtree contracts remain unchanged, especially actor-owned state boundaries, projector snapshot authority, advisory frontend safety, and diagnostic-data minimization.

## Testing

Use pure unit tests for the state machine: clean bypass, dirty prompt, Save/Discard/Cancel, untitled Save As, canceled Save As, save success/failure, and re-entrant requests. Use native GPUI tests for close interception and action routing where practical. Tests exercise public state-machine transitions and GPUI behavior, not source strings or private actor mutation.

Run targeted `cargo nextest` filters and the native visual suite if prompt behavior affects reviewed rendering. No actor or hardware-smoke tests are needed because actor behavior and LV1/fader paths do not change.

## Documentation

Update the architecture boundary to describe pending navigation orchestration and update the user manual to replace the warning that version 2 does not protect dirty sessions.
