# Debug Smoke Auto-Discovery Design

## Purpose

The debug smoke-test app should reduce manual LV1 connection setup by auto-filling the LV1 identity fields from the first discovered LV1 system. Auto-discovery is a convenience only: it must not auto-connect to LV1, run smoke tests, or bypass any production discovery, connection, generation, lockout, or scene validation behavior.

## Scope

This design applies only to the development-only Tauri debug smoke-test frontend. Production app behavior remains unchanged.

In scope:

- Start LV1 discovery when the debug smoke app opens.
- Use existing `refresh_lv1_discovery` and `app-status-changed` projection data.
- Auto-fill the debug LV1 identity fields from the first discovered LV1 system.
- Stop auto-fill after the user manually edits any LV1 identity field.
- Show lightweight discovery status in the debug UI.

Out of scope:

- Auto-connecting to LV1.
- Automatically running smoke tests.
- Adding debug-only backend discovery commands.
- Changing production connection modal behavior.
- Adding a discovered-system picker or dropdown.

## User Experience

When the debug smoke app loads, it starts discovery through the existing `refresh_lv1_discovery` command. While no system has been discovered, the panel shows a status such as `Searching for LV1 systems...`.

When the first discovered LV1 system appears in the projected app state, the debug form fills:

- `uuid`
- `host`
- `address`
- `port`

The app then shows a status such as `Auto-filled from discovered LV1: <host> <address>:<port>`.

The engineer still presses the existing Connect or smoke-test buttons explicitly. Auto-fill never initiates a connection or test run.

If the engineer edits any LV1 identity field, auto-fill is disabled for the current debug app session. Later discovery updates must not overwrite manually entered values.

## Architecture

The implementation should use the existing production discovery and projection path:

```text
debug frontend
  -> refresh_lv1_discovery Tauri command
  -> ShowCommand::RefreshLv1Discovery
  -> show-owned discovery state
  -> AppEventBus Show event
  -> projector
  -> app-status-changed AppViewState
  -> debug frontend auto-fill
```

No new backend command is required. The debug frontend already subscribes to `app-status-changed`; it should read `discoveredLv1Systems` from the projected state and select the first entry.

The debug app should trigger discovery on startup and may continue refreshing discovery at a modest interval while auto-fill is still enabled and no LV1 identity has been filled. Once a system is auto-filled or the user manually edits an identity field, polling should stop.

## State Model

The debug UI should track whether the LV1 identity fields are still auto-fillable.

Suggested frontend state:

- `identityAutoFillEnabled`: starts as `true`.
- `identityAutoFilled`: starts as `false`, becomes `true` after auto-fill succeeds.
- `identityManuallyEdited`: starts as `false`, becomes `true` when the user edits `uuid`, `host`, `address`, or `port`.

Auto-fill may run only when:

- `identityAutoFillEnabled` is `true`.
- `identityManuallyEdited` is `false`.
- The latest projected `discoveredLv1Systems` list has at least one system.

Manual edit wins over discovery. If a user edit and a discovery update happen close together, the implementation should preserve the user's edited value.

## Error Handling

Discovery failures should not block manual use of the debug smoke app. If `refresh_lv1_discovery` rejects, the UI should show a lightweight status such as `LV1 discovery failed; enter identity manually.` and leave the fields editable.

Auto-fill should ignore incomplete discovered identities. If the first discovered system is missing a required identity field, the UI should keep searching or allow manual entry instead of filling partial values.

## Safety Requirements

- Auto-discovery must not call `connect_lv1_system`.
- Auto-discovery must not run any smoke-test command.
- Auto-discovery must not emit `app-status-changed` directly.
- Auto-discovery must not bypass the existing `ShowCommand::RefreshLv1Discovery` path.
- Production app command registration and behavior must remain unchanged.

## Testing

Add frontend unit coverage for:

- Debug app requests discovery on startup.
- A discovered LV1 system auto-fills empty identity fields.
- Manual identity field edits prevent later discovery updates from overwriting values.
- Auto-fill does not invoke connect or smoke-test commands.
- Discovery failure leaves manual input available and reports a non-blocking status.

Backend tests are not required unless implementation changes backend behavior. If backend code is touched, add targeted Rust tests proving production command registration and discovery semantics remain unchanged.

## Verification

Run the smallest relevant checks during implementation, then run:

```bash
cd ui
npm run test -- SmokeDebugApp SmokeTestPanel
npm run typecheck
```

If backend code is changed, also run the relevant Rust targeted tests.
