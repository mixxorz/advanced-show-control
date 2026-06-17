# GitHub Actions CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub Actions CI that runs Rust, frontend, Storybook, and path-filtered visual regression checks.

**Architecture:** One workflow file owns CI orchestration. Node 24 is defined once in `.nvmrc` and consumed by GitHub Actions. Visual regression tests are separated behind path filtering so backend-only changes do not run Playwright screenshots.

**Tech Stack:** GitHub Actions, Node.js 24, npm, Rust stable from `rust-toolchain.toml`, cargo-nextest, Git LFS, Playwright Chromium.

## Global Constraints

- Add GitHub Actions CI for code checks and tests.
- Define Node.js version 24 for local and CI usage.
- Use `ubuntu-latest` for all jobs.
- Use one workflow at `.github/workflows/ci.yml`.
- Triggers must include `push`, `pull_request`, and `workflow_dispatch`.
- Rust checks must run on every push and pull request.
- Frontend non-visual checks must run on every push and pull request.
- Visual regression tests must run only when UI, Storybook, visual-test, or visual-baseline inputs change, or when manually dispatched.
- Visual baseline PNGs must be available through Git LFS in CI.
- Do not add packaging or release workflows.
- Do not build signed Tauri app bundles.
- Do not add hosted visual testing such as Chromatic.
- Do not run a full operating-system matrix.
- Do not update visual baselines from CI.

---

## File Structure

- Create `.nvmrc`: single source for Node.js version `24`.
- Create `.github/workflows/ci.yml`: GitHub Actions workflow with Rust, frontend, visual-change detection, and visual jobs.

---

### Task 1: Define Node 24

**Files:**
- Create: `.nvmrc`

**Interfaces:**
- Produces: root `.nvmrc` containing `24` for local tools and `actions/setup-node`.

- [ ] **Step 1: Add Node version file**

Create `.nvmrc` at the repository root:

```text
24
```

- [ ] **Step 2: Verify file content**

Run from the repository root:

```bash
node --version
```

Expected: if local Node is already using `.nvmrc`, output starts with `v24`. If local Node is not v24, do not change system Node; document that CI will enforce Node 24 through `actions/setup-node`.

- [ ] **Step 3: Commit Task 1**

```bash
git add .nvmrc
git commit -m "chore: define node version"
```

---

### Task 2: Add GitHub Actions CI Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `.nvmrc` from Task 1.
- Consumes: `rust-toolchain.toml` for Rust stable, rustfmt, and clippy.
- Consumes: root `package-lock.json` and `ui/package-lock.json` for `npm ci`.
- Consumes: existing scripts in `ui/package.json`.
- Produces: CI jobs named `rust`, `frontend`, `visual-changes`, and `visual`.

- [ ] **Step 1: Create workflow directory and file**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  rust:
    name: Rust
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Install cargo-nextest
        uses: taiki-e/install-action@nextest

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Run clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Run tests
        run: cargo nextest run --workspace

      - name: Build workspace
        run: cargo build --workspace

  frontend:
    name: Frontend
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          lfs: true

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: npm
          cache-dependency-path: |
            package-lock.json
            ui/package-lock.json

      - name: Install root dependencies
        run: npm ci

      - name: Install UI dependencies
        working-directory: ui
        run: npm ci

      - name: Check UI formatting
        working-directory: ui
        run: npm run format:check

      - name: Lint UI
        working-directory: ui
        run: npm run lint

      - name: Typecheck UI
        working-directory: ui
        run: npm run typecheck

      - name: Build UI
        working-directory: ui
        run: npm run build

      - name: Run UI unit tests
        working-directory: ui
        run: npm run test

      - name: Run Storybook tests
        working-directory: ui
        run: npm run test:storybook

  visual-changes:
    name: Visual change detection
    runs-on: ubuntu-latest
    outputs:
      visual: ${{ steps.filter.outputs.visual }}
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Detect visual-relevant changes
        id: filter
        uses: dorny/paths-filter@v3
        with:
          filters: |
            visual:
              - '.gitattributes'
              - '.nvmrc'
              - 'ui/.storybook/**'
              - 'ui/package.json'
              - 'ui/package-lock.json'
              - 'ui/playwright.config.ts'
              - 'ui/src/**'
              - 'ui/tests/visual/**'

  visual:
    name: Visual regression
    runs-on: ubuntu-latest
    needs: visual-changes
    if: github.event_name == 'workflow_dispatch' || needs.visual-changes.outputs.visual == 'true'
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          lfs: true

      - name: Pull Git LFS files
        run: git lfs pull

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: npm
          cache-dependency-path: |
            package-lock.json
            ui/package-lock.json

      - name: Install root dependencies
        run: npm ci

      - name: Install UI dependencies
        working-directory: ui
        run: npm ci

      - name: Install Playwright Chromium
        working-directory: ui
        run: npx playwright install --with-deps chromium

      - name: Run visual tests
        working-directory: ui
        run: npm run test:visual

      - name: Upload Playwright artifacts
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-artifacts
          path: |
            ui/test-results/
            ui/playwright-report/
          if-no-files-found: ignore
```

- [ ] **Step 2: Validate workflow syntax locally if available**

Run from the repository root:

```bash
git diff --check -- .github/workflows/ci.yml
```

Expected: no whitespace errors.

If `actionlint` is installed locally, also run:

```bash
actionlint .github/workflows/ci.yml
```

Expected: PASS. If `actionlint` is not installed, document that GitHub will validate the workflow syntax.

- [ ] **Step 3: Run targeted local checks that map to workflow commands**

Run from the repository root:

```bash
cargo fmt --all -- --check
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run format:check
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run test:storybook
```

Expected: PASS with no warnings from the Storybook test setup.

- [ ] **Step 4: Commit Task 2**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add github actions checks"
```

---

### Task 3: Final Verification

**Files:**
- Modify only if verification reveals issues in `.nvmrc` or `.github/workflows/ci.yml`.

**Interfaces:**
- Consumes: `.nvmrc` and `.github/workflows/ci.yml` from previous tasks.
- Produces: verified CI configuration ready for GitHub.

- [ ] **Step 1: Run workflow file checks**

Run from the repository root:

```bash
git diff --check -- .nvmrc .github/workflows/ci.yml
```

Expected: PASS.

If available, run:

```bash
actionlint .github/workflows/ci.yml
```

Expected: PASS. If unavailable, document that it was not run.

- [ ] **Step 2: Run Rust verification commands**

Run from the repository root:

```bash
cargo fmt --all -- --check
```

Expected: PASS.

Run from the repository root:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS.

Run from the repository root:

```bash
cargo nextest run --workspace
```

Expected: PASS.

Run from the repository root:

```bash
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 3: Run frontend verification commands**

Run from the repository root:

```bash
npm --prefix ui run format:check
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run lint
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run typecheck
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run build
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run test
```

Expected: PASS.

Run from the repository root:

```bash
npm --prefix ui run test:storybook
```

Expected: PASS with no warnings from the Storybook test setup.

Run from the repository root:

```bash
npm --prefix ui run test:visual
```

Expected: PASS.

- [ ] **Step 4: Clean generated artifacts**

Run from the repository root:

```bash
rm -rf ui/storybook-static ui/test-results ui/playwright-report
```

Expected: generated Storybook and Playwright artifacts are removed from the working tree.

- [ ] **Step 5: Check final status**

Run from the repository root:

```bash
git status --short
```

Expected: no uncommitted changes, or only intentional fixes that need a commit.

- [ ] **Step 6: Commit verification fixes if needed**

If verification required changes, commit only those files:

```bash
git add .nvmrc .github/workflows/ci.yml
git commit -m "ci: stabilize github actions checks"
```

If no fixes were needed, do not create an empty commit.

---

## Self-Review

- Spec coverage: Tasks define Node 24, create one GitHub Actions workflow, run Rust and frontend checks on every push/PR, path-filter visual tests, support manual visual runs, enable LFS checkout, and avoid baseline updates from CI.
- Placeholder scan: no placeholders or vague implementation steps remain.
- Type consistency: job names, file paths, scripts, and visual path filters are consistent across tasks.
