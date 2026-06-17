# Real Frontend Shell And Scenes Design

## Goal

Replace the current test-bed frontend with the first real application shell. The build should match the provided LV1-style console reference closely enough to establish the final product direction while keeping the implementation focused on the Scenes workflow.

This scope includes the full top-level shell, real Scenes tab, Logs tab integration, placeholder tabs for future workflows, and a persistent bottom status bar. It does not implement Playlists, Events, Sessions, or Settings behavior beyond navigable placeholder panels.

## Constraints

- Preserve the existing Tauri `AppViewState` and command contracts unless a frontend-only type is needed.
- Keep global safety and connection state visible at all times.
- The bottom status bar must show a live clock, not `lastEventAt`.
- Define frontend fonts explicitly so text renders consistently across supported operating systems. Use IBM Plex Sans for UI text and IBM Plex Mono for numeric/status text.
- Define reusable Tailwind/CSS theme variables for fonts, console colors, orange accent states, surfaces, borders, status colors, sizing, and interaction states.
- Avoid hard-coded Tailwind values when a reusable token is appropriate.
- Build component by component from smallest to largest.
- Put each React component in its own file.
- Do not implement behavior for future roadmap tabs beyond clear placeholders.

## Visual Direction

The UI should use a dark operator-console style inspired by the screenshot:

- Near-black neutral chrome with no green tint.
- Flat neutral charcoal panel and card backgrounds.
- Slightly lighter neutral charcoal section backgrounds.
- Minimal depth; avoid shadows and gradients unless a component specifically needs emphasis.
- Orange active states for selected scene, scoped buttons, and primary actions.
- Muted gray text for labels and inactive state.
- Green state text for active/current/synced statuses.
- Blue state text for cued or next-scene style status.
- Thin panel borders and compact high-density controls.
- Monospaced or tabular numeric presentation for scene numbers, durations, status values, and clock.

The shell should look purpose-built for live operation, not like a generic dashboard.

## App Shell

The app shell replaces the current simple header/nav layout with a fixed console workspace:

- Top tab bar: `Scenes`, `Playlists`, `Events`, `Sessions`, `Logs`, `Settings`.
- Main content region that fills available space.
- Persistent bottom status bar.
- Existing reconnect overlay remains visible above the shell when reconnect is active.
- Connection entry or connection screen behavior can continue using the existing app flow, but the connected app view should use the new shell.

`Scenes` is the default operational tab. Non-Scenes tabs remain navigable so the product frame is stable for later roadmap work.

## Scenes Tab

The Scenes tab is the first complete workflow in the new shell.

Left scene list:

- Shows scene index, scene name, and X-fade duration for each app scene config.
- Highlights the selected edit scene with an orange row treatment.
- Indicates the current LV1 scene with a green marker or text state.
- Keeps duplicate scene-name warnings visible because exact scene tracking matters for safety.
- Empty state remains explicit when no scenes are loaded.

Selected scene header:

- Shows selected scene number and name.
- Shows FADER and PAN scope toggles using the existing commands.
- Shows X-fade duration editing using the existing duration command.
- Shows action buttons for `Store` and other visible shell actions where behavior already exists. Unsupported actions should appear disabled or be omitted until backed by commands.

Channel scope editor:

- Uses existing `scene.channelConfigs` and `scene.scopedChannels` data.
- Groups channels using existing display group helpers.
- Renders dense console-style scope buttons.
- Scoped channels use orange active state.
- Unscoped channels use dark inactive state.
- `All` and `None` remain wired to `setAllChannelsScoped`.
- Button titles can continue exposing channel name, fader dB, and pan-family summary.

## Other Tabs

`Logs` should continue rendering existing frontend-facing log data, adapted visually to the new shell.

`Playlists`, `Events`, `Sessions`, and `Settings` should render clear placeholder panels with concise copy that the workflow is not built yet. These placeholders are part of the shell only; they must not imply behavior exists.

## Bottom Status Bar

The bottom bar remains visible across tabs and contains compact status cells:

- Mode or fade state, derived from `fadeState` and lockout where appropriate.
- Current LV1 scene from `currentScene`.
- Selected or cued scene based on `selectedSceneId` and the selected scene config.
- Connection status and connected LV1 identity when available.
- Sync or safety status derived from connection, lockout, and reconnect state.
- Live local clock.

The clock should update on an interval and use stable formatting. It must not use `lastEventAt`.

## Component Boundaries

Implementation should proceed bottom-up. Each component should live in its own file.

Small shared components:

- `ConsoleButton`
- `TopTab`
- `Panel`
- `StatusCell`
- `PlaceholderTab`
- `SceneListRow`
- `ScopeButton`

Composed components:

- `TopTabBar`
- `SceneList`
- `SelectedSceneHeader`
- `ScopeToggleGroup`
- `ChannelScopeGrid`
- `BottomStatusBar`
- `ConsoleLogsTab`

Views:

- `ScenesTab`
- `AppShell`

Non-component helpers should stay in helper files such as existing format utilities when reuse is useful.

## Data Flow

The frontend continues to read state through `useAppState` and execute mutations through `useAppCommands`.

- Selecting a scene calls `selectScene`.
- Storing a scene calls `storeSceneConfig`.
- Duration edits call `setSceneDurationMs`.
- FADER/PAN toggles call `setSceneScopeFadersEnabled` and `setSceneScopePanEnabled`.
- Scope buttons call `setChannelScoped`.
- All/None calls `setAllChannelsScoped`.

The UI should not duplicate backend safety decisions. It should present state and route user intent through existing commands.

## Error Handling And Safety Visibility

- Existing command error handling remains in the provider/app layer.
- Duplicate scene-name warnings stay visible in the Scenes tab.
- Lockout, reconnect, disconnected, and blocked fade states must be visible in the bottom bar.
- Unsupported future actions must not fire placeholder commands.

## Testing

Use the existing frontend test infrastructure.

- Update or add Storybook stories for the new shell, connected Scenes tab, duplicate scene warning, Logs tab, and placeholder tabs.
- Keep visual snapshots representative of the final console look.
- Run `npm run typecheck` and `npm run build` before claiming completion.
- Run targeted visual tests when snapshot changes are intentional and practical.

## Non-Goals

- Implementing Playlists, Events, Sessions, or Settings workflows.
- Changing backend command contracts.
- Adding new scene recall or fade safety behavior.
- Replacing LV1 as the source of truth for scenes.
