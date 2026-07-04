# Windows Build GitHub Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce an unsigned Windows x64 Tauri build artifact through GitHub Actions while trialing only on `windows-build-gh-actions`.

**Architecture:** Add one packaging workflow that runs on `windows-latest`, installs the project toolchains, runs the Tauri build, and uploads Windows bundle artifacts. Enable Tauri bundling in the existing app config so the workflow produces distributable files rather than only a compiled binary.

**Tech Stack:** GitHub Actions, Tauri v2 CLI, Rust stable from `rust-toolchain.toml`, Node from `.nvmrc`, npm workspaces by prefix.

## Global Constraints

- The workflow must automatically run only for pushes to `windows-build-gh-actions` during trial and error.
- The workflow may also support manual `workflow_dispatch` runs.
- The Windows build is unsigned.
- Do not alter app runtime behavior or safety-critical fade logic.
- Rust test style: no Rust behavior changes are planned, so no Rust tests are added.

---

### Task 1: Trial-Only Windows Build Workflow

**Files:**
- Create: `.github/workflows/windows-build.yml`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: root `package-lock.json`, `ui/package-lock.json`, `.nvmrc`, `rust-toolchain.toml`, `src-tauri/tauri.conf.json`.
- Produces: GitHub Actions artifact named `advanced-show-control-windows-x64` containing Windows bundle output from `src-tauri/target/release/bundle`.

- [ ] **Step 1: Enable Tauri bundling**

Update `src-tauri/tauri.conf.json` so `bundle.active` is `true`:

```json
"bundle": {
  "active": true,
  "targets": "all",
  "icon": [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico"
  ]
}
```

- [ ] **Step 2: Add the Windows build workflow**

Create `.github/workflows/windows-build.yml`:

```yaml
name: Windows Build

on:
  push:
    branches:
      - windows-build-gh-actions
  workflow_dispatch:

jobs:
  build-windows:
    name: Build Windows x64
    runs-on: windows-latest

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

      - name: Cache Rust build outputs
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target

      - name: Install root dependencies
        run: npm ci

      - name: Install UI dependencies
        run: npm ci
        working-directory: ui

      - name: Build Windows app
        run: npm run tauri -- build

      - name: Upload Windows artifacts
        uses: actions/upload-artifact@v4
        with:
          name: advanced-show-control-windows-x64
          path: |
            src-tauri/target/release/bundle/**/*
            src-tauri/target/release/advanced-show-control.exe
          if-no-files-found: error
```

- [ ] **Step 3: Verify workflow syntax locally where possible**

Run: `git diff --check`

Expected: no whitespace errors.

- [ ] **Step 4: Commit and push trial branch**

Run:

```bash
git add .github/workflows/windows-build.yml src-tauri/tauri.conf.json docs/superpowers/plans/2026-07-04-windows-build-github-actions.md
git commit -m "ci: add trial windows build workflow"
git push -u origin windows-build-gh-actions
```

Expected: branch is pushed and the `Windows Build` workflow starts for `windows-build-gh-actions` only.

- [ ] **Step 5: Babysit GitHub Actions**

Run: `gh run list --branch windows-build-gh-actions --workflow "Windows Build" --limit 5`

Expected: latest run appears.

If it fails, inspect with:

```bash
gh run view --log-failed
```

Make the smallest corrective commit, push again, and repeat until the workflow uploads `advanced-show-control-windows-x64` or the failure is caused by unavailable GitHub credentials/permissions outside the repository.

## Self-Review

- Spec coverage: The plan covers trial-only trigger scope, unsigned Windows x64 packaging, artifact upload, and babysitting loop.
- Placeholder scan: No placeholders remain.
- Type consistency: No code interfaces are introduced.
