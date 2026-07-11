# Startup Auto-Connect Fix Design

## Goal

Restore safe startup auto-connect to the previously connected LV1 system. A fresh app launch should use persisted connection metadata, connect only to a safely matched discovered system, and leave the connection modal open when no safe match exists or connection fails.

## Root Cause

The lifecycle cutover changed `startup_auto_connect_lv1` to read `ShowState.connected_lv1_identity`. That state describes the current process and is empty on a fresh launch. The existing persisted connection-preferences module became orphaned, so startup returns an unchanged result before discovery or connection begins.

## Persistence Ownership

`SettingsActor` will own remembered LV1 identity as app-lifetime metadata. The identity will be persisted in the existing app-config `settings.json` file under `lastConnectedLv1`.

The persisted settings document will retain the existing flat public setting keys and add `lastConnectedLv1` at the same top level. Its internal Rust representation will distinguish between:

- Public `AppSettings`, which are projected to and replaced by the frontend.
- Private remembered connection metadata, which is read and updated only through dedicated settings actor commands.

This boundary prevents a frontend full-object settings replacement from clearing or replacing the remembered identity.

No settings migration or compatibility path is required at this stage. The app will not import the old `preferences.json` file. If `settings.json` cannot be parsed as the current persisted settings document, the whole document is discarded in memory and the app starts from default public settings with no remembered LV1 identity. The next successful settings or identity update writes the current schema.

The obsolete `connection_preferences` module will be removed so the app has one settings persistence mechanism.

## Remembered Identity Updates

A connection identity is remembered only after the current generation has successfully installed a connected LV1 runtime. Failed, cancelled, or stale connection attempts must not replace the remembered identity.

Remembering the identity is a settings actor operation. Lifecycle sends an explicit settings command after successful runtime installation. A persistence failure is logged visibly but does not turn an established connection into a failed connection result or tear down its valid runtime. The in-memory remembered identity remains unchanged when its write fails.

Disconnecting does not erase the remembered identity. Failed startup attempts also preserve it so a later launch can retry.

## Startup Flow

Startup auto-connect follows this sequence:

1. Request the remembered LV1 identity from `SettingsActor`.
2. If no identity is stored, return an unchanged result without starting a connection.
3. Refresh LV1 discovery through the existing show-owned discovery path.
4. Consider only discovered systems currently marked available.
5. Select a safe target using the matching policy below.
6. If no safe target exists, return an unchanged result and remain offline.
7. Abort any current runtime, begin a new generation, and connect to the discovered target through the existing lifecycle path.
8. Let the connected projection close the startup connection modal through the existing frontend status listener.

Discovery failures and connection failures return an error to the frontend. The modal remains open and manual connection remains available.

## Matching Policy

Target selection is conservative and deterministic:

1. If the remembered identity has a UUID, prefer an available discovered system with the same UUID.
2. If no UUID match exists, trim the remembered hostname and select by exact trimmed hostname only when exactly one available discovered system matches.
3. If neither rule produces one safe target, do not auto-connect.

Startup must not match by IP address and port. Reused network addresses could otherwise connect to the wrong console. A UUID match takes precedence over hostname fallback even when another system shares the remembered hostname.

## Frontend Behavior

No new frontend state machine is required. `AppRuntime` already:

- Opens the connection modal in startup mode.
- Subscribes to `app-status-changed` before requesting startup auto-connect.
- Closes only the startup modal after receiving a connected projection.
- Keeps the modal open and displays command errors when startup auto-connect fails.

The fix will preserve those behaviors. A no-match result leaves the startup modal open without presenting a false connection error.

## Safety Constraints

- Preserve lifecycle generation guards and stale-runtime rejection.
- Do not install or retain handles from a stale startup attempt.
- Do not send fader commands before a valid connected runtime is installed.
- Do not infer console identity from address and port.
- Do not project remembered identity as currently connected identity.
- Do not clear remembered identity on disconnect or a failed startup attempt.
- Do not let persistence metadata updates bypass the settings actor.

## Logging

Use `tracing` at the owning layer with stable event fields.

- Matching details and no-match decisions are diagnostic `DEBUG` events to avoid noisy startup UI logs.
- Discovery or connection failures remain visible through the existing command failure path.
- Failure to persist a newly connected identity is an `ERROR` with a complete user-facing message, while the established connection remains usable.
- Do not log duplicate connection success facts from settings and lifecycle layers.

## Tests

### Pure Unit Tests

- A missing settings file loads default public settings with no remembered identity.
- A settings file that does not parse as the current document resets wholesale to defaults.
- Remembered identity round-trips in the internal settings document.
- Public settings replacement preserves remembered identity.
- UUID matching selects the discovered UUID identity even if host or address changed.
- A unique exact trimmed hostname match is selected when no UUID match exists.
- UUID matching takes precedence over hostname fallback.
- Duplicate available hostnames produce no target.
- Unavailable systems are ignored.
- Address and port alone never produce a target.

### Actor Tests

Interact through the settings actor mailbox to prove:

- Remembered identity can be stored and retrieved.
- Storing identity persists it to `settings.json`.
- Replacing public settings does not erase identity.
- Persistence failures return an error and do not publish misleading success state.

### Lifecycle Tests

Use lifecycle commands, actor mailboxes, `AppEventBus`, and tracing capture as applicable to prove:

- Startup without remembered identity is an unchanged no-op.
- Startup refreshes discovery and selects a UUID match.
- Startup uses only an unambiguous hostname fallback.
- No match or ambiguous match does not begin a connection generation.
- A failed startup connection preserves remembered identity.
- A stale attempt cannot install a runtime or overwrite remembered identity.
- A successful manual or startup connection stores the confirmed identity.

### Frontend Tests

Retain the existing Vitest coverage proving:

- Connected projection closes the startup modal.
- Startup command failure keeps the modal open and displays the error.
- Offline startup leaves the modal available for manual selection.

No Storybook or visual regression changes are required because the visible modal states do not change.

### Hardware Smoke Test

Extend the debug smoke suite with a single-process startup auto-connect scenario:

1. Complete the existing manual connection path so the confirmed LV1 identity is stored through production commands.
2. Disconnect through the production disconnect command.
3. Invoke the production `startup_auto_connect_lv1` command without restarting the debug app.
4. Observe production `app-status-changed` snapshots until the runtime reports connected.
5. Assert that the connected identity has the same UUID as the originally connected LV1 system.
6. Assert that the runtime remains usable by completing the next existing production-command smoke operation.

This smoke scenario verifies live discovery, safe identity matching, lifecycle reconnection, and connected-state projection. It does not claim to verify reloading `settings.json` across an app restart; settings actor persistence tests provide deterministic coverage for writing and loading the remembered identity.

## Verification

Run targeted settings and lifecycle Rust tests, the `AppRuntime` frontend tests, and then the standard non-visual repository verification. Run `make smoke` against an LV1-compatible target and inspect `logs/debug-smoke-report.txt` for the authoritative result.

## Related Work

- GitHub issue #43
- `docs/superpowers/specs/2026-06-18-startup-connection-behavior-design.md`
- `src-tauri/src/lifecycle/mod.rs`
- `src-tauri/src/settings/`
- `ui/src/AppRuntime.tsx`
