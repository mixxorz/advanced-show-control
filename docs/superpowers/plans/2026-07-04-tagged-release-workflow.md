# Tagged Release Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build unsigned Windows and universal macOS release bundles from simple build-number tags and publish a GitHub Release with exactly one Windows zip and one macOS dmg.

**Architecture:** Replace trial platform workflows with one tag-only release workflow. The workflow validates a matching changelog section, builds platform artifacts in parallel, stages only the end-user files, and creates a GitHub Release using the changelog section as the body.

**Tech Stack:** GitHub Actions, GitHub CLI, Tauri v2 CLI, Rust stable from `rust-toolchain.toml`, Node from `.nvmrc`, npm by prefix, PowerShell zip packaging, shell changelog extraction.

## Global Constraints

- Releases trigger only from version tags matching `v[0-9]+`.
- No `workflow_dispatch` release trigger.
- Release notes come from a matching `CHANGELOG.md` section.
- GitHub Release attaches exactly two files: a macOS universal `.dmg` and a Windows x64 setup `.zip` containing only the NSIS setup `.exe`.
- Do not attach MSI, raw `.app`, raw Windows app `.exe`, bundle directories, debug binaries, or probe binaries.
- Artifacts are unsigned and macOS artifacts are not notarized.
- Do not alter app runtime behavior or safety-critical fade logic.
- Rust test style: no Rust behavior changes are planned, so no Rust tests are added.

---

### Task 1: Changelog and Release Workflow

**Files:**
- Create: `CHANGELOG.md`
- Create: `.github/workflows/release.yml`
- Delete: `.github/workflows/windows-build.yml`
- Delete: `.github/workflows/macos-build.yml`
- Modify: `docs/superpowers/plans/2026-07-04-tagged-release-workflow.md`

**Interfaces:**
- Consumes: version tag `vN`, `CHANGELOG.md` section `## [vN] - YYYY-MM-DD`, root `package-lock.json`, `ui/package-lock.json`, `.nvmrc`, `rust-toolchain.toml`, `src-tauri/tauri.conf.json`.
- Produces: GitHub Release named `vN` with release body from the changelog section.
- Produces: Release asset `Advanced-Show-Control_vN_Windows_x64_Setup.zip` containing exactly one NSIS setup `.exe`.
- Produces: Release asset `Advanced-Show-Control_vN_macOS_universal.dmg`.

- [ ] **Step 1: Add `CHANGELOG.md`**

Create `CHANGELOG.md`:

```markdown
# Changelog

All notable changes to Advanced Show Control will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [v1] - 2026-07-04

### Added

- Added unsigned Windows x64 and universal macOS release packaging.
- Added tag-driven GitHub Release publishing.

### Notes

- Release artifacts are unsigned.
- macOS artifacts are not notarized and may require explicit user approval to open.
```

- [ ] **Step 2: Replace trial workflows with release workflow**

Delete `.github/workflows/windows-build.yml` and `.github/workflows/macos-build.yml`.

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - "v[0-9]+"

permissions:
  contents: write

env:
  APP_NAME: Advanced Show Control

jobs:
  validate-release:
    name: Validate release metadata
    runs-on: ubuntu-latest
    outputs:
      release_tag: ${{ steps.version.outputs.release_tag }}
    steps:
      - name: Check out repository
        uses: actions/checkout@v4

      - name: Validate tag and changelog
        id: version
        shell: bash
        run: |
          tag="${GITHUB_REF_NAME}"
          if [[ ! "$tag" =~ ^v[0-9]+$ ]]; then
            echo "Tag '$tag' is not a build-number tag like v1" >&2
            exit 1
          fi
          if ! grep -Eq "^## \\[${tag}\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" CHANGELOG.md; then
            echo "CHANGELOG.md is missing a section for ${tag}" >&2
            exit 1
          fi
          echo "release_tag=${tag}" >> "$GITHUB_OUTPUT"

  build-windows:
    name: Build Windows x64
    runs-on: windows-latest
    needs: validate-release
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

      - name: Stage Windows setup zip
        shell: pwsh
        run: |
          $releaseTag = "${{ needs.validate-release.outputs.release_tag }}"
          New-Item -ItemType Directory -Force -Path dist/release/windows | Out-Null
          $setup = Get-ChildItem -Path "target/release/bundle/nsis" -Filter "*setup.exe" | Select-Object -First 1
          if (-not $setup) {
            throw "Could not find NSIS setup exe"
          }
          $stagedExe = "dist/release/windows/Advanced Show Control Setup.exe"
          Copy-Item $setup.FullName $stagedExe
          Compress-Archive -Path $stagedExe -DestinationPath "dist/release/Advanced-Show-Control_${releaseTag}_Windows_x64_Setup.zip" -Force

      - name: Upload Windows release artifact
        uses: actions/upload-artifact@v4
        with:
          name: release-windows
          path: dist/release/Advanced-Show-Control_${{ needs.validate-release.outputs.release_tag }}_Windows_x64_Setup.zip
          if-no-files-found: error

  build-macos:
    name: Build macOS universal
    runs-on: macos-latest
    needs: validate-release
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

      - name: Build macOS dmg
        run: npm run tauri -- build --target universal-apple-darwin --bundles dmg

      - name: Stage macOS dmg
        shell: bash
        run: |
          release_tag="${{ needs.validate-release.outputs.release_tag }}"
          mkdir -p dist/release
          dmg="$(find target/universal-apple-darwin/release/bundle/dmg -maxdepth 1 -type f -name '*.dmg' | head -n 1)"
          if [[ -z "$dmg" ]]; then
            echo "Could not find macOS dmg" >&2
            exit 1
          fi
          cp "$dmg" "dist/release/Advanced-Show-Control_${release_tag}_macOS_universal.dmg"

      - name: Upload macOS release artifact
        uses: actions/upload-artifact@v4
        with:
          name: release-macos
          path: dist/release/Advanced-Show-Control_${{ needs.validate-release.outputs.release_tag }}_macOS_universal.dmg
          if-no-files-found: error

  create-release:
    name: Create GitHub Release
    runs-on: ubuntu-latest
    needs:
      - validate-release
      - build-windows
      - build-macos
    steps:
      - name: Check out repository
        uses: actions/checkout@v4

      - name: Extract release notes
        shell: bash
        run: |
          release_tag="${{ needs.validate-release.outputs.release_tag }}"
          awk -v release_tag="$release_tag" '
            $0 ~ "^## \\[" release_tag "\\] - " { capture=1; next }
            capture && /^## \\[/ { exit }
            capture { print }
          ' CHANGELOG.md > release-notes.md
          if [[ ! -s release-notes.md ]]; then
            echo "No release notes found for ${release_tag}" >&2
            exit 1
          fi
          cat >> release-notes.md <<'NOTES'

---

These artifacts are unsigned. The macOS artifact is not notarized.
NOTES

      - name: Download staged release artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist/release
          merge-multiple: true

      - name: Create release
        env:
          GH_TOKEN: ${{ github.token }}
        shell: bash
        run: |
          gh release create "${GITHUB_REF_NAME}" \
            --title "Advanced Show Control ${GITHUB_REF_NAME}" \
            --notes-file release-notes.md \
            dist/release/Advanced-Show-Control_${{ needs.validate-release.outputs.release_tag }}_Windows_x64_Setup.zip \
            dist/release/Advanced-Show-Control_${{ needs.validate-release.outputs.release_tag }}_macOS_universal.dmg
```

- [ ] **Step 3: Verify locally where possible**

Run: `git diff --check`

Expected: no whitespace errors.

Run: `npm run tauri -- build --target universal-apple-darwin --bundles dmg`

Expected: succeeds and produces a `.dmg` under `target/universal-apple-darwin/release/bundle/dmg/`.

Run: `cargo run --manifest-path src-tauri/dev-tools/Cargo.toml --bin lv1-probe -- --help`

Expected: prints probe help.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add CHANGELOG.md .github/workflows/release.yml .github/workflows/windows-build.yml .github/workflows/macos-build.yml docs/superpowers/specs/2026-07-04-build-number-release-versioning-design.md docs/superpowers/plans/2026-07-04-tagged-release-workflow.md
git commit -m "ci: add tag-driven release workflow"
git push
```

Expected: branch is pushed. No release workflow runs until a matching version tag is pushed.

## Self-Review

- Spec coverage: The plan covers tag-only releases, changelog validation, Windows and macOS builds, exact release asset filtering, GitHub Release creation, and unsigned artifact notes.
- Placeholder scan: No placeholders remain.
- Type consistency: Artifact names are consistent across staging, artifact upload, artifact download, and release creation.
