# Universal macOS Build GitHub Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce an unsigned universal macOS Tauri build artifact through GitHub Actions while trialing only on `windows-build-gh-actions`.

**Architecture:** Add one macOS packaging workflow that runs on `macos-latest`, installs both Apple Silicon and Intel Rust targets, runs a universal Tauri build, and uploads macOS bundle artifacts from root `target/universal-apple-darwin/release`. Reuse the existing enabled Tauri bundling config.

**Tech Stack:** GitHub Actions, Tauri v2 CLI, Rust stable from `rust-toolchain.toml`, Node from `.nvmrc`, npm workspaces by prefix.

## Global Constraints

- The workflow must automatically run only for pushes to `windows-build-gh-actions` during trial and error.
- The workflow may also support manual `workflow_dispatch` runs.
- The macOS build is unsigned and not notarized.
- The macOS build should be universal for Apple Silicon and Intel Macs.
- Do not alter app runtime behavior or safety-critical fade logic.
- Rust test style: no Rust behavior changes are planned, so no Rust tests are added.

---

### Task 1: Trial-Only Universal macOS Build Workflow

**Files:**
- Create: `.github/workflows/macos-build.yml`
- Create: `docs/superpowers/plans/2026-07-04-universal-macos-build-github-actions.md`
- Modify: `src-tauri/Cargo.toml`
- Modify: `Makefile`

**Interfaces:**
- Consumes: root `package-lock.json`, `ui/package-lock.json`, `.nvmrc`, `rust-toolchain.toml`, `src-tauri/tauri.conf.json`.
- Produces: GitHub Actions artifact named `advanced-show-control-macos-universal` containing macOS bundle output from `target/universal-apple-darwin/release/bundle` and the app binary from `target/universal-apple-darwin/release/advanced-show-control`.
- Preserves: `make probe` and `make smoke` by opting them into the `dev-tools` Cargo feature.

- [ ] **Step 1: Add the macOS build workflow**

Create `.github/workflows/macos-build.yml`:

```yaml
name: macOS Build

on:
  push:
    branches:
      - windows-build-gh-actions
  workflow_dispatch:

jobs:
  build-macos:
    name: Build macOS universal
    runs-on: macos-latest

    steps:
      - name: Check out repository
        uses: actions/checkout@v4

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: npm
          cache-dependency-path: |
            package-lock.json
            ui/package-lock.json

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin

      - name: Cache Rust build outputs
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target

      - name: Install root dependencies
        run: npm ci

      - name: Install UI dependencies
        run: npm ci
        working-directory: ui

      - name: Build macOS universal app
        run: npm run tauri -- build --target universal-apple-darwin

      - name: Upload macOS artifacts
        uses: actions/upload-artifact@v4
        with:
          name: advanced-show-control-macos-universal
          path: |
            target/universal-apple-darwin/release/bundle/**/*
            target/universal-apple-darwin/release/advanced-show-control
          if-no-files-found: error
```

- [ ] **Step 2: Verify workflow syntax locally where possible**

Run: `git diff --check`

Expected: no whitespace errors.

- [ ] **Step 3: Commit and push trial branch**

Run:

```bash
git add .github/workflows/macos-build.yml docs/superpowers/plans/2026-07-04-universal-macos-build-github-actions.md
git commit -m "ci: add trial macos build workflow"
git push
```

Expected: branch is pushed and the `macOS Build` workflow starts for `windows-build-gh-actions` only.

- [ ] **Step 4: Babysit GitHub Actions**

Run: `gh run list --branch windows-build-gh-actions --workflow "macOS Build" --limit 5`

Expected: latest run appears.

If it fails, inspect with:

```bash
gh run view --log-failed
```

Make the smallest corrective commit, push again, and repeat until the workflow uploads `advanced-show-control-macos-universal` or the failure is caused by unavailable GitHub credentials/permissions outside the repository.

## Self-Review

- Spec coverage: The plan covers trial-only trigger scope, unsigned universal macOS packaging, artifact upload, and babysitting loop.
- Placeholder scan: No placeholders remain.
- Type consistency: No code interfaces are introduced.
