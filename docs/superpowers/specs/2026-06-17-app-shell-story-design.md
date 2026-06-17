# App Shell Story Design

## Context

Storybook is now configured for component development, but the top-level `App` still cannot be rendered there because it owns Tauri event subscriptions, command calls, timers, and app state. We need an app-level shell story without importing Tauri runtime APIs into Storybook.

## Goals

- Extract a pure `AppShell` presenter for the existing app shell rendering.
- Keep `App` as the runtime container for Tauri effects, command wiring, and state ownership.
- Add Storybook stories that render the full shell in both connection and main app modes.
- Preserve current runtime behavior.

## Non-Goals

- Do not redesign the UI.
- Do not change command behavior, polling, reconnect logic, or snapshot ordering.
- Do not add frontend behavior tests in this pass.

## Architecture

`App.tsx` remains the only component that imports Tauri APIs and command helpers. It owns `useState`, `useEffect`, `applySnapshot`, discovery polling, reconnect polling, and all runtime command callbacks.

`ui/src/components/AppShell.tsx` becomes a dumb presenter. It receives `appState`, `commandError`, `showConnection`, `activeTab`, and callback props. It renders either `ConnectionScreen` or the main shell with `Header`, tab navigation, `SceneTab`, `LogsTab`, and reconnect overlay.

`ui/src/components/AppShell.stories.tsx` renders the presenter with mock state and no-op callbacks. Stories cover connection searching, discovered systems, scene tab, logs tab, command error, and reconnect overlay.

## Data Flow

Runtime data still flows through `App` from Tauri commands/events into `AppViewState`. `AppShell` receives only props and invokes callbacks. In Storybook, mock `AppViewState` fixtures replace Tauri snapshots.

## Verification

- `npm run typecheck`
- `npm run build-storybook`

Storybook build warnings about no MDX stories or large chunks are acceptable if the build completes successfully.
