# Startup And Connection UX Design

## Goal

Improve startup and LV1 connection UX so the app starts from a clear system chooser, can auto-connect to the last known LV1 system when it is available, and handles unexpected disconnects without leaving the user in an unclear state.

## Approved Behavior

- The app opens on a full-screen Connection screen unless launch auto-connect succeeds.
- Connection is a top-level app mode, not a tab.
- The main app keeps only the normal working screens, such as Scene and Logs.
- The last main screen is in-memory React state only. It is not persisted.
- On launch auto-connect success, the app enters the main app on the Scene screen.
- The header connection status opens the full-screen Connection screen while connected.
- Opening Connection while connected does not disconnect from LV1.
- Manual disconnect goes directly to the Connection screen.
- Unexpected LV1 disconnect shows a minimal top-level `Reconnecting...` dialog over the current screen.
- Unexpected reconnect retries for 15 seconds.
- Reconnect success closes the dialog and leaves the user on their previous main screen.
- Reconnect timeout closes the dialog and navigates to Connection.
- There is no soft connection lock. Users may intentionally switch systems or disconnect during a fade. Existing disconnect safety and generation guards remain responsible for stopping sends and blocking stale runtime writes.

## Connection Screen

The Connection screen is a system chooser and status view.

Each discovered LV1 row shows:

- Machine hostname
- IP address
- Port number
- Latency or discovery response timing
- Connection status for that system, such as `Connected`, `Available`, `Connecting`, or `Unavailable`

Row behavior:

- Tapping an available row immediately attempts to connect to that LV1 system.
- The UI stays on Connection while the connection is pending.
- On successful connection, the UI leaves Connection and returns to the last main screen.
- Tapping the currently connected system resumes the main app without reconnecting.
- Tapping a different system while already connected switches systems through the normal disconnect/connect runtime path.
- Failed manual connection leaves the user on Connection and shows a command/status error for the failed attempt.

Discovery behavior:

- Discovery polling runs while the Connection screen is visible.
- Discovery polling does not run continuously while the user is in the main app.
- Startup performs the discovery needed for launch auto-connect.
- Discovery failures do not crash the app. The Connection screen remains visible and refreshes on the next successful scan.

## Last Connected Preferences

Persist connection preferences in Tauri's app config directory as an app-local preferences file. This is separate from `.lv1show` show files.

Recommended file:

```text
<tauri app config dir>/preferences.json
```

The file contains connection preferences only for now:

```json
{
  "lastConnectedLv1": {
    "uuid": "uuid-from-discovery",
    "host": "LV1-FOH",
    "address": "192.168.1.35",
    "port": 50000
  }
}
```

Rules:

- Preferences update only after a successful connection.
- UUID is the primary identity when present.
- If a remembered UUID exists, launch auto-connect only matches by UUID.
- If the remembered UUID is not discovered at launch, the app quietly remains on Connection.
- The app does not fall back to host/IP for auto-connect when a UUID exists.
- Host, address, and port may still be stored for display and diagnostics.
- Changing show files does not affect the remembered LV1 system.

## Runtime Architecture

Add a backend-owned connection coordinator around the existing LV1 runtime setup.

The coordinator owns:

- Discovery scans requested by the visible Connection screen
- The one startup scan used for auto-connect
- Last-connected preference load/save
- Launch auto-connect decision-making
- Unexpected disconnect reconnect attempts
- UI-visible connection-system list state

The existing connected runtime remains responsible for:

- Creating and owning `Lv1Actor`
- Creating and owning `FadeEngine`
- Installing and clearing `AppCommandBus` targets
- Running the shell-state projector
- Running `SceneRecallFader`
- Applying generation guards
- Stopping unsafe/stale sends after disconnect or reconnect

The coordinator may request connect, disconnect, and reconnect actions, but it must use the same runtime setup and teardown path as existing manual commands. It does not bypass fade safety, generation guards, scene validation, or disconnect cleanup.

Lockout is unrelated to this feature. Lockout remains a scene/fade automation control and is not read or enforced by the connection coordinator.

## Reconnect Flow

Unexpected LV1 disconnect differs from manual disconnect.

Manual disconnect:

- Tears down the current runtime through the normal disconnect path.
- Navigates to the full-screen Connection screen.
- Does not show the reconnect dialog.

Unexpected disconnect:

- Immediately stops LV1 sends through the existing disconnect behavior.
- Shows a top-level modal/dialog with only `Reconnecting...`.
- Attempts to reconnect to the same remembered UUID for 15 seconds.
- On success, restores the connected runtime and closes the dialog.
- On timeout, closes the dialog and navigates to Connection.

The reconnect dialog is not a navigation target. It overlays the current main screen and preserves the user's previous main screen if reconnect succeeds.

## UI State Model

React owns local navigation state:

- `connection` full-screen mode visible or hidden
- current main screen, such as `scene` or `logs`
- reconnect dialog visibility, driven by backend status

Backend app state exposes enough connection data for the UI to render:

- Current app connection state
- Currently connected LV1 identity, if any
- Discovered LV1 systems
- Per-system hostname, address, port, latency/response timing, and status
- Pending connection target, if any
- Reconnect state, including whether the top-level dialog should be visible

The UI should not own connection safety decisions. It sends explicit user intent to backend commands, and backend state determines the result.

## Testing

Backend tests should cover:

- Preferences load/save round trip for the last connected UUID.
- Preferences update only after successful connection.
- Launch auto-connect connects when the remembered UUID is discovered.
- Launch auto-connect does nothing silently when the remembered UUID is absent.
- Launch auto-connect does not fall back to host/IP when a UUID exists.
- Discovery projection includes hostname, address, port, latency/response timing, and per-system status.
- Manual disconnect does not enter reconnect mode.
- Unexpected disconnect enters reconnecting and either reconnects or times out after 15 seconds.
- Switching systems uses the same disconnect/connect runtime path and preserves generation guard behavior.

Frontend tests or component-level coverage should cover:

- Connection renders as a full-screen mode, not a tab.
- Header connection status opens Connection while connected.
- Selecting a row keeps the user on Connection until successful connection.
- Selecting the connected row resumes the main app without reconnecting.
- Launch auto-connect success lands on Scene.
- Successful manual connection returns to the last in-memory main screen.
- Unexpected reconnect shows only the minimal `Reconnecting...` dialog.

## Out Of Scope

- Persisting the last main screen.
- Host/IP fallback auto-connect when a UUID exists.
- Manual host/port primary workflow.
- Soft connection lock during fades or other actions.
- Changing lockout behavior.
- Companion, HTTP, or WebSocket control changes.
