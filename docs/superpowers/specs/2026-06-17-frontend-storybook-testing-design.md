# Frontend Storybook Testing Design

## Context

The UI package already uses Vite, React, Storybook 10, and `@storybook/addon-vitest`. It has representative stories for the current component set:

- `AppShell`
- `ConnectionScreen`
- `Header`
- `LogsTab`
- `SceneTab`
- `StatusBadge`

There are no existing frontend tests. The goal is to add the first test harness without changing runtime behavior or broadening the UI scope.

## Goals

- Add Vitest for frontend tests.
- Add Storybook-powered Vitest tests for component stories.
- Add local Playwright visual regression tests for Storybook stories.
- Add one basic happy-path test per current component.
- Store visual baseline images with Git LFS.
- Allow Playwright visual tests to run in parallel while keeping CI stable.

## Non-Goals

- Do not redesign components.
- Do not add deep interaction coverage for every edge case.
- Do not introduce hosted visual testing such as Chromatic.
- Do not test Tauri shell commands through the frontend test harness in this pass.

## Chosen Approach

Use Vitest browser tests for Storybook story assertions and Playwright screenshot tests for visual regression.

This keeps behavior and visual concerns separate:

- Storybook `play` assertions verify that each component's normal happy-path story renders expected user-visible content.
- Playwright opens the built Storybook iframe for those same happy-path stories and compares screenshots against committed baselines.

## Storybook `play` Assertions

A Storybook story defines how a component renders with representative props and app state. A `play` function adds a small test that runs after the story renders.

Example:

```tsx
export const Connected: Story = {
  play: async ({ canvas }) => {
    await expect(canvas.getByText("Connected")).toBeInTheDocument();
  },
};
```

The story then acts as both documentation and a test fixture. Storybook's Vitest addon can load the story in a browser test environment, render it, and run the `play` function. If expected text, controls, or status indicators are missing, the test fails.

Initial happy-path assertions should stay simple:

- `AppShell`: renders the normal scene tab shell and global controls.
- `ConnectionScreen`: renders a discovered-system or connection-ready state.
- `Header`: renders connected status and app-level controls.
- `LogsTab`: renders populated log content.
- `SceneTab`: renders the selected stored scene UI.
- `StatusBadge`: renders the passed label.

These tests are intended as first coverage, not exhaustive behavior tests.

## Playwright Visual Tests

Playwright visual tests run separately from Storybook `play` assertions.

The test flow is:

1. Build Storybook as static files.
2. Start a local static server for the built Storybook.
3. Open each target story's isolated iframe URL, such as `/iframe.html?id=components-statusbadge--good`.
4. Wait for the story to finish rendering.
5. Compare a screenshot against the committed baseline image.

Use one Playwright visual test file with a table of happy-path story IDs. Each row becomes one screenshot comparison, so the suite has one baseline image per component story.

The test should use isolated iframe URLs instead of the full Storybook manager UI. This reduces screenshot noise and avoids shared navigation state between stories.

## Parallelization

Playwright visual tests can run in parallel because each story screenshot is independent.

Configure Playwright with:

- `fullyParallel: true`
- A conservative fixed worker count in CI, such as `2`
- Default local worker behavior outside CI

This keeps local runs fast while reducing CI screenshot flake from CPU contention.

## Git LFS

Visual baseline PNGs should be tracked with Git LFS. Add `.gitattributes` entries for Playwright snapshot PNGs after the final snapshot location is known.

The expected pattern is one or both of:

```gitattributes
ui/**/__screenshots__/*.png filter=lfs diff=lfs merge=lfs -text
ui/**/visual.spec.ts-snapshots/*.png filter=lfs diff=lfs merge=lfs -text
```

The implementation should prefer Playwright's default snapshot location unless there is a clear reason to customize it.

Git LFS requires developers and CI to have LFS installed and to fetch LFS objects before running visual comparisons.

## Scripts

Add UI package scripts for the new workflows:

- `test`: run Vitest for normal frontend tests.
- `test:storybook`: run Storybook's Vitest project for story `play` assertions.
- `test:visual`: build or serve Storybook and run Playwright screenshot comparisons.
- `test:visual:update`: update visual baselines intentionally.

The exact command names may use Storybook's generated Vitest config conventions, but the package-level scripts should make the workflows discoverable.

## Test Boundaries

The tests should use existing Storybook fixtures and providers. They should not call live Tauri APIs, connect to LV1, or depend on external network state.

Mock app state remains the source of frontend story data. If a component needs a missing happy-path fixture, add the smallest fixture change needed for that story.

## Verification

Implementation should be verified with the smallest relevant commands first, then the complete frontend checks:

- `npm run typecheck`
- `npm run build`
- `npm run test`
- `npm run test:storybook`
- `npm run test:visual`

Visual baseline creation may require an initial `npm run test:visual:update` before `npm run test:visual` can pass.
