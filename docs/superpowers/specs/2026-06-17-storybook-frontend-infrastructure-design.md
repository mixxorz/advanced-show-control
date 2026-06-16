# Storybook Frontend Infrastructure Design

## Context

The current frontend is a Tauri-hosted React test bed for exercising connection, scene, show-file, safety, and logging workflows. The roadmap now calls for Storybook before real frontend development so components can be built outside the live Tauri runtime.

This pass is infrastructure-first. It does not redesign the app shell or introduce final UI components.

## Goals

- Add Storybook under `ui/` using the existing React, Vite, TypeScript, and Tailwind stack.
- Make component development possible without a running Tauri shell or LV1 system.
- Preserve the current frontend as a test bed.
- Add representative stories for existing presentational components.
- Provide reusable, typed mock frontend state for future stories.

## Non-Goals

- Do not redesign the UI.
- Do not introduce a new component library or design-system layer.
- Do not add frontend behavior tests in this pass; that is the next roadmap item.
- Do not add stories for the top-level `App`, because it owns Tauri event subscriptions and runtime effects.
- Do not refactor large components unless a very small compile-focused adjustment is required.

## Architecture

Storybook will live entirely in `ui/`:

- `ui/.storybook/main.ts` configures Storybook with the Vite React builder.
- `ui/.storybook/preview.ts` imports `../src/index.css` so stories use the same Tailwind 4 styling as the app.
- `ui/package.json` gets `storybook` and `build-storybook` scripts.
- `ui/src/storybook/mockAppState.ts` exports realistic `AppViewState` fixtures.
- Story files sit next to the existing components or in a nearby story-specific location, following the least disruptive project pattern.

The live Tauri runtime remains separate. Storybook renders components by passing props directly.

## Stories

The initial story coverage should prove the workflow across small and larger existing components:

- `StatusBadge`: neutral, warning, and good tones.
- `Header`: connected, lockout, dirty show file, command error, and fade state examples.
- `ConnectionScreen`: searching, available system, connected system, and command error examples.
- `SceneTab`: no scenes, stored scene with scoped channels, duplicate scene warning, and selected scene examples.
- `LogsTab`: empty logs and populated logs.

Callback props should use no-op handlers or Storybook actions if that fits cleanly with the installed addon set. Stories must not call Tauri commands.

## Data Flow

Mock fixtures mirror the serialized `AppViewState` shape defined in `ui/src/types.ts`. Stories can derive scenario-specific state by shallowly overriding a base fixture. The Rust/Tauri snapshot remains the source of truth for runtime state shape; mocks are only for component development.

## Error Handling

Storybook setup should fail at build time for TypeScript or import problems. Runtime command errors are represented as mocked component props. No network, LV1, or Tauri failure paths are exercised in this pass.

## Verification

Use the smallest verification that proves the setup works:

- `npm run typecheck`
- `npm run build-storybook`

If package installation changes the lockfile, keep only the dependency changes required for Storybook.

## Follow-Up

After this infrastructure lands, the next roadmap item is frontend testing. Future real UI work should use Storybook stories as the development surface before wiring components into the Tauri app.
