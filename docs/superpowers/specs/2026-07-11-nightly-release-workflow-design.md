# Nightly Release Workflow Design

## Goal

Publish an automated, dated prerelease from `main` at 6:00 AM Philippine time when `main` has changed since the previous nightly build.

The nightly release must contain the same unsigned Windows x64 setup zip and universal macOS dmg as a stable release. Existing tag-driven `vN` stable releases must retain their current validation, changelog, naming, and publication behavior.

## Triggers And Schedule

The nightly workflow supports:

- A scheduled trigger using `0 22 * * *`, which corresponds to 6:00 AM in the Philippines (UTC+8) on the following calendar day.
- A `workflow_dispatch` trigger for end-to-end verification and manual recovery.

GitHub Actions schedules are best-effort and may begin later than the configured minute. Both triggers execute the same metadata, change detection, build, and publication path.

## Release Identity

The workflow calculates the release date with `TZ=Asia/Manila` and uses it consistently:

- Tag: `nightly-YYYY-MM-DD`
- Release title: `Advanced Show Control Nightly YYYY-MM-DD`
- Windows asset: `Advanced-Show-Control_nightly-YYYY-MM-DD_Windows_x64_Setup.zip`
- macOS asset: `Advanced-Show-Control_nightly-YYYY-MM-DD_macOS_universal.dmg`

Nightly releases are marked as GitHub prereleases so they do not replace the latest stable release. They are retained permanently.

The packaged application's internal version remains unchanged. Nightly identity exists only in the Git tag, GitHub Release, release title, notes, and asset names.

## Shared Build Workflow

Add `.github/workflows/build-release-artifacts.yml` as a reusable workflow invoked through `workflow_call`. It accepts a release identifier and owns the two platform build jobs.

The Windows job preserves the existing release process:

- Build the Tauri app on the Windows x64 runner.
- Locate the NSIS setup executable.
- Stage a zip containing exactly that one setup executable.
- Upload the named Windows release asset as a workflow artifact.

The macOS job preserves the existing release process:

- Build the universal Apple target on the macOS runner.
- Locate the generated dmg.
- Stage the dmg with the requested release identifier.
- Upload the named macOS release asset as a workflow artifact.

The reusable workflow does not create tags or GitHub Releases. Publication policy remains with each caller.

Refactor `.github/workflows/release.yml` to replace its duplicated Windows and macOS build jobs with a call to the reusable workflow. Its `vN` tag validation, changelog extraction, stable release title, notes, and publication remain unchanged.

## Nightly Metadata And Change Detection

Add `.github/workflows/nightly-release.yml`. Its metadata job checks out the scheduled `main` commit with complete tag history and:

1. Calculates the Philippine release date and nightly tag.
2. Checks whether the intended dated tag already exists.
3. Finds the newest existing `nightly-*` tag by its date-based name.
4. Resolves that tag to its commit.
5. Compares the prior nightly commit with the current workflow commit.

If both commits match, the workflow succeeds without invoking the shared Windows or macOS builds and without creating another release. If no prior nightly tag exists or the commits differ, the build and publication jobs run.

If the intended dated tag already exists at a different commit, metadata validation fails with a clear collision error rather than moving or replacing a published tag. If it exists at the current commit, the workflow follows the successful no-change path.

A workflow-level concurrency group prevents scheduled and manually dispatched nightly runs from publishing concurrently. Runs are not cancelled after starting.

## Publication And Release Notes

The publication job runs only after both platform builds succeed. It downloads exactly the two staged assets and creates `nightly-YYYY-MM-DD` at the workflow commit using the GitHub CLI and the workflow token.

The tag and prerelease are created only during publication. A failed metadata or build job therefore leaves no tag or partial GitHub Release.

Release notes include:

- The exact source commit.
- Commits added since the previous nightly, when one exists.
- An initial-nightly message when no previous nightly exists.
- The existing warning that artifacts are unsigned and the macOS artifact is not notarized.

A rerun at a commit already represented by the newest nightly tag follows the no-change path. This makes manual retries idempotent after successful publication.

## Permissions And Safety

The nightly caller receives `contents: write` so the GitHub CLI can create a tag and prerelease. The shared build workflow does not require secrets or application runtime credentials.

This work changes build and release automation only. It does not alter application runtime behavior, LV1 communication, scene recall, fade execution, or safety logic.

## Verification

Before publishing the workflow changes:

- Run Prettier against the changed YAML files.
- Run `actionlint` against all GitHub Actions workflows.
- Run `git diff --check`.
- Review the workflow job dependencies and the complete YAML diff.

After the nightly workflow is available on the GitHub default branch, perform a live integration verification with the GitHub CLI:

1. Trigger `nightly-release.yml` with `gh workflow run`.
2. Watch the resulting run with `gh run watch --exit-status` and require success.
3. Inspect the resulting prerelease with `gh release view` and confirm its tag, prerelease status, source commit, title, and two expected asset names.
4. Download both assets with `gh release download`.
5. Confirm the Windows zip contains exactly one setup executable and confirm the macOS dmg is present and non-empty.
6. Trigger the workflow again at the same commit.
7. Confirm the second run succeeds through the no-change path without platform builds and without creating another release.

The live verification intentionally creates one real nightly prerelease. It exercises the scheduled path's shared logic, GitHub-hosted platform runners, workflow artifact transfer, token permissions, GitHub CLI publication, and no-change idempotency.

## Out Of Scope

- Signing or notarizing release artifacts.
- Automatically deleting old nightly releases.
- Changing the stable `vN` release numbering model.
- Updating the application's internal version for nightly builds.
- Guaranteeing exact start time beyond GitHub Actions scheduling semantics.
