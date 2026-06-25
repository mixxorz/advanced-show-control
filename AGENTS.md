# AGENTS.md

## Project Context

This project is a Tauri/Rust/React desktop app that adds timed fader fades to Waves eMotion LV1 and LV1 Classic scene workflows.

Project layout:

- `src-tauri/` contains the single Rust/Tauri crate, `advanced-show-control`. Core Rust modules such as `lv1/`, `fade/`, `scene_recall/`, `show/`, and `runtime/` live under `src-tauri/src/` alongside Tauri adapter modules.
- `src-tauri/src/bin/lv1-probe.rs` contains the preserved LV1 probe/developer CLI binary.
- `ui/` contains the React/TypeScript frontend.

Do not assume `src/` is the frontend; this project does not use the default Tauri template layout.

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

- `Lv1Actor` owns the LV1 TCP connection and mirrored LV1 state.
- `FadeEngine` owns active fade timing and fader writes.
- `ShowState` owns show data and show-file state.
- `SceneRecallFader` owns scene recall policy and starts validated scene fades.
- The projector owns the Tauri-side `AppViewState` projection and emits `app-status-changed`.
- `AppLifecycle` is the Tauri-side runtime lifecycle seam and app-lifetime command-bus holder.
- `AppEventBus` broadcasts facts/events.
- `AppCommandBus` routes acknowledged commands to the current LV1 and fade targets.

Read these files before substantial work:

- `docs/roadmap.md` for product intent, safety model, current MVP roadmap, and deferred work.
- `docs/architecture.md` for runtime architecture.
- `docs/coding-conventions.md` for logging, testing, frontend, verification, and commit conventions.
- `docs/lv1-osc.md` for LV1 protocol details when touching protocol behavior.

## Agent Developer Guidance

- Prefer the smallest correct change.
- Use TDD for behavior changes, bug fixes, and safety logic.
- When writing implementation plans, explicitly choose the relevant Rust test style from the allowed categories below.
- Keep code paths explicit and easy to reason about; this project controls live mixer faders.
- Follow existing module patterns unless the task is specifically to clean up structure.
- Do not make broad refactors while implementing a feature unless they are required for the feature and covered by tests.
- Do not add backward-compatibility code unless there is a concrete need.
- Keep docs current when behavior, architecture, or project phase changes.
- Add future ideas to the appropriate release section in `docs/roadmap.md` instead of expanding the current scope.
- For UI work, preserve the existing design language unless the task is to redesign it.
- For frontend styling, define reusable fonts, colors, spacing, borders, and interaction states as Tailwind/CSS theme variables. Avoid hard-coded Tailwind values when a reusable token is appropriate.

## Logging Policy

- Follow `docs/coding-conventions.md` as the source of truth for logging policy.
- Use `tracing` as the application logging API.
- User-facing `INFO`, `WARN`, and `ERROR` messages must be complete enough to show directly in the UI.
- Do not duplicate the same fact at multiple layers.

## Rust Test Policy

Rust tests should fit one of these categories:

- Pure unit tests that call functions directly and have no side effects.
- Actor tests that interact through the actor mailbox, `AppEventBus`, and a tracing listener when tracing output is part of the behavior under test.
- Smoke tests through the debug module/app.

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

Use the smallest relevant `make` target while developing, then run broader verification before completion. The root `Makefile` is a thin command index over the Cargo and npm workflows below.

Common root targets:

```bash
make help
make fmt
make lint
make test
make build
make check
```

`make check` runs the standard non-visual CI-style verification: formatting, linting, tests, and builds. It does not run Docker visual checks or the hardware/debug smoke app.

Common development targets:

```bash
make dev
make storybook
make probe ARGS="..."
```

`make dev` starts the Tauri dev server and app. `make storybook` starts Storybook on port 6006. `make probe` runs the LV1 probe CLI and forwards optional `ARGS`.

Debug smoke target:

```bash
make smoke
make smoke VERBOSE=1
```

`make smoke` runs the dev-only Tauri hardware smoke app quietly and requires an LV1-compatible target environment. Use `VERBOSE=1` to stream terminal logs.

After running `make smoke`, always inspect `logs/debug-smoke-report.txt` for the authoritative suite result. The terminal output can be noisy or truncated; do not claim the smoke passed just because the shell command returned or no failure marker appeared in captured output.

Runtime diagnostic logs are JSONL files written under Tauri's app config directory, not the repo `logs/` folder. On macOS, check:

```bash
~/Library/Application Support/com.advancedshowcontrol.app/logs/diagnostics-*.jsonl
~/Library/Application Support/com.advancedshowcontrol.debug/logs/diagnostics-*.jsonl
```

The normal app uses `com.advancedshowcontrol.app`; the debug smoke app uses `com.advancedshowcontrol.debug`.

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

Common frontend checks:

```bash
make ui-fmt
make ui-lint
make ui-typecheck
make ui-build
make ui-test
make ui-storybook-test
make visual-test

npm --prefix ui run format:check
npm --prefix ui run lint
npm --prefix ui run typecheck
npm --prefix ui run build
npm --prefix ui run test
npm --prefix ui run test:storybook
npm --prefix ui run test:visual:ci
```

Frontend check meanings:

- `make ui-fmt` / `npm --prefix ui run format:check` runs Prettier in check mode.
- `make ui-lint` / `npm --prefix ui run lint` runs ESLint.
- `make ui-typecheck` / `npm --prefix ui run typecheck` runs TypeScript with `tsc --noEmit`.
- `make ui-build` / `npm --prefix ui run build` runs the Vite production build.
- `make ui-test` / `npm --prefix ui run test` runs Vitest unit tests.
- `make ui-storybook-test` / `npm --prefix ui run test:storybook` runs Storybook interaction/browser tests through Vitest.
- `make visual-test` / `npm --prefix ui run test:visual:ci` runs Playwright visual regression tests in the Docker visual-test image for CI-compatible screenshots.
- `make visual-update` / `npm --prefix ui run test:visual:update:ci` regenerates Playwright visual snapshots in the Docker visual-test image; use this when UI changes intentionally update screenshots.
- Prefer the `:ci` visual commands over local `npm run test:visual` / `npm run test:visual:update` so screenshot rendering matches CI more closely.

CI runs the checks covered by `make check`: Rust formatting, linting, tests, and build, plus frontend `format:check`, `lint`, `typecheck`, `build`, `test`, and `test:storybook`. CI also runs Docker-backed visual checks on manual workflow dispatch or when visual-relevant files change.

Hook-only targeted checks may run at commit time for staged files. Do not run these manually; let the hooks run them at commit time:

- Rust formatting for staged Rust files.
- Rust clippy for staged Rust files.
- UI Prettier for staged UI files.
- UI ESLint for staged UI files.

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
