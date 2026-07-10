# Nightly Release Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish dated Windows and macOS prereleases from `main` at 6:00 AM Philippine time when the source commit has changed since the previous nightly.

**Architecture:** Extract the existing Windows and macOS packaging jobs into a reusable `workflow_call` workflow. Keep stable release validation and publication in the existing caller, and add a separate scheduled/manual caller that detects source changes, invokes the shared builds, and publishes a dated prerelease with GitHub CLI.

**Tech Stack:** GitHub Actions, GitHub CLI, Bash, PowerShell, Tauri v2 CLI, Rust stable, Node from `.nvmrc`, npm, Prettier, actionlint

## Global Constraints

- Schedule nightlies with `0 22 * * *`, corresponding to 6:00 AM Philippine time.
- Support `workflow_dispatch` through the same nightly execution path.
- Name nightly tags `nightly-YYYY-MM-DD` using `TZ=Asia/Manila`.
- Mark nightly GitHub Releases as prereleases and retain them permanently.
- Skip Windows and macOS builds when the newest nightly tag resolves to the current source commit.
- Never move or replace an existing dated nightly tag that points to a different commit.
- Publish exactly one Windows x64 setup zip and one universal macOS dmg.
- Preserve stable `vN` tag validation, changelog extraction, release notes, asset names, and publication behavior.
- Leave the packaged application's internal `0.1.0` version unchanged.
- Leave artifacts unsigned and macOS artifacts unnotarized.
- No Rust behavior changes are planned, so no Rust tests are added. GitHub workflow validation and live integration runs cover this work.
- Do not stage or modify unrelated worktree changes.

---

### Task 1: Reusable Release Artifact Builds

**Files:**
- Create: `.github/workflows/build-release-artifacts.yml`
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: required reusable-workflow string input `release_id`.
- Produces: workflow artifacts `release-windows` and `release-macos`, each containing one correctly named end-user release asset.
- Preserves: stable caller output `needs.validate-release.outputs.release_tag` and stable `create-release` behavior.

- [ ] **Step 1: Establish the missing-workflow baseline**

Run:

```bash
test -f .github/workflows/build-release-artifacts.yml
```

Expected: exits non-zero because the reusable workflow does not exist yet.

- [ ] **Step 2: Create the reusable build workflow**

Create `.github/workflows/build-release-artifacts.yml` with:

```yaml
name: Build Release Artifacts

on:
  workflow_call:
    inputs:
      release_id:
        description: Identifier included in release asset names
        required: true
        type: string

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

      - name: Stage Windows setup zip
        shell: pwsh
        run: |
          $releaseId = "${{ inputs.release_id }}"
          New-Item -ItemType Directory -Force -Path dist/release/windows | Out-Null
          $setup = Get-ChildItem -Path "target/release/bundle/nsis" -Filter "*setup.exe" | Select-Object -First 1
          if (-not $setup) {
            throw "Could not find NSIS setup exe"
          }
          $stagedExe = "dist/release/windows/Advanced Show Control Setup.exe"
          Copy-Item $setup.FullName $stagedExe
          Compress-Archive -Path $stagedExe -DestinationPath "dist/release/Advanced-Show-Control_${releaseId}_Windows_x64_Setup.zip" -Force

      - name: Upload Windows release artifact
        uses: actions/upload-artifact@v4
        with:
          name: release-windows
          path: dist/release/Advanced-Show-Control_${{ inputs.release_id }}_Windows_x64_Setup.zip
          if-no-files-found: error

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

      - name: Build macOS dmg
        run: npm run tauri -- build --target universal-apple-darwin --bundles dmg

      - name: Stage macOS dmg
        shell: bash
        run: |
          release_id="${{ inputs.release_id }}"
          mkdir -p dist/release
          shopt -s nullglob
          dmgs=(target/universal-apple-darwin/release/bundle/dmg/*.dmg)
          if [[ "${#dmgs[@]}" -ne 1 ]]; then
            echo "Expected exactly one macOS dmg, found ${#dmgs[@]}" >&2
            exit 1
          fi
          cp "${dmgs[0]}" "dist/release/Advanced-Show-Control_${release_id}_macOS_universal.dmg"

      - name: Upload macOS release artifact
        uses: actions/upload-artifact@v4
        with:
          name: release-macos
          path: dist/release/Advanced-Show-Control_${{ inputs.release_id }}_macOS_universal.dmg
          if-no-files-found: error
```

- [ ] **Step 3: Refactor the stable release caller**

Replace `.github/workflows/release.yml` with:

```yaml
name: Release

on:
  push:
    tags:
      - "v[0-9]+"

permissions:
  contents: write

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

  build-release-artifacts:
    name: Build release artifacts
    needs: validate-release
    uses: ./.github/workflows/build-release-artifacts.yml
    with:
      release_id: ${{ needs.validate-release.outputs.release_tag }}

  create-release:
    name: Create GitHub Release
    runs-on: ubuntu-latest
    needs:
      - validate-release
      - build-release-artifacts
    steps:
      - name: Check out repository
        uses: actions/checkout@v4

      - name: Extract release notes
        shell: bash
        run: |
          release_tag="${{ needs.validate-release.outputs.release_tag }}"
          awk -v release_tag="$release_tag" '
            $0 ~ "^## \\[" release_tag "\\] - " { capture=1; next }
            capture && /^## \[/ { exit }
            capture { print }
          ' CHANGELOG.md > release-notes.md
          if [[ ! -s release-notes.md ]]; then
            echo "No release notes found for ${release_tag}" >&2
            exit 1
          fi
          printf '\n---\n\nThese artifacts are unsigned. The macOS artifact is not notarized.\n' >> release-notes.md

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

- [ ] **Step 4: Format and statically validate the shared workflow refactor**

Run:

```bash
npx --yes prettier@3.8.4 --write .github/workflows/build-release-artifacts.yml .github/workflows/release.yml
actionlint .github/workflows/build-release-artifacts.yml .github/workflows/release.yml
git diff --check
```

If `actionlint` is unavailable, install it first with `brew install actionlint`.

Expected: Prettier reports both files, `actionlint` emits no diagnostics, and `git diff --check` emits no output.

- [ ] **Step 5: Review and commit the shared build refactor**

Run:

```bash
git status --short
git diff -- .github/workflows/build-release-artifacts.yml .github/workflows/release.yml
git add .github/workflows/build-release-artifacts.yml .github/workflows/release.yml
git commit -m "ci: share release artifact builds"
```

Expected: the diff contains only the intended shared-build refactor; the commit succeeds without including unrelated worktree files.

---

### Task 2: Scheduled Nightly Prerelease

**Files:**
- Create: `.github/workflows/nightly-release.yml`

**Interfaces:**
- Consumes: reusable workflow input `release_id`, `github.sha`, repository `nightly-*` tags, and `${{ github.token }}`.
- Produces: metadata outputs `should_release`, `release_tag`, and `previous_tag`; a dated tag; and one GitHub prerelease with two assets.

- [ ] **Step 1: Establish the missing-nightly baseline**

Run:

```bash
test -f .github/workflows/nightly-release.yml
```

Expected: exits non-zero because the nightly workflow does not exist yet.

- [ ] **Step 2: Add the scheduled and manually dispatchable nightly workflow**

Create `.github/workflows/nightly-release.yml` with:

```yaml
name: Nightly Release

on:
  schedule:
    - cron: "0 22 * * *"
  workflow_dispatch:

permissions:
  contents: write

concurrency:
  group: nightly-release
  cancel-in-progress: false

jobs:
  metadata:
    name: Resolve nightly metadata
    runs-on: ubuntu-latest
    outputs:
      should_release: ${{ steps.metadata.outputs.should_release }}
      release_tag: ${{ steps.metadata.outputs.release_tag }}
      previous_tag: ${{ steps.metadata.outputs.previous_tag }}
    steps:
      - name: Check out repository with tags
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Resolve nightly metadata
        id: metadata
        shell: bash
        run: |
          release_tag="nightly-$(TZ=Asia/Manila date +%F)"
          current_sha="${GITHUB_SHA}"
          echo "release_tag=${release_tag}" >> "$GITHUB_OUTPUT"

          if existing_sha="$(git rev-parse -q --verify "refs/tags/${release_tag}^{commit}")"; then
            if [[ "$existing_sha" == "$current_sha" ]]; then
              echo "Nightly ${release_tag} already represents ${current_sha}; skipping."
              echo "previous_tag=${release_tag}" >> "$GITHUB_OUTPUT"
              echo "should_release=false" >> "$GITHUB_OUTPUT"
              exit 0
            fi

            echo "Nightly tag ${release_tag} already exists at ${existing_sha}, not ${current_sha}; refusing to move it." >&2
            exit 1
          fi

          mapfile -t nightly_tags < <(git tag --list 'nightly-[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' --sort=-refname)
          previous_tag="${nightly_tags[0]:-}"
          echo "previous_tag=${previous_tag}" >> "$GITHUB_OUTPUT"

          if [[ -n "$previous_tag" ]]; then
            previous_sha="$(git rev-parse "${previous_tag}^{commit}")"
            if [[ "$previous_sha" == "$current_sha" ]]; then
              echo "No changes since ${previous_tag} at ${current_sha}; skipping."
              echo "should_release=false" >> "$GITHUB_OUTPUT"
              exit 0
            fi
          fi

          echo "Publishing ${release_tag} for ${current_sha}."
          echo "should_release=true" >> "$GITHUB_OUTPUT"

  build-release-artifacts:
    name: Build nightly release artifacts
    needs: metadata
    if: needs.metadata.outputs.should_release == 'true'
    uses: ./.github/workflows/build-release-artifacts.yml
    with:
      release_id: ${{ needs.metadata.outputs.release_tag }}

  create-release:
    name: Create nightly prerelease
    runs-on: ubuntu-latest
    needs:
      - metadata
      - build-release-artifacts
    if: needs.metadata.outputs.should_release == 'true'
    steps:
      - name: Check out repository with tags
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Build nightly release notes
        env:
          PREVIOUS_TAG: ${{ needs.metadata.outputs.previous_tag }}
          SOURCE_SHA: ${{ github.sha }}
        shell: bash
        run: |
          {
            printf 'Automated nightly build from `main` at commit `%s`.\n\n' "$SOURCE_SHA"
            if [[ -n "$PREVIOUS_TAG" ]]; then
              printf '## Changes since %s\n\n' "$PREVIOUS_TAG"
              git log --pretty=format:'- %s (`%h`)' "${PREVIOUS_TAG}..${SOURCE_SHA}"
              printf '\n'
            else
              printf 'This is the first nightly release.\n'
            fi
            printf '\n---\n\nThese artifacts are unsigned. The macOS artifact is not notarized.\n'
          } > release-notes.md

      - name: Download staged release artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist/release
          merge-multiple: true

      - name: Create nightly prerelease
        env:
          GH_TOKEN: ${{ github.token }}
          RELEASE_TAG: ${{ needs.metadata.outputs.release_tag }}
          SOURCE_SHA: ${{ github.sha }}
        shell: bash
        run: |
          release_date="${RELEASE_TAG#nightly-}"
          gh release create "$RELEASE_TAG" \
            --target "$SOURCE_SHA" \
            --title "Advanced Show Control Nightly ${release_date}" \
            --notes-file release-notes.md \
            --prerelease \
            "dist/release/Advanced-Show-Control_${RELEASE_TAG}_Windows_x64_Setup.zip" \
            "dist/release/Advanced-Show-Control_${RELEASE_TAG}_macOS_universal.dmg"
```

- [ ] **Step 3: Format and statically validate all release workflows**

Run:

```bash
npx --yes prettier@3.8.4 --write .github/workflows/build-release-artifacts.yml .github/workflows/release.yml .github/workflows/nightly-release.yml
actionlint .github/workflows/*.yml
git diff --check
```

Expected: Prettier reports all three release workflow files, `actionlint` emits no diagnostics for any workflow, and `git diff --check` emits no output.

- [ ] **Step 4: Review the complete workflow dependency change**

Run:

```bash
git status --short
git diff -- .github/workflows/build-release-artifacts.yml .github/workflows/release.yml .github/workflows/nightly-release.yml
```

Expected: the nightly flow is `metadata -> reusable builds -> publication`; the stable flow remains `validation -> reusable builds -> publication`; unrelated worktree changes remain untouched.

- [ ] **Step 5: Commit the nightly workflow**

Run:

```bash
git add .github/workflows/nightly-release.yml
git commit -m "ci: publish nightly prereleases"
```

Expected: the commit contains only `.github/workflows/nightly-release.yml`.

---

### Task 3: Live GitHub CLI Integration Verification

**Files:**
- No repository files are modified.
- Download temporary verification assets under `/var/folders/m0/bgpqyx3n6qqf9fbhcj3rz62m0000gn/T/opencode/nightly-release-verification/`.

**Interfaces:**
- Consumes: committed workflows on GitHub's `main` branch and authenticated `gh` access to `mixxorz/advanced-show-control`.
- Produces: one real dated nightly prerelease and evidence that a second manual run skips platform builds and publication.

- [ ] **Step 1: Confirm only intended commits will be pushed**

Run:

```bash
git status --short
git log --oneline origin/main..main
git diff --stat origin/main..main
```

Expected: the commit range contains the design and plan documentation commits plus the two workflow commits; unrelated unstaged files are not part of the diff.

- [ ] **Step 2: Push the implementation to the default branch**

Run:

```bash
git push origin main
```

Expected: `main` is pushed successfully and `.github/workflows/nightly-release.yml` becomes dispatchable on GitHub.

- [ ] **Step 3: Trigger a real nightly build and capture its run ID**

Run:

```bash
before_run_id="$(gh run list --workflow nightly-release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
gh workflow run nightly-release.yml --ref main
for attempt in {1..30}; do
  run_id="$(gh run list --workflow nightly-release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  if [[ -n "$run_id" && "$run_id" != "$before_run_id" ]]; then
    break
  fi
  sleep 2
done
test -n "$run_id"
test "$run_id" != "$before_run_id"
printf '%s\n' "$run_id"
```

Expected: prints the database ID of the new manually dispatched workflow run.

- [ ] **Step 4: Watch the build and publication to completion**

Run:

```bash
run_id="$(gh run list --workflow nightly-release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
test -n "$run_id"
gh run watch "$run_id" --exit-status
gh run view "$run_id" --json conclusion,jobs --jq '{conclusion, jobs: [.jobs[] | {name, conclusion}]}'
```

Expected: the workflow conclusion is `success`; metadata, Windows build, macOS build, and prerelease publication jobs succeed.

- [ ] **Step 5: Inspect the real prerelease and its assets**

Run:

```bash
release_tag="nightly-$(TZ=Asia/Manila date +%F)"
gh release view "$release_tag" --json tagName,name,isPrerelease,targetCommitish,assets --jq '{tagName, name, isPrerelease, targetCommitish, assets: [.assets[].name]}'
```

Expected:

```json
{
  "tagName": "nightly-YYYY-MM-DD",
  "name": "Advanced Show Control Nightly YYYY-MM-DD",
  "isPrerelease": true,
  "targetCommitish": "<the pushed main commit SHA>",
  "assets": [
    "Advanced-Show-Control_nightly-YYYY-MM-DD_Windows_x64_Setup.zip",
    "Advanced-Show-Control_nightly-YYYY-MM-DD_macOS_universal.dmg"
  ]
}
```

- [ ] **Step 6: Download and inspect both release assets**

Run:

```bash
release_tag="nightly-$(TZ=Asia/Manila date +%F)"
verification_dir="/var/folders/m0/bgpqyx3n6qqf9fbhcj3rz62m0000gn/T/opencode/nightly-release-verification"
mkdir -p "$verification_dir"
gh release download "$release_tag" --dir "$verification_dir" --clobber
windows_zip="$verification_dir/Advanced-Show-Control_${release_tag}_Windows_x64_Setup.zip"
macos_dmg="$verification_dir/Advanced-Show-Control_${release_tag}_macOS_universal.dmg"
test -s "$windows_zip"
test -s "$macos_dmg"
zip_entry_count="$(unzip -Z1 "$windows_zip" | wc -l | tr -d ' ')"
zip_entry="$(unzip -Z1 "$windows_zip")"
test "$zip_entry_count" -eq 1
[[ "$zip_entry" == *.exe ]]
printf 'Windows entry: %s\nmacOS dmg: %s\n' "$zip_entry" "$macos_dmg"
```

Expected: both assets are non-empty, the zip has exactly one entry ending in `.exe`, and the command prints that setup entry and dmg path.

- [ ] **Step 7: Trigger the no-change verification run**

Run:

```bash
before_run_id="$(gh run list --workflow nightly-release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
gh workflow run nightly-release.yml --ref main
for attempt in {1..30}; do
  second_run_id="$(gh run list --workflow nightly-release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  if [[ -n "$second_run_id" && "$second_run_id" != "$before_run_id" ]]; then
    break
  fi
  sleep 2
done
test -n "$second_run_id"
test "$second_run_id" != "$before_run_id"
gh run watch "$second_run_id" --exit-status
gh run view "$second_run_id" --json conclusion,jobs --jq '{conclusion, jobs: [.jobs[] | {name, conclusion}]}'
```

Expected: the workflow succeeds; metadata succeeds; the reusable build call and prerelease publication are skipped, with no Windows or macOS runner builds executed.

- [ ] **Step 8: Confirm publication remained idempotent**

Run:

```bash
release_tag="nightly-$(TZ=Asia/Manila date +%F)"
RELEASE_TAG="$release_tag" gh release list --limit 100 --json tagName --jq '[.[] | select(.tagName == env.RELEASE_TAG)] | length'
gh release view "$release_tag" --json assets --jq '.assets | length'
```

Expected: the first command prints `1` and the second prints `2`.

## Self-Review

- Spec coverage: The plan covers the Philippine schedule, manual dispatch, dated prerelease identity, permanent retention, change detection, collision refusal, concurrency, shared builds, stable release preservation, generated notes, exact assets, static validation, real publication, asset inspection, and no-change verification.
- Placeholder scan: No placeholders, deferred implementation, or unspecified error handling remains.
- Interface consistency: Both callers pass `release_id`; shared artifacts are named `release-windows` and `release-macos`; metadata outputs match all downstream references; stable and nightly asset names match staging, download, publication, and verification commands.
