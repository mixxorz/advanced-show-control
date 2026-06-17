# GitHub Actions CI Design

## Context

The repository currently has no GitHub Actions workflows. The project has three verification areas:

- Rust workspace checks for the core crate and Tauri shell crate.
- Frontend TypeScript, lint, build, and Vitest/Storybook checks in `ui/`.
- Playwright visual regression tests against built Storybook, with PNG baselines stored through Git LFS.

The CI setup should make the same checks easy to run in GitHub without expanding the product scope.

## Goals

- Add GitHub Actions CI for code checks and tests.
- Define Node.js version 24 for local and CI usage.
- Run Rust checks on every push and pull request.
- Run frontend non-visual checks on every push and pull request.
- Run visual regression tests only when UI, Storybook, visual-test, or visual-baseline inputs change.
- Allow visual regression tests to be run manually.
- Ensure Git LFS visual baselines are available in CI.

## Non-Goals

- Do not add packaging or release workflows.
- Do not build signed Tauri app bundles.
- Do not add hosted visual testing such as Chromatic.
- Do not run a full operating-system matrix.
- Do not update visual baselines from CI.

## Node Version

Add a root `.nvmrc` containing:

```text
24
```

GitHub Actions should use `actions/setup-node` with `node-version-file: .nvmrc`. This keeps local development and CI aligned.

## Runner Target

Use `ubuntu-latest` for all jobs.

This is sufficient for code checks and tests. macOS-specific desktop packaging is deferred until bundling work.

## Workflow Shape

Create one workflow at `.github/workflows/ci.yml`.

Triggers:

- `push`
- `pull_request`
- `workflow_dispatch`

Jobs:

- `rust`: always runs.
- `frontend`: always runs.
- `visual-changes`: detects whether visual tests are relevant for this change.
- `visual`: runs when `visual-changes` reports relevant changes, or when triggered manually.

## Rust Job

The Rust job should:

1. Check out the repository.
2. Install Rust using `rust-toolchain.toml`.
3. Cache Cargo registry, git dependencies, and build outputs.
4. Install `cargo-nextest`.
5. Run:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo nextest run --workspace`
   - `cargo build --workspace`

These commands match the project verification guidance.

## Frontend Job

The frontend job should:

1. Check out the repository with Git LFS enabled.
2. Set up Node 24 from `.nvmrc`.
3. Run `npm ci` at the repository root for root-level Node tooling.
4. Run `npm ci` in `ui/`.
5. Run:
   - `npm run format:check`
   - `npm run lint`
   - `npm run typecheck`
   - `npm run build`
   - `npm run test`
   - `npm run test:storybook`

The frontend job should not run Playwright visual screenshots by default.

## Visual Change Detection

The visual tests should run only when relevant files change.

Relevant paths:

- `.gitattributes`
- `.nvmrc`
- `ui/.storybook/**`
- `ui/package.json`
- `ui/package-lock.json`
- `ui/playwright.config.ts`
- `ui/src/**`
- `ui/tests/visual/**`

Use a path-filter action or equivalent shell-based diff check to expose an output consumed by the `visual` job.

## Visual Job

The visual job should:

1. Check out the repository with Git LFS enabled.
2. Run `git lfs pull` so baseline images are available.
3. Set up Node 24 from `.nvmrc`.
4. Run `npm ci` at the repository root.
5. Run `npm ci` in `ui/`.
6. Install Playwright Chromium with dependencies:
   - `npx playwright install --with-deps chromium`
7. Run:
   - `npm run test:visual`
8. Upload Playwright artifacts on failure only.

The visual job should also run when the workflow is manually dispatched, even if no relevant paths changed.

## LFS Requirements

Actions checkout should use `lfs: true` where frontend or visual baselines may be read. The visual job should also run `git lfs pull` explicitly before Playwright tests.

The workflow should never update or commit visual baselines. Baseline updates remain a local developer action using `npm run test:visual:update`.

## Caching

Use conservative caching for speed:

- `actions/setup-node` npm cache for root `package-lock.json` and `ui/package-lock.json`.
- Rust/Cargo cache for registry, git dependencies, and build outputs.

Cache misses must not break CI.

## Verification

Local verification before committing should include:

- `npm --prefix ui run format:check`
- `npm --prefix ui run lint`
- `npm --prefix ui run typecheck`
- `npm --prefix ui run build`
- `npm --prefix ui run test`
- `npm --prefix ui run test:storybook`
- `npm --prefix ui run test:visual`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --workspace`
- `cargo build --workspace`

If local environment limitations prevent a command from running, document the limitation and rely on the matching GitHub Actions command for CI validation.
