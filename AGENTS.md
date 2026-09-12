# AGENTS.md

## Project Context

This project is a native Rust desktop app built with GPUI Kit. It adds timed fader fades to Waves eMotion LV1 and LV1 Classic scene workflows. Supported production targets are macOS 15 or newer and Windows 10 or newer.

Project layout:

- `app/` contains the production `advanced-show-control` crate. Core modules such as `lv1/`, `fade/`, `scenes/`, `cue_lists/`, `show/`, and `runtime/` live under `app/src/`; the GPUI Kit host and views live under `app/src/native_ui/`.
- `dev-tools/` is a separate, non-publishable development crate containing the `advanced-show-control-smoke` hardware-smoke CLI and `lv1-probe` CLI.
- `site/` contains the published user manual; internal architecture and engineering documentation lives in `docs/`.

There is no JavaScript frontend or Tauri host. Do not add npm, React, TypeScript, browser-test, or Tauri dependencies.

LV1 remains the source of truth for scene creation and scene recall. The app is a fader-fade overlay. It stores fade metadata for LV1 scenes and moves only the scoped faders that the engineer has configured.

The app owns:

- Which LV1 scenes have app-managed fade behavior.
- Which faders are scoped into each app-managed scene.
- Stored target fader values for scoped faders.
- Fade duration, fade execution, and safety behavior.
- Show-file storage for the app's fade configuration.

LV1 owns:

- Scene creation and scene recall.
- Normal console state such as routing, plugins, mutes, processing, and non-app-managed scene scope.
- Live mixer state.

Current architecture is actor-oriented:

- `Lv1Actor` owns a generation-scoped LV1 TCP transport, reconnect loop, and mirrored LV1 state.
- `FadeEngine` owns generation-scoped active fade timing and writes directly through its `Lv1ActorHandle` peer.
- `Scenes` is one app-lifetime actor/document that owns scene configs, selection, clipboard, scene reconciliation, and recall policy/queue; it receives generation-scoped LV1/Fade peers from lifecycle.
- Cue Lists is a synchronous domain component owned with the app-lifetime Scenes actor/document; it owns cue documents and reconciles their scene UUID references.
- `Show` owns app-lifetime show-file metadata, dirty state, lockout, connection/discovery metadata, and persistence orchestration; it does not own scene configs or cue documents.
- `Settings` is app-lifetime and persists app settings plus private remembered LV1 identity.
- The projector constructs complete, versioned `AppViewState` snapshots and publishes them through the native projection sink to GPUI.
- `AppLifecycle` owns explicit connection generations and direct peer wiring.
- `AppEventBus` broadcasts facts/events; mailbox commands go directly to their owning actor.

Read these files before substantial work:

- `docs/architecture.md` for runtime architecture.
- `docs/coding-conventions.md` for logging, testing, native UI, verification, and commit conventions.
- `docs/lv1-osc.md` for LV1 protocol details when touching protocol behavior.

Roadmap and actionable work live in GitHub Milestones and Issues:

- Use `gh` to inspect milestones and issues when planning work.
- Read milestone descriptions for release scope, safety notes, and exit criteria.
- Treat issues as the source of truth for actionable tasks.

## Agent Developer Guidance

- Prefer the smallest correct change.
- Use TDD for behavior changes, bug fixes, and safety logic.
- When writing implementation plans, explicitly choose the relevant Rust test style from the allowed categories below.
- Keep code paths explicit and easy to reason about; this project controls live mixer faders.
- Follow existing module patterns unless the task is specifically to clean up structure.
- Do not make broad refactors while implementing a feature unless they are required for the feature and covered by tests.
- Do not add backward-compatibility code unless there is a concrete need.
- Keep docs current when behavior, architecture, or project phase changes.
- File or reference GitHub issues for future ideas instead of adding roadmap items to repository docs.
- For UI work, preserve the existing design language unless the task is to redesign it.
- Define reusable fonts, colors, spacing, borders, and interaction states as GPUI theme tokens. Avoid hard-coded visual values when a reusable token is appropriate.

## Code Contracts

- Before changing or reviewing production code, discover applicable `@cc` comments and ancestor
  `CONTRACTS` files manually or with `cc-check list path/to/file.rs` or
  `cc-check list path/to/file.rs:42`. The command accepts a source file or source location, not a
  directory, and includes ancestor `CONTRACTS` files unless `--no-global` is passed.
- Treat applicable contracts as simultaneous requirements. Keep implementation, behavior tests, and
  contract prose aligned in the same change.
- Surface changes or removals of existing contracts explicitly to their listed owners. Do not weaken
  a contract merely to make an implementation appear compliant.
- Use `cc-check format` to validate syntax and duplicate IDs. The command does not prove semantic
  compliance; verify behavior from implementation, callers, and tests.

## Logging Policy

- Follow `docs/coding-conventions.md` as the source of truth for logging policy.
- Use `tracing` as the application logging API.
- User-facing `INFO`, `WARN`, and `ERROR` messages must be complete enough to show directly in the UI.
- Do not duplicate the same fact at multiple layers.

## Rust Test Policy

Rust tests should fit one of these categories:

- Pure unit tests that call functions directly and have no side effects.
- Actor tests that interact through the actor mailbox, `AppEventBus`, and a tracing listener when tracing output is part of the behavior under test.
- Smoke tests through the separate `dev-tools/` smoke CLI.

Do not test side-effecting actor behavior by directly mutating actor internals or inspecting private state. When writing implementation plans, specify which of these categories covers each Rust behavior being added or changed.

## Safety-Critical Rules

- Do not bypass lockout checks.
- Do not bypass exact scene identity validation unless the task explicitly changes the matching model.
- Do not bypass generation guards. Stale tasks must not send fader commands or write misleading UI logs after disconnect or reconnect.
- Do not send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Scene recall automation must validate before aborting an existing fade.
- Blocked, skipped, or disabled recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Make safety blocks visible through logs or UI state.
- Preserve manual override, abort, overlap/same-scene, and disconnect safety behavior.

## Verification Commands

Use the smallest relevant `make` target while developing, then run broader verification before completion. The root `Makefile` is a thin command index over the native Cargo workflows below.

Common root targets:

```bash
make help
make fmt
make lint
make test
make build
make check
```

`make check` runs the standard CI-style formatting, linting, tests, and builds for the production and development-tool crates. It does not run native visual checks, packaging, or hardware smoke.

Native development and packaging targets:

```bash
make dev
make visual-test
make package-macos RELEASE_ID="local"
make package-windows RELEASE_ID="local"
make probe ARGS="..."
```

`make dev` runs the GPUI application. `make visual-test` runs the GPUI native visual/component tests on macOS. The package targets create distributable archives under `dist/release/`; macOS packaging requires macOS and builds an ad-hoc-signed universal `.app` without Developer ID signing or notarization, while Windows packaging creates an unsigned archive and requires PowerShell and the MSVC x64 target. `make probe` runs the LV1 probe CLI and forwards optional `ARGS`.

Debug smoke target:

```bash
make smoke
make smoke VERBOSE=1
```

`make smoke` runs the non-GUI Rust hardware-smoke CLI from `dev-tools/` and requires an LV1-compatible target environment. Use `VERBOSE=1` to stream terminal logs.

After running `make smoke`, always inspect `logs/debug-smoke-report.txt` for the authoritative suite result. The terminal output can be noisy or truncated; do not claim the smoke passed just because the shell command returned or no failure marker appeared in captured output.

Runtime diagnostic logs are JSONL files written under the native app-data directory, not the repo `logs/` folder. On macOS, check:

```bash
~/Library/Application Support/com.advancedshowcontrol.app/logs/diagnostics-*.jsonl
~/Library/Application Support/com.advancedshowcontrol.debug/logs/diagnostics-*.jsonl
```

The production app retains the `com.advancedshowcontrol.app` data location for compatibility. The smoke CLI uses the isolated `com.advancedshowcontrol.debug` data location and writes its authoritative repository report to `logs/debug-smoke-report.txt`.

Common Rust checks:

```bash
make rust-fmt
make rust-lint
make rust-test
make rust-build

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
```

Use `cargo nextest run ...` for Rust tests, including targeted inner-loop checks. Avoid `cargo test` unless you specifically need a test harness feature that nextest cannot provide.

Native UI tests use `#[gpui_kit::test]` and run with the Rust suite. On macOS, `make visual-test` selects the native visual/component tests. Windows appearance requires manual visual acceptance where equivalent image rendering is unavailable.

CI runs the checks covered by `make check` on native targets and may run platform packaging separately.

Hook-only targeted Rust formatting and Clippy checks run for staged production and development-tool Rust files at commit time. Do not run these manually; let the hooks run them at commit time.

Do not bypass hooks; fix failures in a new commit.

Useful targeted Rust checks:

```bash
cargo nextest run -p advanced-show-control scene_recall
cargo nextest run -p advanced-show-control commands::tests
cargo nextest run -p advanced-show-control fade
```

Before claiming work is complete, run the verification command that proves the claim and read the output.

## Commit Rules

- Commit early and often in this repo. You do not need to ask for approval before making commits unless the user explicitly asks you not to commit.
- Check `git status --short` before committing.
- Inspect the relevant `git diff` before committing.
- Stage only intended files.
- Do not include unrelated user or agent changes.
- Use concise commit messages that match the repo style, such as `feat: ...`, `fix: ...`, `test: ...`, or `docs: ...`.
- Run relevant verification before committing code changes.
- Do not amend commits unless explicitly asked.
- Do not force-push unless explicitly asked.
- Do not use destructive git commands such as `git reset --hard` or `git checkout -- <path>` unless explicitly asked.
- If tests or hooks fail, fix the issue in a new commit rather than hiding or bypassing the failure.
