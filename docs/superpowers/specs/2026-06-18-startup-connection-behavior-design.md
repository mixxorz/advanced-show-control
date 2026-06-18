# Startup Connection Behavior Design

## Goal

Define startup and manual LV1 connection behavior so the app supports both fast reconnect to the last console and deliberate offline use.

## Current Behavior

- The React app opens the connection modal on startup.
- Startup calls `startup_auto_connect_lv1`, which reads connection preferences, refreshes LV1 discovery, and auto-connects only when a discovered system matches the remembered UUID.
- If startup auto-connect succeeds, the modal closes.
- If startup auto-connect does not find a match or fails, the modal remains open and displays errors through the existing command error path.
- When an engineer clicks a discovered system, the UI calls `connect_lv1_system` and closes the modal only when the returned snapshot is connected.
- Clicking the modal close button hides the modal without connecting, preserving offline use.

## Desired Behavior

Startup auto-connect should choose the last connected system by this order:

1. Match by UUID when the remembered system has a UUID and discovery reports the same UUID.
2. If no UUID match is found, match by hostname when the remembered system has a hostname and discovery reports the same hostname.
3. If neither produces a safe single target, leave the modal open and stay offline.

The app should not fall back to IP address and port for startup auto-connect. Reused network addresses can point to the wrong console.

## Ambiguity Handling

Hostname fallback must be conservative. If more than one discovered system has the remembered hostname, startup auto-connect should not choose one. The modal remains open so the engineer can select the intended system.

UUID matching remains the preferred path. A UUID match can auto-connect even when another discovered system shares the remembered hostname.

## Architecture

Keep the existing ownership boundaries:

- React owns modal visibility and manual connection UX.
- Tauri command code owns startup preference lookup and connection initiation.
- `ShellState` remains the source of shell-facing connection projection.
- `Lv1Actor` remains the owner of the LV1 TCP connection lifecycle.

The implementation should be a small change in the remembered startup target selection helper, plus tests. No new frontend state is required for the matching rule.

## Error Handling

- Startup connection attempts that fail should keep the modal open and surface the existing command error.
- Manual connection attempts should keep the modal open until a connected snapshot is returned.
- Discovery or preference read errors should use existing error display behavior.
- Offline use remains valid when the engineer closes the modal.

## Tests

Add or update backend tests for the startup target selection rule:

- UUID match selects the remembered system.
- Hostname fallback selects a single matching system when UUID matching is unavailable or absent.
- UUID match takes precedence over a hostname match.
- Duplicate hostname fallback produces no auto-connect target.
- No UUID or hostname match produces no auto-connect target.

Frontend tests are not required for this change because the modal already closes only after connected snapshots and remains open on command errors.
