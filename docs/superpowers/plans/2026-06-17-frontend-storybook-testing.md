# Frontend Storybook Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Vitest, Storybook Vitest `play` assertions, and Playwright visual regression tests for the current frontend components.

**Architecture:** Storybook stories remain the shared component fixtures. Vitest runs Storybook `play` assertions for happy-path behavior checks, while Playwright opens built Storybook iframe URLs for screenshot comparisons. Git LFS tracks baseline PNGs.

**Tech Stack:** React 19, Vite 7, Storybook 10, `@storybook/addon-vitest`, Vitest browser mode, Playwright, Git LFS.

## Global Constraints

- Do not redesign components.
- Do not add deep interaction coverage for every edge case.
- Do not introduce hosted visual testing such as Chromatic.
- Do not test Tauri shell commands through the frontend test harness in this pass.
- Tests must use existing Storybook fixtures and providers.
- Tests must not call live Tauri APIs, connect to LV1, or depend on external network state.
- Visual baseline PNGs must be tracked with Git LFS.
- Playwright visual tests must be parallelizable with conservative CI workers.

---

## File Structure

- Modify `ui/package.json`: add test scripts and dev dependencies.
- Modify `ui/package-lock.json`: lock installed test dependencies.
- Modify `ui/vite.config.ts`: add Vitest configuration without changing Vite runtime behavior.
- Create `ui/vitest.setup.ts`: import jest-dom matchers for Storybook/Vitest assertions.
- Create `ui/vitest.workspace.ts`: define separate Vitest projects for normal frontend tests and Storybook story tests.
- Modify `ui/src/components/*.stories.tsx`: add one `play` assertion to one happy-path story per component.
- Create `ui/playwright.config.ts`: configure Storybook visual test server, parallel workers, and screenshot defaults.
- Create `ui/tests/visual/storybook.visual.spec.ts`: one table-driven Playwright visual suite for the six happy-path stories.
- Create `.gitattributes`: track Playwright PNG snapshots with Git LFS.
- Generated `ui/tests/visual/storybook.visual.spec.ts-snapshots/*.png`: baseline images created by Playwright update command.

---

### Task 1: Add Vitest And Storybook Test Configuration

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/package-lock.json`
- Modify: `ui/vite.config.ts`
- Create: `ui/vitest.setup.ts`
- Create: `ui/vitest.workspace.ts`

**Interfaces:**
- Consumes: existing Storybook config in `ui/.storybook/main.ts` and `ui/.storybook/preview.ts`.
- Produces: package scripts `test` and `test:storybook`; shared Vitest setup with jest-dom matchers.

- [ ] **Step 1: Install test dependencies**

Run from `ui/`:

```bash
npm install -D vitest @vitest/browser @vitest/coverage-v8 @testing-library/jest-dom @storybook/test
```

Expected: `package.json` and `package-lock.json` update with the new dev dependencies.

- [ ] **Step 2: Add test scripts**

Edit `ui/package.json` scripts to include these entries while keeping existing scripts:

```json
{
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "build": "vite build",
    "typecheck": "tsc --noEmit",
    "lint": "eslint .",
    "lint:fix": "eslint . --fix",
    "format": "prettier . --write",
    "format:check": "prettier . --check",
    "storybook": "storybook dev -p 6006",
    "build-storybook": "storybook build",
    "test": "vitest --project unit",
    "test:storybook": "vitest --project storybook"
  }
}
```

- [ ] **Step 3: Create Vitest setup file**

Create `ui/vitest.setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 4: Add Vitest config to Vite**

Modify `ui/vite.config.ts` to include test setup while preserving existing Vite settings:

```ts
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  test: {
    globals: true,
    setupFiles: ["./vitest.setup.ts"],
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
  },
});
```

If TypeScript reports that `test` is not a valid Vite config property, import `defineConfig` from `vitest/config` instead:

```ts
import { defineConfig } from "vitest/config";
```

- [ ] **Step 5: Create Vitest workspace**

Create `ui/vitest.workspace.ts`:

```ts
import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
import { defineWorkspace } from "vitest/config";

export default defineWorkspace([
  {
    extends: "./vite.config.ts",
    test: {
      name: "unit",
      include: ["src/**/*.{test,spec}.{ts,tsx}"],
      passWithNoTests: true,
    },
  },
  {
    extends: "./vite.config.ts",
    plugins: [storybookTest({ configDir: ".storybook" })],
    test: {
      name: "storybook",
      browser: {
        enabled: true,
        headless: true,
        provider: "playwright",
        instances: [{ browser: "chromium" }],
      },
      setupFiles: ["./vitest.setup.ts", ".storybook/vitest.setup.ts"],
    },
  },
]);
```

- [ ] **Step 6: Run unit test script to verify empty suite passes**

Run from `ui/`:

```bash
npm run test
```

Expected: PASS or “no test files found” handled by `passWithNoTests`, with exit code `0`.

- [ ] **Step 7: Run Storybook test script to verify config loads**

Run from `ui/`:

```bash
npm run test:storybook
```

Expected: the command discovers Storybook stories. It may fail before Task 2 if stories have no `play` assertions only if the addon treats missing assertions as failures; if so, continue to Task 2 and rerun.

- [ ] **Step 8: Commit Task 1**

```bash
git add ui/package.json ui/package-lock.json ui/vite.config.ts ui/vitest.setup.ts ui/vitest.workspace.ts
git commit -m "test: add frontend vitest setup"
```

---

### Task 2: Add One Storybook Happy-Path Assertion Per Component

**Files:**
- Modify: `ui/src/components/AppShell.stories.tsx`
- Modify: `ui/src/components/ConnectionScreen.stories.tsx`
- Modify: `ui/src/components/Header.stories.tsx`
- Modify: `ui/src/components/LogsTab.stories.tsx`
- Modify: `ui/src/components/SceneTab.stories.tsx`
- Modify: `ui/src/components/StatusBadge.stories.tsx`

**Interfaces:**
- Consumes: `test` and `expect` helpers from `@storybook/test`.
- Produces: one `play` assertion on a happy-path story per current component.

- [ ] **Step 1: Add Storybook test imports to each story file**

Add this import to each listed `.stories.tsx` file:

```ts
import { expect, within } from "@storybook/test";
```

- [ ] **Step 2: Add `StatusBadge` happy-path assertion**

Update `Good` in `ui/src/components/StatusBadge.stories.tsx`:

```ts
export const Good: Story = {
  args: {
    label: "Connected",
    tone: "good",
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText("Connected")).toBeInTheDocument();
  },
};
```

- [ ] **Step 3: Add `Header` happy-path assertion**

Update `Connected` in `ui/src/components/Header.stories.tsx`:

```ts
export const Connected: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText(/connected/i)).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: /connection/i })).toBeInTheDocument();
  },
};
```

If the button accessible name differs, inspect `Header.tsx` and use the exact visible accessible label from the component.

- [ ] **Step 4: Add `ConnectionScreen` happy-path assertion**

Update `SystemsFound` in `ui/src/components/ConnectionScreen.stories.tsx`:

```ts
export const SystemsFound: Story = {
  args: {
    appState: discoveredSystemsAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText(/available lv1 systems/i)).toBeInTheDocument();
  },
};
```

If the heading text differs, inspect `ConnectionScreen.tsx` and use the exact stable user-facing heading.

- [ ] **Step 5: Add `SceneTab` happy-path assertion**

Update `StoredSceneSelected` in `ui/src/components/SceneTab.stories.tsx`:

```ts
export const StoredSceneSelected: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText(/stored scene/i)).toBeInTheDocument();
  },
};
```

If the exact copy differs, inspect `SceneTab.tsx` and assert a stable heading or label that is visible in the selected stored scene state.

- [ ] **Step 6: Add `LogsTab` happy-path assertion**

Update `Populated` in `ui/src/components/LogsTab.stories.tsx`:

```ts
export const Populated: Story = {
  args: {
    appState: connectedAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText(/logs/i)).toBeInTheDocument();
  },
};
```

If there are multiple matches for `logs`, prefer a role query for the tab or heading visible in `LogsTab.tsx`.

- [ ] **Step 7: Add `AppShell` happy-path assertion**

Update `SceneTab` in `ui/src/components/AppShell.stories.tsx`:

```ts
export const SceneTab: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText(/advanced show control/i)).toBeInTheDocument();
    await expect(canvas.getByRole("tab", { name: /scene/i })).toBeInTheDocument();
  },
};
```

If the tab is implemented as a button rather than an ARIA tab, use `getByRole("button", { name: /scene/i })`.

- [ ] **Step 8: Run Storybook tests and tighten selectors**

Run from `ui/`:

```bash
npm run test:storybook
```

Expected: all six happy-path story assertions pass. If a query fails because the exact text or role differs, inspect the component and update the assertion to a stable user-visible element. Do not assert implementation-only class names.

- [ ] **Step 9: Run frontend typecheck**

Run from `ui/`:

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 10: Commit Task 2**

```bash
git add ui/src/components/AppShell.stories.tsx ui/src/components/ConnectionScreen.stories.tsx ui/src/components/Header.stories.tsx ui/src/components/LogsTab.stories.tsx ui/src/components/SceneTab.stories.tsx ui/src/components/StatusBadge.stories.tsx
git commit -m "test: add storybook happy path assertions"
```

---

### Task 3: Add Playwright Visual Regression And Git LFS Baselines

**Files:**
- Modify: `ui/package.json`
- Modify: `ui/package-lock.json`
- Create: `ui/playwright.config.ts`
- Create: `ui/tests/visual/storybook.visual.spec.ts`
- Create: `.gitattributes`
- Create: `ui/tests/visual/storybook.visual.spec.ts-snapshots/*.png`

**Interfaces:**
- Consumes: existing Storybook stories and `build-storybook` script.
- Produces: package scripts `test:visual` and `test:visual:update`; Playwright screenshots for six happy-path story iframe URLs.

- [ ] **Step 1: Install Playwright dependencies**

Run from `ui/`:

```bash
npm install -D @playwright/test
npx playwright install chromium
```

Expected: `package.json` and `package-lock.json` update; Chromium browser is available locally for tests.

- [ ] **Step 2: Add visual test scripts**

Edit `ui/package.json` scripts to add:

```json
{
  "scripts": {
    "test:visual": "playwright test",
    "test:visual:update": "playwright test --update-snapshots"
  }
}
```

- [ ] **Step 3: Create Playwright config**

Create `ui/playwright.config.ts`:

```ts
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/visual",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://127.0.0.1:6007",
    trace: "on-first-retry",
  },
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
    },
  },
  webServer: {
    command: "npm run build-storybook && npx storybook dev -p 6007 --ci --no-open",
    url: "http://127.0.0.1:6007",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 900 } },
    },
  ],
});
```

If `storybook dev` against the build output is not supported by the installed Storybook version, replace the `webServer.command` with a static server package and add that package as a dev dependency:

```bash
npm install -D http-server
```

Then use:

```ts
command: "npm run build-storybook && npx http-server storybook-static -p 6007 -a 127.0.0.1",
```

- [ ] **Step 4: Create the visual story table**

Create `ui/tests/visual/storybook.visual.spec.ts`:

```ts
import { expect, test } from "@playwright/test";

const stories = [
  ["AppShell", "app-appshell--scene-tab"],
  ["ConnectionScreen", "components-connectionscreen--systems-found"],
  ["Header", "components-header--connected"],
  ["LogsTab", "components-logstab--populated"],
  ["SceneTab", "components-scenetab--stored-scene-selected"],
  ["StatusBadge", "components-statusbadge--good"],
] as const;

for (const [componentName, storyId] of stories) {
  test(`${componentName} story matches visual baseline`, async ({ page }) => {
    await page.goto(`/iframe.html?id=${storyId}&viewMode=story`);
    await page.locator("#storybook-root").waitFor({ state: "visible" });

    await expect(page).toHaveScreenshot(`${storyId}.png`, {
      fullPage: true,
    });
  });
}
```

- [ ] **Step 5: Add Git LFS attributes for Playwright snapshots**

Create `.gitattributes` at the repo root:

```gitattributes
ui/tests/visual/**/*.png filter=lfs diff=lfs merge=lfs -text
```

- [ ] **Step 6: Verify Git LFS is available**

Run from the repo root:

```bash
git lfs version
```

Expected: command prints a Git LFS version. If it is not installed, install Git LFS before generating and committing PNG baselines.

- [ ] **Step 7: Generate baseline screenshots**

Run from `ui/`:

```bash
npm run test:visual:update
```

Expected: six PNG baseline files are created under `ui/tests/visual/storybook.visual.spec.ts-snapshots/`.

- [ ] **Step 8: Verify visual test passes against baselines**

Run from `ui/`:

```bash
npm run test:visual
```

Expected: all six visual comparisons pass.

- [ ] **Step 9: Verify LFS tracks PNG baselines**

Run from the repo root:

```bash
git lfs ls-files
```

Expected: the six generated PNG baseline paths appear in the output.

- [ ] **Step 10: Commit Task 3**

```bash
git add .gitattributes ui/package.json ui/package-lock.json ui/playwright.config.ts ui/tests/visual/storybook.visual.spec.ts ui/tests/visual/storybook.visual.spec.ts-snapshots
git commit -m "test: add storybook visual regression"
```

---

### Task 4: Final Verification And Documentation Check

**Files:**
- Modify only if verification reveals formatting, lint, type, or docs issues in files already touched by Tasks 1-3.

**Interfaces:**
- Consumes: all scripts and tests from previous tasks.
- Produces: verified frontend test setup ready for continued UI development.

- [ ] **Step 1: Run frontend format check**

Run from `ui/`:

```bash
npm run format:check
```

Expected: PASS. If formatting fails, run `npm run format`, inspect the diff, and rerun `npm run format:check`.

- [ ] **Step 2: Run frontend lint**

Run from `ui/`:

```bash
npm run lint
```

Expected: PASS.

- [ ] **Step 3: Run frontend typecheck**

Run from `ui/`:

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 4: Run frontend build**

Run from `ui/`:

```bash
npm run build
```

Expected: PASS.

- [ ] **Step 5: Run unit tests**

Run from `ui/`:

```bash
npm run test
```

Expected: PASS.

- [ ] **Step 6: Run Storybook tests**

Run from `ui/`:

```bash
npm run test:storybook
```

Expected: PASS.

- [ ] **Step 7: Run visual tests**

Run from `ui/`:

```bash
npm run test:visual
```

Expected: PASS.

- [ ] **Step 8: Confirm working tree only contains intended changes**

Run from the repo root:

```bash
git status --short
```

Expected: no uncommitted changes, or only intentional changes that were just produced by verification and need to be committed.

- [ ] **Step 9: Commit verification fixes if needed**

If verification required formatting or small fixes, commit only those files:

```bash
git add <fixed-files>
git commit -m "test: stabilize frontend test setup"
```

If no fixes were needed, do not create an empty commit.

---

## Self-Review

- Spec coverage: Tasks cover Vitest setup, Storybook Vitest tests, one happy-path `play` assertion per component, Playwright local visual regression, Git LFS baseline tracking, parallel Playwright configuration, and final verification.
- Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps remain.
- Type consistency: scripts, file paths, story names, and produced interfaces are consistent across tasks.
