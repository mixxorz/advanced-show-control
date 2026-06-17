# Scene Tab Component Split Design

## Goal

Break `SceneTab` into the smallest logical React components that are useful to understand, story, and visually verify independently. Preserve the existing Scene workflow behavior while aligning component stories with `designs/01 SCENES.png`.

## Scope

This pass is a frontend structure and Storybook pass. It does not change Tauri commands, app state shape, scene recall behavior, fade safety logic, or backend data ownership.

The root `SceneTab` remains the app-state container. Child components become prop-driven and live in their own files with matching Storybook stories.

## Design Target

Stories should use the visual direction from `designs/01 SCENES.png`:

- Dark operator-console surfaces.
- Orange selected, active, and primary action states.
- Dense button grids.
- Compact monospaced numeric labels for scene numbers, crossfade values, and channel labels.
- Clear left scene list, selected scene toolbar, channel scope panel, and status legend shapes.

## Default Storybook Topology

The default connected SceneTab fixture should represent the expected LV1 topology:

- Inputs: channels `1-80`, rendered as one continuous grid.
- Groups: channels `1-16`.
- Auxes: channels `1-24`.
- Masters: `LR`, `C`, `Mono`.
- Matrix: channels `1-8`.
- Link/DCAs: channels `1-16`.

The fixture should include representative stored channel configs across every group so stories exercise grouping, ordering, labels, scoped/unscoped states, fader values, pan-family summaries, and no-pan cases. Hidden/control group `24` is not part of the default visual fixture.

## Component Boundaries

### Shared UI

- `ui/Button.tsx`: generic action button for actions such as Store, All, None, Cue, Recall, Copy, and Paste. Variants cover primary, secondary, ghost, and disabled states.
- `ui/ToggleButton.tsx`: generic pressed/unpressed control used by FADER/PAN toggles and channel scope buttons.

### Scene Tab

- `SceneTab.tsx`: thin container that reads `useAppState`, reads `useAppCommands`, computes the selected scene, keeps local-only `duplicateSceneNames()`, and composes child components.
- `scene-tab/SceneList.tsx`: left panel with title, table headers, duplicate warning, empty state, rows, and legend.
- `scene-tab/SceneListRow.tsx`: one scene row with selected/current/cued visual states, scene number, scene name, and X-fade duration.
- `scene-tab/SceneStatusLegend.tsx`: active, cued next, selected, and selected plus cued legend.
- `scene-tab/SelectedSceneToolbar.tsx`: selected scene strip containing selected scene identity, scope controls, crossfade input, and scene action buttons.
- `scene-tab/CrossfadeInput.tsx`: dedicated X-fade input with draft/commit behavior and visual up/down controls.
- `scene-tab/SceneScopeControls.tsx`: FADER and PAN toggles using the shared `ToggleButton`.
- `scene-tab/ChannelScopePanel.tsx`: channel scope header, All/None actions, and section composition.
- `scene-tab/ChannelScopeSection.tsx`: one channel section such as Inputs, Groups, Auxes, Masters, Matrix, or Link/DCAs.
- `scene-tab/ChannelScopeButton.tsx`: one scoped/unscoped channel button with tooltip detail for channel name, fader dB, and pan-family summary.

Helpers that are only used by one component should stay local to that component. Shared formatting should continue to use existing format utilities when reusable.

## Storybook Requirements

Each component gets its own `.stories.tsx` file. Root `SceneTab.stories.tsx` remains the integration story and uses `MockAppProviders`.

Presentational component stories should render directly with props. Stories should include the states needed to verify the design:

- Selected and unselected scene rows.
- Current active scene and cued next scene row states.
- Duplicate scene warning.
- Empty scene list.
- FADER/PAN enabled and disabled toggles.
- X-fade input normal and immediate values.
- Channel sections for inputs, groups, auxes, masters, matrix, and Link/DCAs.
- Scoped and unscoped channel buttons.
- Full connected SceneTab using the default topology.

## Implementation Plan Constraint

The implementation plan must be one component per task. A task may create the component file and its matching story file, but it must not implement multiple components at once.

Each component task must end with a review and verification loop:

1. Run the smallest relevant frontend verification for the component.
2. Open or capture the Storybook/Playwright visual state for that component.
3. Compare the snapshot against `designs/01 SCENES.png` for the portion of UI represented by the component.
4. Adjust the component until it visually matches the design closely enough for the current scope.
5. Only then move to the next component task.

The final task verifies the composed `SceneTab` story against the full design reference.

## Non-Goals

- No backend or command contract changes.
- No new scene recall actions beyond already wired commands.
- No implementation of unsupported Cue, Recall, Copy, or Paste behavior if those commands do not exist yet.
- No separate file for `duplicateSceneNames()` while it remains local to `SceneTab`.
