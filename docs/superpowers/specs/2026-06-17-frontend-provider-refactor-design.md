# Frontend Provider Refactor Design

## Purpose

Reduce frontend prop drilling by moving shared app state and command actions into React context providers, while keeping Storybook development straightforward through mock providers.

The refactor is behavior-preserving. It should not change Tauri command semantics, shell state projection, safety behavior, or the current visual design.

## Current State

`App.tsx` owns runtime wiring: startup auto-connect, Tauri event listening, discovery polling, reconnect polling, app-state snapshot application, command errors, and shell-local UI state.

Most prop drilling runs through `App -> AppShell -> Header/ConnectionScreen/SceneTab`:

- `appState`
- `commandError`
- app commands such as save, load, abort, lockout, scene selection, duration edits, scope toggles, store, connect, and disconnect

Leaf components such as `StatusBadge`, `ShowFileControls`, and `DurationInput` are already small and can remain prop-driven where that keeps them easy to reuse.

## Design

Use two small contexts:

- `AppStateProvider` exposes app-wide state values: `appState` and `commandError`.
- `AppCommandsProvider` exposes typed command functions used by connected components.

Consumers use hooks:

- `useAppState()` returns app state and command error.
- `useAppCommands()` returns command handlers.

`App.tsx` remains the runtime owner. It keeps:

- Tauri startup and event subscriptions.
- Discovery refresh polling.
- Reconnect polling and timeout handling.
- Snapshot version guarding via the existing `applySnapshot` path.
- `activeTab` and `showConnection` as shell-local state.

`App.tsx` wraps `AppShell` with both providers. `AppShell` keeps explicit props only for shell-local concerns:

- `activeTab`
- `onSelectTab`
- `showConnection`
- `onOpenConnection`
- `onResume`

Connection visibility remains shell-local because Storybook needs to show the connection and main-shell states directly, and this state is UI presentation rather than backend app state.

## Component Boundaries

Connected app-level components should read shared state and commands from hooks instead of accepting drilled props. This includes:

- `AppShell`, for reconnect overlay state and conditional tabs.
- `Header`, for status, show-file state, lockout, abort, and show-file commands.
- `ConnectionScreen`, for discovered systems, command errors, connect, disconnect, and resume handoff.
- `SceneTab`, for selected scene state and scene-edit commands.
- `LogsTab`, for log state.

Small leaf components can stay prop-driven when their inputs are narrow and reusable:

- `StatusBadge`
- `ShowFileControls`
- `DurationInput`

This keeps the provider boundary at app feature components without making generic UI pieces depend on app context.

## Storybook

Storybook stories for connected components will wrap stories with mock providers rather than passing the full app runtime prop surface.

Provide a small Storybook-friendly mock provider helper that accepts:

- `appState`
- `commandError`
- partial command overrides

Any omitted command uses a safe no-op implementation. Commands that currently return success should default to `async () => true` when the component expects a boolean result.

This makes app-level stories representative of production wiring while avoiding live Tauri dependencies. Leaf component stories can remain prop-based.

## Data Flow

Runtime data flow remains unchanged:

1. Tauri commands and events produce `AppViewState` snapshots.
2. `App.tsx` applies snapshots only when their `stateVersion` is newer.
3. `AppStateProvider` exposes the current accepted snapshot.
4. Components render from `useAppState()`.
5. Components call typed handlers from `useAppCommands()`.
6. Command handlers update app state or command error through the existing command helper paths.

## Error Handling

Command errors remain centralized in `App.tsx` and exposed through `AppStateProvider`.

Command handlers keep the existing behavior:

- Clear the current command error before attempting a command.
- Apply returned snapshots through the guarded snapshot path.
- On snapshot command failure, set `commandError` and refresh app state.
- On void command failure, set `commandError`.

## Non-Goals

- No visual redesign.
- No new frontend test framework.
- No changes to backend command names or payloads.
- No changes to safety behavior, lockout handling, reconnect behavior, or shell projection.
- No broad component decomposition beyond what is needed to remove prop drilling.

## Verification

Run the frontend checks after implementation:

```bash
npm run typecheck
npm run build
```

If Storybook-specific tooling is available and relevant after the refactor, run the smallest command that proves stories still compile.
