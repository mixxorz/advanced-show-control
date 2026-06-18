# Connection Behavior Design

## Goal

Define startup, manual, and visible LV1 connection behavior so the app supports fast reconnect to the last console, clear connected/offline status, deliberate offline use, and reliable modal-based connection selection.

## Current Behavior

- The React app opens the connection modal on startup.
- Startup calls `startup_auto_connect_lv1`, which reads connection preferences, refreshes LV1 discovery, and auto-connects only when a discovered system matches the remembered UUID.
- If startup auto-connect succeeds, the modal closes.
- If startup auto-connect does not find a match or fails, the modal remains open and displays errors through the existing command error path.
- When an engineer clicks a discovered system, the UI calls `connect_lv1_system` and closes the modal only when the returned snapshot is connected.
- Clicking the modal close button hides the modal without connecting, preserving offline use.
- The top bar has a connection state indicator and a connected-system button. The button should open the connection modal.
- The connection modal shows discovered LV1 systems with name, IP address, port, latency, availability, and connected state.

## Startup Behavior

On app open, the connection modal should automatically open. Startup should then attempt auto-connect only when the remembered last connected system is safely available. If auto-connect succeeds, the modal closes. If the remembered system is not available, ambiguous, or the connection attempt fails, the modal remains open.

The engineer may close the modal with the close button without connecting. That is a valid offline state, and the rest of the UI should continue to support offline use.

## Auto-Connect Matching

Startup auto-connect should choose the last connected system from discovered systems that are available to connect by this order:

1. Match by UUID when the remembered system has a UUID and discovery reports the same UUID.
2. If no UUID match is found, match by hostname when the remembered system has a hostname and exactly one available discovered system reports the same hostname. Hostname matching should use the exact discovered hostname string after trimming surrounding whitespace; do not invent fuzzy matching or partial matching.
3. If neither produces a safe single target, leave the modal open and stay offline.

The app should not fall back to IP address and port for startup auto-connect. Reused network addresses can point to the wrong console.

## Manual Connection Behavior

When the engineer clicks an available system in the connection modal, the app should begin connecting to that system. The modal must not close until the connection has been successfully established and the returned app snapshot reports `connection === "connected"` with `connectedLv1Identity` matching the selected system.

If manual connection fails, the modal remains open and displays the error. The engineer can retry, choose another system, or close the modal to continue offline.

Clicking the row for the currently connected system should close the modal without reconnecting because the requested state is already established.

Unavailable systems should not start a connection attempt. They may remain visible for clarity, but the UI should make them look disabled or non-actionable.

## Top Bar Behavior

The top bar connection state indicator should reflect the current app connection state:

- Connected state shows a connected status and the current connected system name when available.
- Disconnected state shows an offline status.
- Connecting state should not be presented as connected; it should remain visibly in-progress or otherwise not mislead the engineer.

The connected-system button in the top bar should open the connection modal. This should work while connected, connecting, and offline, so the modal remains the single place to review available systems and choose a connection.

## Connection Modal Display

Each discovered LV1 system row should display the information available from discovery and app state:

- System name or a fallback label when no host name is available.
- IP address and port.
- Latency when available, otherwise a clear placeholder.
- Availability status.
- Connected status for the currently connected system.

The currently connected system should be visually highlighted in blue using the existing console design language. Other available systems should not use the connected highlight. Unavailable systems should remain visually distinct and should not look selectable as a successful target. A system is the currently connected system when its identity matches `connectedLv1Identity`, preferring UUID comparison when both sides have UUIDs and otherwise comparing hostname, address, and port together.

## Ambiguity Handling

Hostname fallback must be conservative. If more than one available discovered system has the remembered hostname, startup auto-connect should not choose one. The modal remains open so the engineer can select the intended system.

UUID matching remains the preferred path. A UUID match can auto-connect even when another discovered system shares the remembered hostname.

## Architecture

Keep the existing ownership boundaries:

- React owns modal visibility and manual connection UX.
- Tauri command code owns startup preference lookup and connection initiation.
- `ShellState` remains the source of shell-facing connection projection.
- `Lv1Actor` remains the owner of the LV1 TCP connection lifecycle.

The implementation should preserve the existing ownership boundaries and avoid new connection owners. The backend change should stay focused on remembered startup target selection. The frontend change should stay focused on modal visibility, top-bar entry points, status presentation, and test coverage around existing command behavior.

Frontend app lifecycle tests should avoid direct Tauri module mocks when practical. Extract the stateful app runtime boundary behind injectable command and event functions, then keep the default `App` component as the production Tauri wiring layer.

## Error Handling

- Startup connection attempts that fail should keep the modal open and surface the existing command error.
- Manual connection attempts should keep the modal open until a connected snapshot is returned.
- Discovery or preference read errors should use existing error display behavior.
- Offline use remains valid when the engineer closes the modal.
- Connection UI must not imply that fader writes or LV1 control are available while offline or connecting.

## Tests

Add or update backend tests for the startup target selection rule:

- UUID match selects the remembered system.
- Hostname fallback selects a single matching system when UUID matching is unavailable or absent.
- UUID match takes precedence over a hostname match.
- Duplicate hostname fallback produces no auto-connect target.
- No UUID or hostname match produces no auto-connect target.

Add frontend behavior tests with Vitest and React Testing Library. These tests should use mock app state and commands for component-level behavior, and injected fake command/event functions for app lifecycle behavior:

- App startup opens the connection modal.
- Successful startup auto-connect closes the modal.
- Failed startup auto-connect keeps the modal open and displays the error.
- Clicking a discovered system calls the manual connect command and does not close the modal before a connected snapshot is returned.
- Failed manual connection keeps the modal open and displays the error.
- Closing the modal supports offline use.
- Clicking an unavailable system does not start a connection attempt.
- The top-bar connection indicator reflects connected, connecting, and offline states without showing false connected status.
- Clicking the connected-system/top-bar connection button opens the connection modal.
- The connection modal renders system name, IP address, port, latency, and status.
- The currently connected system is highlighted with the connected blue treatment.

Keep Storybook stories as visual state documentation for connection modal and top-bar states. Storybook `play` tests are optional and should only be used when the interaction improves the story itself. Do not rely on Storybook `play` tests as the primary proof of connection behavior. Keep the existing Playwright visual snapshot flow as visual regression coverage for stories.
